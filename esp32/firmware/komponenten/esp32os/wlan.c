// ESP32-OS: WLAN über den ESP32-C6 (esp_hosted per SDIO, esp_wifi_remote
// stellt die normale esp_wifi-API auf dem P4 bereit). Schmale C-Schnittstelle
// für Rust (src/wlan.rs). Zugangsdaten werden nie ins Log geschrieben.

#include <stdlib.h>
#include <string.h>
#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "esp_wifi_he.h"
#include "esp_timer.h"
#include "esp_netif_sntp.h"
#include "esp_hosted.h"
#include "eh_common_fw_version.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"

static const char *TAG = "wlan";
static esp_netif_t *netz;
static volatile int verbunden;     // 1 = IP erhalten
static volatile int verbindet;     // Verbindungsaufbau läuft
static volatile int letzter_grund; // Grund der letzten Trennung
static volatile int gestartet;
// iTWT (W5): 1 = vereinbart, -1 = abgelehnt, 0 = keine
static volatile int itwt_status;
// Die nächste Trennung haben wir selbst ausgelöst (neues Verbinden, während
// noch verbunden oder ein Versuch lief): nicht als Fehlschlag des neuen
// Auftrags werten – höchstens ein Ereignis, höchstens 3 s danach.
static volatile int eigene_trennung;
static volatile int64_t eigene_trennung_zeit;
// Signal/Kanal/PHY zwischengespeichert (Abfrage geht über SDIO zum C6)
static int64_t ap_zeit;
static int ap_rssi, ap_kanal, ap_phy;
// Ältere C6-Firmware kennt die Abfrage des ausgehandelten PHY-Modus nicht
// (wartet 5 s auf eine Antwort). Erst fragen, wenn die Versionsabfrage
// beantwortet wurde (gleiche RPC-Generation); bis dahin gilt, was der
// Zugangspunkt kann.
static volatile int version_geprueft;
static volatile int phymode_geht;

static void ereignis(void *arg, esp_event_base_t basis, int32_t id, void *daten) {
    if (basis == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        wifi_event_sta_disconnected_t *d = daten;
        if (eigene_trennung && esp_timer_get_time() - eigene_trennung_zeit < 3000000) {
            eigene_trennung = 0;
            ESP_LOGI(TAG, "alte Verbindung beendet (Grund %d)", d->reason);
            return;
        }
        eigene_trennung = 0;
        verbunden = 0;
        verbindet = 0;
        letzter_grund = d->reason;
        ap_zeit = 0;
        ESP_LOGW(TAG, "getrennt, Grund %d", d->reason);
    } else if (basis == WIFI_EVENT && id == WIFI_EVENT_STA_CONNECTED) {
        ESP_LOGI(TAG, "mit dem Zugangspunkt verbunden");
    } else if (basis == WIFI_EVENT && id == WIFI_EVENT_ITWT_SETUP) {
        wifi_event_sta_itwt_setup_t *e = daten;
        itwt_status = e->status == 1 ? 1 : -1;
        // Wachintervall = Mantisse · 2^Exponent µs
        unsigned long invl = (unsigned long)e->config.wake_invl_mant << e->config.wake_invl_expn;
        ESP_LOGI(TAG, "iTWT: Einrichtung %s (Status %d, Grund %d, Fluss %d, Intervall %lu us, Wachdauer %d x 256 us)",
                 e->status == 1 ? "ok" : "abgelehnt", (int)e->status, e->reason, e->config.flow_id, invl,
                 e->config.min_wake_dura);
    } else if (basis == WIFI_EVENT && id == WIFI_EVENT_ITWT_TEARDOWN) {
        itwt_status = 0;
        ESP_LOGI(TAG, "iTWT: abgebaut");
    } else if (basis == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        ip_event_got_ip_t *e = daten;
        ESP_LOGI(TAG, "IP " IPSTR, IP2STR(&e->ip_info.ip));
        verbunden = 1;
        verbindet = 0;
        letzter_grund = 0;
    }
}

// Netz, Ereignisse und WLAN (Station) starten. 0 = ok.
int esp32os_wlan_start(void) {
    if (gestartet) return 0;
    // esp_hosted protokolliert beim Setzen der Konfiguration die SSID
    // (eh_host_wifi.c, Stufe Info) – Zugangsdaten gehören nicht ins Log (W0).
    esp_log_level_set("eh_wifi", ESP_LOG_WARN);
    // Transport zum C6 (SDIO): setzt den C6 zurück und wartet auf ihn
    int h = esp_hosted_init();
    if (h != 0) {
        ESP_LOGE(TAG, "esp_hosted_init: %d", h);
        return ESP_FAIL;
    }
    h = esp_hosted_connect_to_slave();
    if (h != 0) {
        ESP_LOGE(TAG, "esp_hosted_connect_to_slave: %d", h);
        return ESP_FAIL;
    }
    esp_err_t r = esp_netif_init();
    if (r != ESP_OK) return r;
    r = esp_event_loop_create_default();
    if (r != ESP_OK && r != ESP_ERR_INVALID_STATE) return r;
    netz = esp_netif_create_default_wifi_sta();
    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    r = esp_wifi_init(&cfg);
    if (r != ESP_OK) return r;
    esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, ereignis, NULL);
    esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, ereignis, NULL);
    r = esp_wifi_set_storage(WIFI_STORAGE_RAM);
    if (r != ESP_OK) return r;
    r = esp_wifi_set_mode(WIFI_MODE_STA);
    if (r != ESP_OK) return r;
    r = esp_wifi_start();
    if (r == ESP_OK) gestartet = 1;
    return r;
}

// Versionen von Host (esp_hosted auf dem P4) und Koprozessor (C6) ins Log
// und an Rust (W2: Versionsgleichheit). Rückgabe 0 = C6-Version gelesen.
int esp32os_wlan_versionen(uint32_t *host, uint32_t *cp, char *cp_ziel, int ziel_len) {
    *host = (PROJECT_VERSION_MAJOR_1 << 16) | (PROJECT_VERSION_MINOR_1 << 8) | PROJECT_VERSION_PATCH_1;
    esp_hosted_coprocessor_fwver_t v;
    memset(&v, 0, sizeof v);
    int r = esp_hosted_get_coprocessor_fwversion(&v);
    *cp = r == 0 ? ((v.major1 & 0xff) << 16) | ((v.minor1 & 0xff) << 8) | (v.patch1 & 0xff) : 0;
    phymode_geht = r == 0;
    version_geprueft = 1;
    ap_zeit = 0;
    // Ziel steht fest (Board: C6); eine weitere RPC-Abfrage kennt alte
    // C6-Firmware ohnehin nicht.
    if (cp_ziel && ziel_len > 0) {
        strncpy(cp_ziel, CONFIG_ESP_HOSTED_IDF_SLAVE_TARGET, (size_t)ziel_len - 1);
        cp_ziel[ziel_len - 1] = 0;
    }
    ESP_LOGI(TAG, "esp_hosted Host %d.%d.%d, Koprozessor %s Firmware %lu.%lu.%lu (Abfrage %d)",
             PROJECT_VERSION_MAJOR_1, PROJECT_VERSION_MINOR_1, PROJECT_VERSION_PATCH_1,
             cp_ziel && cp_ziel[0] ? cp_ziel : "?", (unsigned long)v.major1, (unsigned long)v.minor1,
             (unsigned long)v.patch1, r);
    return r;
}

// Mit einem Netz verbinden (WPA2/WPA3, PMF wenn möglich). 0 = Auftrag ok.
int esp32os_wlan_verbinden(const char *ssid, const char *passwort) {
    wifi_config_t c;
    memset(&c, 0, sizeof c);
    strncpy((char *)c.sta.ssid, ssid, sizeof c.sta.ssid);
    strncpy((char *)c.sta.password, passwort, sizeof c.sta.password);
    c.sta.threshold.authmode = passwort[0] ? WIFI_AUTH_WPA2_PSK : WIFI_AUTH_OPEN;
    c.sta.pmf_cfg.capable = true;
    c.sta.pmf_cfg.required = false;
    c.sta.sae_pwe_h2e = WPA3_SAE_PWE_BOTH;
    c.sta.scan_method = WIFI_ALL_CHANNEL_SCAN;
    c.sta.sort_method = WIFI_CONNECT_AP_BY_SIGNAL;
    // Ein zweiter Versuch im Treiber (kurze Störungen beim Handshake)
    c.sta.failure_retry_cnt = 1;
    // Zustand zuerst umstellen: wer währenddessen fragt, sieht den neuen
    // Auftrag („verbindet“) und nicht mehr die alte Verbindung
    int vorher = verbunden || verbindet;
    verbindet = 1;
    verbunden = 0;
    letzter_grund = 0;
    ap_zeit = 0;
    if (vorher) {
        eigene_trennung_zeit = esp_timer_get_time();
        eigene_trennung = 1;
        esp_wifi_disconnect();
    }
    esp_err_t r = esp_wifi_set_config(WIFI_IF_STA, &c);
    if (r != ESP_OK) {
        verbindet = 0;
        return r;
    }
    r = esp_wifi_connect();
    if (r != ESP_OK) verbindet = 0;
    return r;
}

int esp32os_wlan_trennen(void) {
    eigene_trennung = 0;
    verbunden = 0;
    verbindet = 0;
    ap_zeit = 0;
    return esp_wifi_disconnect();
}

// Zustand für die Oberfläche. ip = IPv4 (Netzwerk-Bytefolge), rssi in dBm,
// phy: Bit 0 = b, 1 = g, 2 = n, 3 = ax (HE).
typedef struct {
    int bereit;
    int verbindet;
    int verbunden;
    unsigned int ip;
    int rssi;
    int kanal;
    int phy;
    int grund;
    // Netzmaske, Gateway, DNS (Netzwerk-Bytefolge) – Funktion 76 (W9)
    unsigned int maske;
    unsigned int gateway;
    unsigned int dns;
    unsigned char mac[6];
} esp32os_wlan_zustand_t;

static unsigned char sta_mac[6];
static int sta_mac_da;

int esp32os_wlan_zustand(esp32os_wlan_zustand_t *z) {
    memset(z, 0, sizeof *z);
    z->bereit = gestartet;
    z->verbindet = verbindet;
    z->verbunden = verbunden;
    z->grund = letzter_grund;
    // MAC einmal beim C6 abfragen (ändert sich nicht)
    if (gestartet && !sta_mac_da && esp_wifi_get_mac(WIFI_IF_STA, sta_mac) == ESP_OK) sta_mac_da = 1;
    memcpy(z->mac, sta_mac, 6);
    if (!verbunden) return 0;
    // Signal höchstens alle 3 s beim C6 abfragen
    int64_t jetzt = esp_timer_get_time();
    if (ap_zeit == 0 || jetzt - ap_zeit > 3000000) {
        wifi_ap_record_t ap;
        if (esp_wifi_sta_get_ap_info(&ap) == ESP_OK) {
            ap_rssi = ap.rssi;
            ap_kanal = ap.primary;
            // Ausgehandelter Modus (nicht nur, was der Zugangspunkt kann)
            // Bit 7: Modus ausgehandelt (sonst: was der Zugangspunkt kann)
            wifi_phy_mode_t m;
            if (version_geprueft && phymode_geht && esp_wifi_sta_get_negotiated_phymode(&m) == ESP_OK) {
                ap_phy = 0x80 | (m == WIFI_PHY_MODE_HE20 ? 8 : (m == WIFI_PHY_MODE_HT20 || m == WIFI_PHY_MODE_HT40) ? 4 : m == WIFI_PHY_MODE_11G ? 2 : 1);
            } else {
                phymode_geht = 0;
                ap_phy = (ap.phy_11b ? 1 : 0) | (ap.phy_11g ? 2 : 0) | (ap.phy_11n ? 4 : 0) | (ap.phy_11ax ? 8 : 0);
            }
            ap_zeit = jetzt;
        }
    }
    z->rssi = ap_rssi;
    z->kanal = ap_kanal;
    z->phy = ap_phy;
    esp_netif_ip_info_t ip;
    if (netz && esp_netif_get_ip_info(netz, &ip) == ESP_OK) {
        z->ip = ip.ip.addr;
        z->maske = ip.netmask.addr;
        z->gateway = ip.gw.addr;
    }
    esp_netif_dns_info_t dns;
    if (netz && esp_netif_get_dns_info(netz, ESP_NETIF_DNS_MAIN, &dns) == ESP_OK) z->dns = dns.ip.u_addr.ip4.addr;
    return 0;
}

// Ein gefundenes Netz.
typedef struct {
    char ssid[33];
    int rssi;
    int kanal;
    int sicherheit; // wifi_auth_mode_t
    int ax;         // 802.11ax (Wi-Fi 6)
} esp32os_wlan_netz_t;

// Netze suchen (blockiert ~2–3 s). Liefert die Anzahl (≤ max) oder −Fehler.
int esp32os_wlan_suchen(esp32os_wlan_netz_t *liste, int max) {
    wifi_scan_config_t s;
    memset(&s, 0, sizeof s);
    s.show_hidden = false;
    esp_err_t r = esp_wifi_scan_start(&s, true);
    if (r != ESP_OK) {
        // Funkteil gerade beschäftigt (z. B. Verbindungsaufbau): einmal nachfassen
        vTaskDelay(pdMS_TO_TICKS(1000));
        r = esp_wifi_scan_start(&s, true);
    }
    if (r != ESP_OK) return -r;
    uint16_t n = (uint16_t)max;
    wifi_ap_record_t *aps = calloc(n ? n : 1, sizeof *aps);
    if (!aps) return -ESP_ERR_NO_MEM;
    r = esp_wifi_scan_get_ap_records(&n, aps);
    if (r != ESP_OK) {
        free(aps);
        return -r;
    }
    for (int i = 0; i < n; i++) {
        memset(&liste[i], 0, sizeof liste[i]);
        memcpy(liste[i].ssid, aps[i].ssid, 32);
        liste[i].rssi = aps[i].rssi;
        liste[i].kanal = aps[i].primary;
        liste[i].sicherheit = aps[i].authmode;
        liste[i].ax = aps[i].phy_11ax;
    }
    free(aps);
    return n;
}

// ---- Internetzeit (W3b): SNTP über lwIP; die Systemuhr stellt SNTP selbst,
// Rust erfährt den Abgleich über esp32os_ntp_gestellt().
static volatile int ntp_neu;
static int ntp_laeuft;

static void ntp_abgleich(struct timeval *tv) {
    (void)tv;
    ntp_neu = 1;
}

int esp32os_ntp_start(void) {
    if (ntp_laeuft) return 0;
    esp_sntp_config_t c = ESP_NETIF_SNTP_DEFAULT_CONFIG("pool.ntp.org");
    c.sync_cb = ntp_abgleich;
    esp_err_t r = esp_netif_sntp_init(&c);
    if (r == ESP_OK) ntp_laeuft = 1;
    return r;
}

void esp32os_ntp_stopp(void) {
    if (!ntp_laeuft) return;
    esp_netif_sntp_deinit();
    ntp_laeuft = 0;
}

// 1 = seit der letzten Abfrage neu gestellt.
int esp32os_ntp_gestellt(void) {
    int r = ntp_neu;
    ntp_neu = 0;
    return r;
}

// ---- Stromsparen (W5): bei dunklem Bildschirm iTWT mit dem Zugangspunkt
// vereinbaren (Wi-Fi 6: der C6 wacht nur zu verabredeten Zeiten auf),
// beim Aufwachen wieder abbauen. Werte wie im ESP-IDF-Beispiel: Intervall
// 512 · 2^10 µs ≈ 0,5 s, Wachdauer 255 · 256 µs ≈ 65 ms. Ergebnis kommt als
// Ereignis (siehe oben). Rückgabe: Fehlercode des Auftrags.
int esp32os_wlan_sparen(int an) {
    if (!verbunden) return ESP_ERR_WIFI_NOT_CONNECT;
    if (an) {
        wifi_itwt_setup_config_t c;
        memset(&c, 0, sizeof c);
        c.setup_cmd = TWT_REQUEST;
        c.trigger = 1;
        c.flow_type = 0;
        c.flow_id = 0;
        c.wake_invl_expn = 10;
        c.wake_duration_unit = 0;
        c.min_wake_dura = 255;
        c.wake_invl_mant = 512;
        c.twt_id = 0;
        c.timeout_time_ms = 5000;
        return esp_wifi_sta_itwt_setup(&c);
    }
    if (itwt_status != 1) return ESP_OK;
    return esp_wifi_sta_itwt_teardown(FLOW_ID_ALL);
}

int esp32os_wlan_itwt_status(void) {
    return itwt_status;
}
