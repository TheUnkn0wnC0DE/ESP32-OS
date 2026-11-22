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
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"

#include "transport.h"

#define PARALLEL 2
#define STAPEL (12 * 1024)
#define BLOCK 4096
/* Puffer von esp_http_client: eine Kopfzeile der Antwort muss in PUFFER_RX
 * passen (Wikipedia schickt lange Content-Security-Policy-Zeilen; 2 KB gab
 * „Out of buffer“). Ab 1 KB legt malloc sie in den PSRAM
 * (CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL). */
#define PUFFER_RX 8192
#define PUFFER_TX 4096

static const char *TAG = "ns_abruf";

struct knoten {
	struct knoten *next;
	struct ns_transport_ereignis e;
	void *speicher; /* text bzw. daten */
};

struct ns_transport {
	SemaphoreHandle_t sperre;
	StaticSemaphore_t sperre_speicher;
	struct knoten *erster, *letzter; /* Warteschlange */
	struct knoten *kopf_erster, *kopf_letzter; /* Kopfzeilen bis zum Status */
	struct knoten *zuletzt; /* zuletzt abgeholt (gültig bis zum nächsten) */
	int halter;
	volatile bool abgebrochen;
	struct ns_transport *weiter; /* Warteschlange der Arbeiter */
	char *url, *methode, *kopfzeilen, *agent;
	void *koerper;
	size_t koerper_n;
};

/* Warteschlange der noch nicht begonnenen Anfragen */
static SemaphoreHandle_t arbeit, schlange_sperre;
static StaticSemaphore_t arbeit_speicher, schlange_speicher;
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
	xSemaphoreTake(t->sperre, portMAX_DELAY);
	if (t->letzter)
		t->letzter->next = k;
	else
		t->erster = k;
	t->letzter = k;
	xSemaphoreGive(t->sperre);
}

static void melden(struct ns_transport *t, enum ns_transport_art art, long status, const char *text, const void *daten,
		   size_t n)
{
	einreihen(t, knoten_neu(art, status, text, daten, n));
}

static void loslassen(struct ns_transport *t)
{
	bool weg;
	xSemaphoreTake(t->sperre, portMAX_DELAY);
	weg = --t->halter == 0;
	xSemaphoreGive(t->sperre);
	if (!weg)
		return;
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
	vSemaphoreDelete(t->sperre);
	heap_caps_free(t);
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

static void ablauf(struct ns_transport *t)
{
	esp_http_client_config_t c;
	esp_http_client_handle_t cl;
	esp_err_t r;
	char meldung[160];
	uint8_t *puffer;
	size_t geschrieben = 0, gesamt = 0;
	int64_t beginn = esp_timer_get_time();
	int status;
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
		snprintf(meldung, sizeof meldung, "Verbindung gescheitert (%s): Server nicht erreichbar oder Zertifikat ungültig",
			 esp_err_to_name(r));
		melden(t, NS_T_FEHLER, 0, meldung, NULL, 0);
		esp_http_client_cleanup(cl);
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
	xSemaphoreTake(t->sperre, portMAX_DELAY);
	if (t->kopf_erster) {
		if (t->letzter)
			t->letzter->next = t->kopf_erster;
		else
			t->erster = t->kopf_erster;
		t->letzter = t->kopf_letzter;
		t->kopf_erster = t->kopf_letzter = NULL;
	}
	xSemaphoreGive(t->sperre);
	melden(t, NS_T_KOPF_ENDE, 0, NULL, NULL, 0);
	puffer = neu(BLOCK);
	if (puffer == NULL) {
		melden(t, NS_T_FEHLER, 0, "Kein Speicher für den Abruf", NULL, 0);
		esp_http_client_cleanup(cl);
		return;
	}
	for (;;) {
		int n;
		if (t->abgebrochen)
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
		xSemaphoreTake(schlange_sperre, portMAX_DELAY);
		t = schlange_erster;
		if (t != NULL) {
			schlange_erster = t->weiter;
			if (schlange_erster == NULL)
				schlange_letzter = NULL;
		}
		xSemaphoreGive(schlange_sperre);
		if (t == NULL)
			continue;
		if (!t->abgebrochen)
			ablauf(t);
		loslassen(t);
	}
}

/* Warteschlange und Arbeiter beim ersten Abruf anlegen (aus dem einen
 * Browser-Faden, daher ohne weitere Sperre) */
static bool arbeiter_bereit(void)
{
	if (arbeit == NULL) {
		arbeit = xSemaphoreCreateCountingStatic(0x7FFF, 0, &arbeit_speicher);
		schlange_sperre = xSemaphoreCreateMutexStatic(&schlange_speicher);
	}
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
	t->sperre = xSemaphoreCreateMutexStatic(&t->sperre_speicher);
	t->halter = 2; /* NetSurf und Arbeiter */
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
		t->halter = 1;
		loslassen(t);
		return NULL;
	}
	xSemaphoreTake(schlange_sperre, portMAX_DELAY);
	if (schlange_letzter)
		schlange_letzter->weiter = t;
	else
		schlange_erster = t;
	schlange_letzter = t;
	xSemaphoreGive(schlange_sperre);
	xSemaphoreGive(arbeit);
	return t;
}

enum ns_transport_art ns_transport_holen(struct ns_transport *t, struct ns_transport_ereignis *e)
{
	struct knoten *k;
	knoten_frei(t->zuletzt);
	t->zuletzt = NULL;
	xSemaphoreTake(t->sperre, portMAX_DELAY);
	k = t->erster;
	if (k != NULL) {
		t->erster = k->next;
		if (t->erster == NULL)
			t->letzter = NULL;
	}
	xSemaphoreGive(t->sperre);
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
	t->abgebrochen = true;
	loslassen(t);
}
