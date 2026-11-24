/* ESP32-OS, B2: Transport am Board (c/transport.h) über esp_http_client.
 *
 * - HTTPS mit Zertifikatsprüfung gegen das Zertifikatsbündel von ESP-IDF
 *   (esp_crt_bundle_attach, nie abgeschaltet); ungültige Zertifikate
 *   lassen die Verbindung scheitern → Fehlerseite.
 * - Zwei feste Arbeiter-Tasks (Stapel im PSRAM, einmal angelegt und dann
 *   immer wieder benutzt) und eine Warteschlange der Anfragen; NetSurf holt
 *   die Ereignisse ohne zu warten ab. Ein Task je Anfrage kostete bei
 *   Seiten mit vielen Bildern (Wikipedia: ~40 Abrufe) zu viel internen RAM
 *   (FreeRTOS-/lwIP-Objekte je Task, Puffer von esp_http_client) – danach
 *   scheiterten DNS und neue Verbindungen. Die Puffer von esp_http_client
 *   bleiben klein (sie liegen meist im knappen internen RAM).
 * - Weiterleitungen folgt NetSurf selbst (disable_auto_redirect).
 * - Speicher aus dem Systemheap (heap_caps), nicht aus dem Heap des
 *   Programms: wird der Browser beendet, läuft eine Anfrage sauber zu Ende
 *   und gibt sich selbst frei (Zählung der Halter).
 * Liegt ausserhalb des Neustart-Abschnitts (netsurf.lf): seine statischen
 * Daten gehören dem System, nicht dem einzelnen Browserlauf.
 */
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "esp_crt_bundle.h"
#include "esp_heap_caps.h"
#include "esp_http_client.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "esp_netif.h"
#include "lwip/dns.h"
#include "lwip/netdb.h"
#include "lwip/sockets.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"

#include "transport.h"

#define PARALLEL 2
#define STAPEL (12 * 1024)
/* BR1: neue Verbindung nur mit dieser Reserve an internem RAM (WLAN,
 * esp_hosted und lwIP brauchen ihn; bei 0 KB bricht die Firmware ab).
 * Sonst bis WARTE_INTERN_US warten, dann Fehler „zu wenig Speicher“. */
#define RESERVE_INTERN (32 * 1024)
#define WARTE_INTERN_US (15 * 1000 * 1000)
#define BLOCK 4096
/* Puffer von esp_http_client: eine Kopfzeile der Antwort muss in PUFFER_RX
 * passen (Wikipedia schickt lange Content-Security-Policy-Zeilen; 2 KB gab
 * „Out of buffer“). Ab 1 KB legt malloc sie in den PSRAM
 * (CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL). */
#define PUFFER_RX 8192
#define PUFFER_TX 4096
/* Fehlerbericht 24.11.: Empfangene Daten warten als 4-KB-Blöcke im PSRAM
 * (System-Heap), bis NetSurf sie abholt – ohne Grenze füllte ein grosser
 * Abruf den PSRAM, dann scheiterten Belegungen anderer Programme (Startmenü)
 * und die Firmware brach ab. Jetzt liest der Arbeiter erst weiter, wenn
 * höchstens WARTEND_MAX Bytes warten und im PSRAM noch PSRAM_RESERVE frei
 * sind. */
#define WARTEND_MAX (512 * 1024)
#define PSRAM_RESERVE (1536 * 1024)

static const char *TAG = "ns_abruf";

struct knoten {
	struct knoten *next;
	struct ns_transport_ereignis e;
	void *speicher; /* text bzw. daten */
};

struct ns_transport {
	struct knoten *erster, *letzter; /* Warteschlange */
	struct knoten *kopf_erster, *kopf_letzter; /* Kopfzeilen bis zum Status */
	struct knoten *zuletzt; /* zuletzt abgeholt (gültig bis zum nächsten) */
	size_t wartend; /* unter `sperre`: Bytes in NS_T_DATEN, noch nicht abgeholt */
	int halter; /* unter `sperre`: Besitzer (NetSurf) und Arbeiter */
	volatile bool abgebrochen;
	struct ns_transport *weiter; /* Warteschlange der Arbeiter */
	char *url, *methode, *kopfzeilen, *agent;
	void *koerper;
	size_t koerper_n;
	/* BR1: Browser-Sitzung des Auftraggebers; der Besitzer hat freigegeben
	 * (unter `sperre`) */
	unsigned sitzung;
	bool besitzer_weg;
	/* Liste aller lebenden Abrufe (unter `sperre`), für den Sitzungswechsel */
	struct ns_transport *alle_vor, *alle_nach;
};

/* Eine Sperre für alle Abrufe: kurzer kritischer Abschnitt (Spinlock) statt
 * Mutex. Der Browser-Faden kann zwangsbeendet (angehalten) werden; hielte
 * er dabei einen Mutex, blieben die Arbeiter für immer stehen. In einem
 * kritischen Abschnitt kann eine Aufgabe nicht angehalten werden. In der
 * Sperre nur Zeiger und Zähler – kein malloc/free, kein Warten. */
static portMUX_TYPE sperre = portMUX_INITIALIZER_UNLOCKED;
static struct ns_transport *alle;

/* BR1: Nummer der laufenden Browser-Sitzung (Systemdaten, überlebt das
 * Programmende). Ein Abruf aus einer früheren Sitzung gehört einem Browser,
 * den es nicht mehr gibt (z. B. zwangsbeendet): nicht mehr laden – sonst
 * belegen seine Verbindungen weiter internen RAM und halten die Arbeiter
 * für die neue Seite auf. */
static volatile unsigned sitzung;

static void zerstoeren(struct ns_transport *t);

/* Halter abgeben (unter `sperre`); true = der letzte, dann ist `t` schon
 * aus der Liste genommen und muss ausserhalb der Sperre zerstört werden */
static bool abgeben_gesperrt(struct ns_transport *t)
{
	if (--t->halter > 0)
		return false;
	if (t->alle_vor)
		t->alle_vor->alle_nach = t->alle_nach;
	else if (alle == t)
		alle = t->alle_nach;
	if (t->alle_nach)
		t->alle_nach->alle_vor = t->alle_vor;
	t->alle_vor = t->alle_nach = NULL;
	return true;
}

void ns_transport_neue_sitzung(void)
{
	/* Abrufe früherer Sitzungen: deren Besitzer (ein beendeter Browser)
	 * gibt nie mehr frei – für ihn abgeben. Laufende beendet der Arbeiter
	 * danach selbst (verwaist), fertige werden gleich zerstört. */
	struct ns_transport *weg = NULL, *t, *n;
	taskENTER_CRITICAL(&sperre);
	sitzung++;
	for (t = alle; t != NULL; t = n) {
		n = t->alle_nach;
		if (t->sitzung != sitzung && !t->besitzer_weg) {
			t->besitzer_weg = true;
			t->abgebrochen = true;
			if (abgeben_gesperrt(t)) {
				t->alle_nach = weg;
				weg = t;
			}
		}
	}
	taskEXIT_CRITICAL(&sperre);
	while (weg != NULL) {
		n = weg->alle_nach;
		zerstoeren(weg);
		weg = n;
	}
}

static bool verwaist(const struct ns_transport *t)
{
	return t->sitzung != sitzung;
}

/* Warteschlange der noch nicht begonnenen Anfragen (unter `sperre`) */
static SemaphoreHandle_t arbeit;
static StaticSemaphore_t arbeit_speicher;
static struct ns_transport *schlange_erster, *schlange_letzter;
static int arbeiter_anzahl;

static void *neu(size_t n)
{
	return heap_caps_calloc(1, n ? n : 1, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
}

static char *kopie(const char *s)
{
	size_t l;
	char *d;
	if (s == NULL)
		return NULL;
	l = strlen(s);
	d = neu(l + 1);
	if (d != NULL)
		memcpy(d, s, l + 1);
	return d;
}

static void knoten_frei(struct knoten *k)
{
	if (k != NULL) {
		heap_caps_free(k->speicher);
		heap_caps_free(k);
	}
}

/* Ereignis anlegen (Text bzw. Daten werden kopiert) */
static struct knoten *knoten_neu(enum ns_transport_art art, long status, const char *text, const void *daten, size_t n)
{
	struct knoten *k = neu(sizeof *k);
	if (k == NULL)
		return NULL;
	k->e.art = art;
	k->e.status = status;
	if (text != NULL) {
		k->speicher = kopie(text);
		k->e.text = k->speicher;
	} else if (daten != NULL && n > 0) {
		k->speicher = neu(n);
		if (k->speicher != NULL)
			memcpy(k->speicher, daten, n);
		k->e.daten = k->speicher;
		k->e.n = k->speicher ? n : 0;
	}
	return k;
}

static void einreihen(struct ns_transport *t, struct knoten *k)
{
	if (k == NULL)
		return;
	taskENTER_CRITICAL(&sperre);
	if (t->letzter)
		t->letzter->next = k;
	else
		t->erster = k;
	t->letzter = k;
	if (k->e.art == NS_T_DATEN)
		t->wartend += k->e.n;
	taskEXIT_CRITICAL(&sperre);
}

static void melden(struct ns_transport *t, enum ns_transport_art art, long status, const char *text, const void *daten,
		   size_t n)
{
	einreihen(t, knoten_neu(art, status, text, daten, n));
}

/* Abruf zerstören (keine Halter mehr, nicht in der Liste) */
static void zerstoeren(struct ns_transport *t)
{
	while (t->erster) {
		struct knoten *k = t->erster;
		t->erster = k->next;
		knoten_frei(k);
	}
	while (t->kopf_erster) {
		struct knoten *k = t->kopf_erster;
		t->kopf_erster = k->next;
		knoten_frei(k);
	}
	knoten_frei(t->zuletzt);
	heap_caps_free(t->url);
	heap_caps_free(t->methode);
	heap_caps_free(t->kopfzeilen);
	heap_caps_free(t->agent);
	heap_caps_free(t->koerper);
	heap_caps_free(t);
}

/* Arbeiter gibt ab */
static void loslassen(struct ns_transport *t)
{
	bool weg;
	taskENTER_CRITICAL(&sperre);
	weg = abgeben_gesperrt(t);
	taskEXIT_CRITICAL(&sperre);
	if (weg)
		zerstoeren(t);
}

/* Kopfzeilen der Antwort sammeln (kommen vor dem Status) */
static esp_err_t ereignis(esp_http_client_event_t *ev)
{
	struct ns_transport *t = ev->user_data;
	if (ev->event_id == HTTP_EVENT_ON_HEADER && ev->header_key && ev->header_value) {
		size_t l = strlen(ev->header_key) + strlen(ev->header_value) + 3;
		char *z = neu(l);
		struct knoten *k;
		if (z == NULL)
			return ESP_OK;
		snprintf(z, l, "%s: %s", ev->header_key, ev->header_value);
		k = knoten_neu(NS_T_KOPF, 0, z, NULL, 0);
		heap_caps_free(z);
		if (k == NULL)
			return ESP_OK;
		if (t->kopf_letzter)
			t->kopf_letzter->next = k;
		else
			t->kopf_erster = k;
		t->kopf_letzter = k;
	}
	return ESP_OK;
}

/* „Name: Wert\r\n…“ → esp_http_client_set_header */
static void kopfzeilen_setzen(esp_http_client_handle_t cl, const char *alle)
{
	const char *z = alle;
	while (z && *z) {
		const char *ende = strstr(z, "\r\n");
		size_t l = ende ? (size_t)(ende - z) : strlen(z);
		char *zeile = neu(l + 1), *dp;
		if (zeile == NULL)
			return;
		memcpy(zeile, z, l);
		dp = strchr(zeile, ':');
		if (dp != NULL) {
			char *wert = dp + 1;
			*dp = '\0';
			while (*wert == ' ')
				wert++;
			esp_http_client_set_header(cl, zeile, wert);
		}
		heap_caps_free(zeile);
		z = ende ? ende + 2 : NULL;
	}
}

/* Warten, bis NetSurf genug abgeholt hat und der PSRAM Luft hat (siehe
 * WARTEND_MAX). false: abgebrochen bzw. verwaist. */
static bool platz_abwarten(struct ns_transport *t)
{
	for (;;) {
		size_t w;
		if (t->abgebrochen || verwaist(t))
			return false;
		taskENTER_CRITICAL(&sperre);
		w = t->wartend;
		taskEXIT_CRITICAL(&sperre);
		if (w <= WARTEND_MAX && heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM) >= PSRAM_RESERVE)
			return true;
		/* nichts wartet mehr, und doch zu wenig PSRAM: weiterlesen, sonst
		 * stünde der Abruf für immer (andere geben frei oder NetSurf meldet
		 * selbst Speichermangel) */
		if (w == 0)
			return true;
		vTaskDelay(pdMS_TO_TICKS(10));
	}
}

/* Rechner und Port aus der URL (http[s]://rechner[:port]/…) */
static bool url_rechner(const char *url, char *rechner, size_t n, int *port)
{
	const char *p = strstr(url, "://");
	size_t l = 0;
	*port = strncmp(url, "https", 5) == 0 ? 443 : 80;
	if (p == NULL)
		return false;
	p += 3;
	/* Benutzerangaben (user:pass@) überspringen */
	{
		const char *at = strchr(p, '@');
		const char *sl = strchr(p, '/');
		if (at != NULL && (sl == NULL || at < sl))
			p = at + 1;
	}
	if (*p == '[')
		return false; /* IPv6-Literal: keine eigene Diagnose */
	while (p[l] != '\0' && p[l] != '/' && p[l] != ':' && p[l] != '?' && p[l] != '#')
		l++;
	if (l == 0 || l >= n)
		return false;
	memcpy(rechner, p, l);
	rechner[l] = '\0';
	if (p[l] == ':')
		*port = atoi(p + l + 1);
	return *port > 0;
}

/* Fehlerbericht 24.11.: Statt „ESP_ERR_HTTP_CONNECT“ sagen, woran es liegt.
 * Nach einer gescheiterten Verbindung der Reihe nach prüfen: Namensauflösung
 * (DNS), TCP-Verbindung zum Server (5 s), sonst liegt es an TLS. */
/* Letzte Erklärung je Rechner (30 s): fehlgeschlagene Bilder und Skripte
 * desselben Rechners prüfen nicht jedes Mal erneut (blockiert sonst die
 * zwei Arbeiter, Review 24.11.) */
static struct {
	char rechner[128];
	char meldung[384];
	int64_t zeit;
} zuletzt_erklaert;

static void verbindung_erklaeren(struct ns_transport *t, char *m, size_t n)
{
	const char *url = t->url;
	char rechner[128];
	char a1[16], a2[16], ziel_text[16];
	int port;
	struct addrinfo hinweis, *ai = NULL;
	struct sockaddr_in ziel;
	esp_netif_ip_info_t ip;
	esp_netif_t *netz = esp_netif_get_handle_from_ifkey("WIFI_STA_DEF");
	char wlan[96] = "";
	int s, fehler = 0;
	if (netz != NULL && esp_netif_get_ip_info(netz, &ip) == ESP_OK) {
		const ip_addr_t *dns = dns_getserver(0);
		char d[48] = "-";
		if (dns != NULL)
			ipaddr_ntoa_r(dns, d, sizeof d);
		snprintf(wlan, sizeof wlan, " (eigene IP %s, Router %s, DNS %s)", esp_ip4addr_ntoa(&ip.ip, a1, sizeof a1),
			 esp_ip4addr_ntoa(&ip.gw, a2, sizeof a2), d);
	}
	if (!url_rechner(url, rechner, sizeof rechner, &port)) {
		snprintf(m, n, "Verbindung gescheitert%s", wlan);
		return;
	}
	taskENTER_CRITICAL(&sperre);
	if (zuletzt_erklaert.zeit != 0 && esp_timer_get_time() - zuletzt_erklaert.zeit < 30 * 1000 * 1000 &&
	    strcmp(zuletzt_erklaert.rechner, rechner) == 0) {
		snprintf(m, n, "%s", zuletzt_erklaert.meldung);
		taskEXIT_CRITICAL(&sperre);
		return;
	}
	taskEXIT_CRITICAL(&sperre);
	if (t->abgebrochen || verwaist(t)) {
		snprintf(m, n, "Abgebrochen");
		return;
	}
	memset(&hinweis, 0, sizeof hinweis);
	hinweis.ai_family = AF_INET;
	hinweis.ai_socktype = SOCK_STREAM;
	if (lwip_getaddrinfo(rechner, NULL, &hinweis, &ai) != 0 || ai == NULL) {
		snprintf(m, n,
			 "Adresse %s nicht gefunden (DNS-Fehler): Der Namensserver antwortet nicht oder kennt den Namen nicht%s. "
			 "Hat dieses WLAN Internet?",
			 rechner, wlan);
		return;
	}
	memcpy(&ziel, ai->ai_addr, sizeof ziel);
	lwip_freeaddrinfo(ai);
	ziel.sin_port = htons((uint16_t)port);
	inet_ntoa_r(ziel.sin_addr, ziel_text, sizeof ziel_text);
	if (t->abgebrochen || verwaist(t)) {
		snprintf(m, n, "Abgebrochen");
		return;
	}
	s = lwip_socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
	if (s >= 0) {
		struct timeval z = { 5, 0 };
		int fl = lwip_fcntl(s, F_GETFL, 0);
		int rc;
		lwip_fcntl(s, F_SETFL, fl | O_NONBLOCK);
		rc = lwip_connect(s, (struct sockaddr *)&ziel, sizeof ziel);
		if (rc != 0 && errno != EINPROGRESS) {
			fehler = errno != 0 ? errno : EHOSTUNREACH;
		} else if (rc != 0) {
			fd_set w;
			FD_ZERO(&w);
			FD_SET(s, &w);
			if (lwip_select(s + 1, NULL, &w, NULL, &z) == 1) {
				socklen_t l = sizeof fehler;
				lwip_getsockopt(s, SOL_SOCKET, SO_ERROR, &fehler, &l);
			} else {
				fehler = ETIMEDOUT;
			}
		}
		lwip_close(s);
	} else {
		fehler = ENOMEM;
	}
	if (fehler == ECONNREFUSED) {
		snprintf(m, n, "Der Server %s (%s) lehnt die Verbindung auf Port %d ab.", rechner, ziel_text, port);
	} else if (fehler != 0) {
		snprintf(m, n,
			 "Keine Verbindung ins Internet: %s (%s) antwortet nicht. Das WLAN ist verbunden%s, lässt aber "
			 "nichts durch – Anmeldeseite des WLANs (Hotel, Zug, Café) oder Router ohne Internet?",
			 rechner, ziel_text, wlan);
	} else if (port == 443 || strncmp(url, "https", 5) == 0) {
		snprintf(m, n,
			 "Sichere Verbindung zu %s gescheitert (TLS): Zertifikat ungültig, abgelaufen oder die Uhrzeit "
			 "stimmt nicht.",
			 rechner);
	} else {
		snprintf(m, n, "Der Server %s antwortet nicht wie erwartet.", rechner);
	}
	taskENTER_CRITICAL(&sperre);
	snprintf(zuletzt_erklaert.rechner, sizeof zuletzt_erklaert.rechner, "%s", rechner);
	snprintf(zuletzt_erklaert.meldung, sizeof zuletzt_erklaert.meldung, "%s", m);
	zuletzt_erklaert.zeit = esp_timer_get_time();
	taskEXIT_CRITICAL(&sperre);
}

static void ablauf(struct ns_transport *t)
{
	esp_http_client_config_t c;
	esp_http_client_handle_t cl;
	esp_err_t r;
	char meldung[384];
	uint8_t *puffer;
	size_t geschrieben = 0, gesamt = 0;
	int64_t beginn = esp_timer_get_time();
	int status;
	while (heap_caps_get_free_size(MALLOC_CAP_INTERNAL) < RESERVE_INTERN) {
		if (t->abgebrochen || verwaist(t))
			return;
		if (esp_timer_get_time() - beginn > WARTE_INTERN_US) {
			ESP_LOGW(TAG, "zu wenig interner RAM (%u KB) - nicht verbunden: %s",
				 (unsigned)(heap_caps_get_free_size(MALLOC_CAP_INTERNAL) / 1024), t->url);
			melden(t, NS_T_FEHLER, 0, "Zu wenig Speicher für eine weitere Verbindung", NULL, 0);
			return;
		}
		vTaskDelay(pdMS_TO_TICKS(100));
	}
	memset(&c, 0, sizeof c);
	c.url = t->url;
	c.crt_bundle_attach = esp_crt_bundle_attach;
	c.disable_auto_redirect = true;
	c.user_agent = t->agent;
	c.timeout_ms = 20000;
	c.buffer_size = PUFFER_RX;
	c.buffer_size_tx = PUFFER_TX;
	c.event_handler = ereignis;
	c.user_data = t;
	c.keep_alive_enable = false;
	c.method = strcmp(t->methode, "POST") == 0 ? HTTP_METHOD_POST : HTTP_METHOD_GET;
	cl = esp_http_client_init(&c);
	if (cl == NULL) {
		melden(t, NS_T_FEHLER, 0, "Abruf konnte nicht angelegt werden (Speicher)", NULL, 0);
		return;
	}
	kopfzeilen_setzen(cl, t->kopfzeilen);
	r = esp_http_client_open(cl, (int)t->koerper_n);
	if (r != ESP_OK) {
		/* Verbindung erst aufräumen (interner RAM), dann erklären */
		esp_http_client_cleanup(cl);
		if (t->abgebrochen || verwaist(t))
			return;
		verbindung_erklaeren(t, meldung, sizeof meldung);
		ESP_LOGW(TAG, "%s: %s", esp_err_to_name(r), meldung);
		melden(t, NS_T_FEHLER, 0, meldung, NULL, 0);
		return;
	}
	while (geschrieben < t->koerper_n && !t->abgebrochen) {
		int n = esp_http_client_write(cl, (const char *)t->koerper + geschrieben, (int)(t->koerper_n - geschrieben));
		if (n <= 0)
			break;
		geschrieben += (size_t)n;
	}
	if (esp_http_client_fetch_headers(cl) < 0 && esp_http_client_get_status_code(cl) <= 0) {
		melden(t, NS_T_FEHLER, 0, "Keine Antwort vom Server", NULL, 0);
		esp_http_client_cleanup(cl);
		return;
	}
	status = esp_http_client_get_status_code(cl);
	melden(t, NS_T_STATUS, status, NULL, NULL, 0);
	/* gesammelte Kopfzeilen in die Warteschlange */
	taskENTER_CRITICAL(&sperre);
	if (t->kopf_erster) {
		if (t->letzter)
			t->letzter->next = t->kopf_erster;
		else
			t->erster = t->kopf_erster;
		t->letzter = t->kopf_letzter;
		t->kopf_erster = t->kopf_letzter = NULL;
	}
	taskEXIT_CRITICAL(&sperre);
	melden(t, NS_T_KOPF_ENDE, 0, NULL, NULL, 0);
	puffer = neu(BLOCK);
	if (puffer == NULL) {
		melden(t, NS_T_FEHLER, 0, "Kein Speicher für den Abruf", NULL, 0);
		esp_http_client_cleanup(cl);
		return;
	}
	for (;;) {
		int n;
		if (!platz_abwarten(t))
			break;
		n = esp_http_client_read(cl, (char *)puffer, BLOCK);
		if (n > 0) {
			gesamt += (size_t)n;
			melden(t, NS_T_DATEN, 0, NULL, puffer, (size_t)n);
		} else if (n == 0) {
			if (esp_http_client_is_complete_data_received(cl) || esp_http_client_get_content_length(cl) <= 0)
				break;
			/* Zeitüberschreitung beim Lesen */
			melden(t, NS_T_FEHLER, 0, "Verbindung abgebrochen (Zeitüberschreitung)", NULL, 0);
			heap_caps_free(puffer);
			esp_http_client_cleanup(cl);
			return;
		} else {
			melden(t, NS_T_FEHLER, 0, "Fehler beim Lesen der Antwort", NULL, 0);
			heap_caps_free(puffer);
			esp_http_client_cleanup(cl);
			return;
		}
	}
	heap_caps_free(puffer);
	esp_http_client_close(cl);
	esp_http_client_cleanup(cl);
	ESP_LOGI(TAG, "%d, %u Byte, %lld ms: %.100s", status, (unsigned)gesamt, (long long)((esp_timer_get_time() - beginn) / 1000), t->url);
	melden(t, NS_T_FERTIG, 0, NULL, NULL, 0);
}

/* Arbeiter: läuft für immer, nimmt eine Anfrage nach der anderen */
static void arbeiter(void *arg)
{
	(void)arg;
	for (;;) {
		struct ns_transport *t;
		xSemaphoreTake(arbeit, portMAX_DELAY);
		taskENTER_CRITICAL(&sperre);
		t = schlange_erster;
		if (t != NULL) {
			schlange_erster = t->weiter;
			if (schlange_erster == NULL)
				schlange_letzter = NULL;
		}
		taskEXIT_CRITICAL(&sperre);
		if (t == NULL)
			continue;
		if (!t->abgebrochen && !verwaist(t))
			ablauf(t);
		/* den Halter des Besitzers aus einer früheren Sitzung gibt
		 * ns_transport_neue_sitzung ab */
		loslassen(t);
	}
}

/* Warteschlange und Arbeiter beim ersten Abruf anlegen (aus dem einen
 * Browser-Faden, daher ohne weitere Sperre) */
static bool arbeiter_bereit(void)
{
	if (arbeit == NULL)
		arbeit = xSemaphoreCreateCountingStatic(0x7FFF, 0, &arbeit_speicher);
	while (arbeiter_anzahl < PARALLEL) {
		if (xTaskCreateWithCaps(arbeiter, "ns_abruf", STAPEL, NULL, 5, NULL, MALLOC_CAP_SPIRAM) != pdPASS) {
			ESP_LOGE(TAG, "Arbeiter %d nicht angelegt", arbeiter_anzahl);
			break;
		}
		arbeiter_anzahl++;
	}
	return arbeiter_anzahl > 0;
}

struct ns_transport *ns_transport_starten(const char *url, const char *methode, const char *kopfzeilen,
					  const void *koerper, size_t koerper_n, const char *agent)
{
	struct ns_transport *t;
	if (!arbeiter_bereit())
		return NULL;
	t = neu(sizeof *t);
	if (t == NULL)
		return NULL;
	t->halter = 2; /* NetSurf und Arbeiter */
	t->sitzung = sitzung;
	t->url = kopie(url);
	t->methode = kopie(methode);
	t->kopfzeilen = kopie(kopfzeilen ? kopfzeilen : "");
	t->agent = kopie(agent ? agent : "NetSurf");
	if (koerper_n > 0) {
		t->koerper = neu(koerper_n);
		if (t->koerper)
			memcpy(t->koerper, koerper, koerper_n);
	}
	t->koerper_n = t->koerper ? koerper_n : 0;
	if (!t->url || !t->methode || !t->kopfzeilen || !t->agent) {
		ESP_LOGE(TAG, "Abruf %s nicht gestartet (Speicher)", url);
		zerstoeren(t); /* noch in keiner Liste */
		return NULL;
	}
	taskENTER_CRITICAL(&sperre);
	if (schlange_letzter)
		schlange_letzter->weiter = t;
	else
		schlange_erster = t;
	schlange_letzter = t;
	t->alle_nach = alle;
	if (alle)
		alle->alle_vor = t;
	alle = t;
	taskEXIT_CRITICAL(&sperre);
	xSemaphoreGive(arbeit);
	return t;
}

enum ns_transport_art ns_transport_holen(struct ns_transport *t, struct ns_transport_ereignis *e)
{
	struct knoten *k;
	knoten_frei(t->zuletzt);
	t->zuletzt = NULL;
	taskENTER_CRITICAL(&sperre);
	k = t->erster;
	if (k != NULL) {
		t->erster = k->next;
		if (t->erster == NULL)
			t->letzter = NULL;
		if (k->e.art == NS_T_DATEN)
			t->wartend -= k->e.n;
	}
	taskEXIT_CRITICAL(&sperre);
	if (k == NULL) {
		memset(e, 0, sizeof *e);
		return NS_T_NICHTS;
	}
	t->zuletzt = k;
	*e = k->e;
	return e->art;
}

void ns_transport_freigeben(struct ns_transport *t)
{
	bool weg = false;
	t->abgebrochen = true;
	/* Flag und Zähler zusammen: sonst gäbe der Sitzungswechsel ein zweites
	 * Mal ab */
	taskENTER_CRITICAL(&sperre);
	if (!t->besitzer_weg) {
		t->besitzer_weg = true;
		weg = abgeben_gesperrt(t);
	}
	taskEXIT_CRITICAL(&sperre);
	if (weg)
		zerstoeren(t);
}
