/* ESP32-OS, B2: Transport für den HTTP(S)-Abrufer (c/abruf.c).
 *
 * Board: c/transport_esp.c – esp_http_client mit dem Zertifikatsbündel von
 * ESP-IDF, je Anfrage ein eigener Task (NetSurf ist einfädig und darf beim
 * Laden nicht stehen bleiben).
 * PC-Tests: c/transport_pc.c – nur http, zu einem Prüfserver auf dem PC.
 *
 * Eine Anfrage liefert Ereignisse in dieser Reihenfolge: STATUS, KOPF (je
 * Kopfzeile „Name: Wert“), KOPF_ENDE, DATEN (beliebig oft), dann FERTIG –
 * oder jederzeit FEHLER (Text). Text und Daten eines Ereignisses gelten
 * bis zum nächsten Aufruf von ns_transport_holen bzw. bis zur Freigabe.
 */
#ifndef NS_TRANSPORT_H
#define NS_TRANSPORT_H
#include <stddef.h>
#include <stdint.h>

struct ns_transport;

enum ns_transport_art {
	NS_T_NICHTS = 0, /* gerade nichts (Anfrage läuft noch) */
	NS_T_STATUS,	 /* status = HTTP-Status */
	NS_T_KOPF,	 /* text = „Name: Wert“ */
	NS_T_KOPF_ENDE,
	NS_T_DATEN,	 /* daten, n */
	NS_T_FERTIG,
	NS_T_FEHLER	 /* text = Meldung */
};

struct ns_transport_ereignis {
	enum ns_transport_art art;
	long status;
	const char *text;
	const uint8_t *daten;
	size_t n;
};

/* Startet die Anfrage. methode: "GET" oder "POST"; kopfzeilen: zusätzliche
 * Kopfzeilen „Name: Wert\r\n…“ (oder NULL); koerper/koerper_n: Inhalt bei
 * POST; agent: User-Agent. NULL = konnte nicht gestartet werden. */
struct ns_transport *ns_transport_starten(const char *url, const char *methode, const char *kopfzeilen,
					  const void *koerper, size_t koerper_n, const char *agent);

/* Nächstes Ereignis, ohne zu warten. Liefert e->art (NS_T_NICHTS: noch
 * nichts da). */
enum ns_transport_art ns_transport_holen(struct ns_transport *t, struct ns_transport_ereignis *e);

/* Bricht ab (falls die Anfrage noch läuft) und gibt die Anfrage frei. */
void ns_transport_freigeben(struct ns_transport *t);
/* Neue Browser-Sitzung (beim Start): Abrufe früherer Sitzungen verwerfen */
void ns_transport_neue_sitzung(void);

#endif
