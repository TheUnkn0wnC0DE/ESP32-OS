/* ESP32-OS, B2: Abrufer für http und https in NetSurf (statt curl).
 *
 * Bedient NetSurfs Abruf-Schnittstelle (content/fetchers.h) wie der
 * curl-Abrufer: Kopfzeilen einzeln als FETCH_HEADER (zuerst die
 * Statuszeile), dann FETCH_DATA und FETCH_FINISHED; 304 → FETCH_NOTMODIFIED,
 * 3xx mit Location → FETCH_REDIRECT (NetSurf folgt selbst und löst relative
 * Ziele auf), nur-2xx-Abrufe → Fehler „Not2xx“. Cookies: „Cookie:“ aus der
 * Cookie-Ablage von NetSurf (urldb), „Set-Cookie:“ zurück über
 * fetch_set_cookie. POST: urlkodiert oder multipart/form-data (auch
 * Dateien). Die Übertragung macht der Transport (c/transport.h).
 *
 * Eingebunden über fetch_curl_register(): content/fetch.c ruft sie mit
 * WITH_CURL auf (nur diese Datei wird mit WITH_CURL übersetzt, build.rs).
 */
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include <libwapcaplet/libwapcaplet.h>

#include "utils/errors.h"
#include "utils/log.h"
#include "utils/messages.h"
#include "utils/nsurl.h"
#include "utils/corestrings.h"
#include "utils/useragent.h"
#include "content/fetch.h"
#include "content/fetchers.h"
#include "content/urldb.h"

#include "transport.h"

struct abruf {
	struct abruf *next;
	struct fetch *griff;
	nsurl *url;
	lwc_string *schema;
	bool nur_2xx;
	bool abgebrochen;
	bool gesperrt; /* in einem Rückruf (Wiedereintritt) */
	bool fertig;   /* Ende gemeldet, nur noch freigeben */
	bool post;
	char *kopfzeilen;
	char *koerper;
	size_t koerper_n;
	long status;
	char *location;
	struct ns_transport *t;
};

static struct abruf *alle;

/* ------------------------------------------------------------ Hilfen */

/* Zeichenkette ans Ende anhängen (realloc), false bei Speichermangel */
static bool anhaengen(char **s, size_t *n, const char *teil, size_t l)
{
	char *neu = realloc(*s, *n + l + 1);
	if (neu == NULL)
		return false;
	memcpy(neu + *n, teil, l);
	*n += l;
	neu[*n] = '\0';
	*s = neu;
	return true;
}

static bool anhaengen_s(char **s, size_t *n, const char *teil)
{
	return anhaengen(s, n, teil, strlen(teil));
}

/* Inhalt einer lokalen Datei (Datei-Upload) */
static bool datei_anhaengen(char **s, size_t *n, const char *pfad)
{
	FILE *f = fopen(pfad, "rb");
	char puffer[1024];
	size_t l;
	bool ok = true;
	if (f == NULL)
		return true; /* fehlende Datei: leerer Inhalt wie bei curl */
	while (ok && (l = fread(puffer, 1, sizeof puffer, f)) > 0)
		ok = anhaengen(s, n, puffer, l);
	fclose(f);
	return ok;
}

/* multipart/form-data: Körper und Typ mit Grenze */
static bool multipart(const struct fetch_multipart_data *teil, char **koerper, size_t *n, char *kopf, size_t kopf_max)
{
	const char *grenze = "----esp32os-netsurf-7d3a91f0";
	char z[512];
	for (; teil != NULL; teil = teil->next) {
		if (teil->file) {
			snprintf(z, sizeof z,
				 "--%s\r\nContent-Disposition: form-data; name=\"%s\"; filename=\"%s\"\r\n"
				 "Content-Type: application/octet-stream\r\n\r\n",
				 grenze, teil->name, teil->value);
			if (!anhaengen_s(koerper, n, z) || !datei_anhaengen(koerper, n, teil->rawfile))
				return false;
		} else {
			snprintf(z, sizeof z, "--%s\r\nContent-Disposition: form-data; name=\"%s\"\r\n\r\n", grenze, teil->name);
			if (!anhaengen_s(koerper, n, z) || !anhaengen_s(koerper, n, teil->value))
				return false;
		}
		if (!anhaengen_s(koerper, n, "\r\n"))
			return false;
	}
	snprintf(z, sizeof z, "--%s--\r\n", grenze);
	if (!anhaengen_s(koerper, n, z))
		return false;
	snprintf(kopf, kopf_max, "Content-Type: multipart/form-data; boundary=%s\r\n", grenze);
	return true;
}

/* ------------------------------------------------------------ Schnittstelle */

static bool abruf_init(lwc_string *schema)
{
	(void)schema;
	return true;
}

static void abruf_ende_schema(lwc_string *schema)
{
	(void)schema;
}

static bool abruf_annehmbar(const struct nsurl *url)
{
	(void)url;
	return true;
}

static void *abruf_setup(struct fetch *griff, struct nsurl *url, bool nur_2xx, bool downgrade_tls, const char *post_urlenc,
			 const struct fetch_multipart_data *post_multipart, const char **headers)
{
	struct abruf *a = calloc(1, sizeof *a);
	size_t kn = 0;
	char *cookie;
	(void)downgrade_tls;
	if (a == NULL)
		return NULL;
	a->griff = griff;
	a->url = nsurl_ref(url);
	a->schema = nsurl_get_component(url, NSURL_SCHEME);
	a->nur_2xx = nur_2xx;
	/* Kopfzeilen von NetSurf (Accept, If-Modified-Since …) */
	for (; headers != NULL && *headers != NULL; headers++) {
		if (!anhaengen_s(&a->kopfzeilen, &kn, *headers) || !anhaengen_s(&a->kopfzeilen, &kn, "\r\n"))
			goto fehler;
	}
	if (!anhaengen_s(&a->kopfzeilen, &kn, "Accept-Language: de, en;q=0.7\r\n"))
		goto fehler;
	/* Cookies aus der Ablage von NetSurf */
	cookie = urldb_get_cookie(url, true);
	if (cookie != NULL) {
		bool ok = anhaengen_s(&a->kopfzeilen, &kn, "Cookie: ") && anhaengen_s(&a->kopfzeilen, &kn, cookie) &&
			  anhaengen_s(&a->kopfzeilen, &kn, "\r\n");
		free(cookie);
		if (!ok)
			goto fehler;
	}
	if (post_urlenc != NULL) {
		a->post = true;
		a->koerper = strdup(post_urlenc);
		a->koerper_n = strlen(post_urlenc);
		if (a->koerper == NULL ||
		    !anhaengen_s(&a->kopfzeilen, &kn, "Content-Type: application/x-www-form-urlencoded\r\n"))
			goto fehler;
	} else if (post_multipart != NULL) {
		char typ[160];
		a->post = true;
		if (!multipart(post_multipart, &a->koerper, &a->koerper_n, typ, sizeof typ) ||
		    !anhaengen_s(&a->kopfzeilen, &kn, typ))
			goto fehler;
	}
	return a;
fehler:
	free(a->kopfzeilen);
	free(a->koerper);
	lwc_string_unref(a->schema);
	nsurl_unref(a->url);
	free(a);
	return NULL;
}

static bool abruf_start(void *p)
{
	struct abruf *a = p;
	a->t = ns_transport_starten(nsurl_access(a->url), a->post ? "POST" : "GET", a->kopfzeilen, a->koerper,
				    a->koerper_n, user_agent_string());
	if (a->t == NULL)
		return false;
	a->next = alle;
	alle = a;
	return true;
}

static void abruf_abbrechen(void *p)
{
	struct abruf *a = p;
	/* aufräumen im Abfragen (die Liste darf sich im Rückruf nicht ändern) */
	a->abgebrochen = true;
}

static void abruf_freigeben(void *p)
{
	struct abruf *a = p;
	struct abruf **pp;
	for (pp = &alle; *pp != NULL; pp = &(*pp)->next) {
		if (*pp == a) {
			*pp = a->next;
			break;
		}
	}
	if (a->t != NULL)
		ns_transport_freigeben(a->t);
	free(a->kopfzeilen);
	free(a->koerper);
	free(a->location);
	lwc_string_unref(a->schema);
	nsurl_unref(a->url);
	free(a);
}

/* Rückruf an NetSurf; danach kann der Abruf abgebrochen sein */
static void melden(struct abruf *a, fetch_msg *m)
{
	a->gesperrt = true;
	fetch_send_callback(m, a->griff);
	a->gesperrt = false;
}

static void kopf_melden(struct abruf *a, const char *text)
{
	fetch_msg m;
	size_t l = strlen(text);
	m.type = FETCH_HEADER;
	m.data.header_or_data.buf = (const uint8_t *)text;
	m.data.header_or_data.len = l;
	melden(a, &m);
	if (l > 9 && strncasecmp(text, "Location:", 9) == 0) {
		const char *w = text + 9;
		while (*w == ' ' || *w == '\t')
			w++;
		free(a->location);
		a->location = strdup(w);
	} else if (l > 11 && strncasecmp(text, "Set-Cookie:", 11) == 0) {
		const char *w = text + 11;
		while (*w == ' ' || *w == '\t')
			w++;
		fetch_set_cookie(a->griff, w);
	}
}

/* Kopf vollständig: Status auswerten. true = Abruf ist damit erledigt. */
static bool kopf_ende(struct abruf *a)
{
	fetch_msg m;
	if (a->status == 304 && !a->post) {
		m.type = FETCH_NOTMODIFIED;
		melden(a, &m);
		return true;
	}
	if (a->status >= 300 && a->status < 400 && a->location != NULL) {
		m.type = FETCH_REDIRECT;
		m.data.redirect = a->location;
		melden(a, &m);
		return true;
	}
	if (a->nur_2xx && (a->status < 200 || a->status > 299)) {
		m.type = FETCH_ERROR;
		m.data.error = messages_get("Not2xx");
		melden(a, &m);
		return true;
	}
	return false;
}

/* Ereignisse eines Abrufs verarbeiten; true = Abruf beendet */
static bool verarbeiten(struct abruf *a)
{
	struct ns_transport_ereignis e;
	fetch_msg m;
	char statuszeile[48];
	int runden = 0;
	/* je Abfragen höchstens einige Ereignisse: andere Abrufe und das
	 * Zeichnen kommen auch dran */
	while (runden++ < 64 && !a->abgebrochen) {
		switch (ns_transport_holen(a->t, &e)) {
		case NS_T_NICHTS:
			return false;
		case NS_T_STATUS:
			a->status = e.status;
			fetch_set_http_code(a->griff, e.status);
			snprintf(statuszeile, sizeof statuszeile, "HTTP/1.1 %ld", e.status);
			kopf_melden(a, statuszeile);
			break;
		case NS_T_KOPF:
			kopf_melden(a, e.text);
			break;
		case NS_T_KOPF_ENDE:
			if (kopf_ende(a))
				return true;
			break;
		case NS_T_DATEN:
			m.type = FETCH_DATA;
			m.data.header_or_data.buf = e.daten;
			m.data.header_or_data.len = e.n;
			melden(a, &m);
			break;
		case NS_T_FERTIG:
			m.type = FETCH_FINISHED;
			melden(a, &m);
			return true;
		case NS_T_FEHLER:
			m.type = FETCH_ERROR;
			m.data.error = e.text;
			melden(a, &m);
			return true;
		}
	}
	return a->abgebrochen;
}

static void abruf_abfragen(lwc_string *schema)
{
	struct abruf *a, *next;
	bool gleich;
	for (a = alle; a != NULL; a = next) {
		next = a->next;
		if (a->gesperrt || lwc_string_caseless_isequal(a->schema, schema, &gleich) != lwc_error_ok || !gleich)
			continue;
		if (a->fertig || verarbeiten(a)) {
			/* NetSurf gibt den Abruf frei (ruft abruf_freigeben) */
			a->fertig = true;
			next = a->next;
			fetch_remove_from_queues(a->griff);
			fetch_free(a->griff);
		}
	}
}

static const struct fetcher_operation_table ops = {
	.initialise = abruf_init,
	.acceptable = abruf_annehmbar,
	.setup = abruf_setup,
	.start = abruf_start,
	.abort = abruf_abbrechen,
	.free = abruf_freigeben,
	.poll = abruf_abfragen,
	.finalise = abruf_ende_schema,
};

/* Von content/fetch.c (WITH_CURL) beim Start gerufen: http und https */
nserror fetch_curl_register(void)
{
	nserror r = fetcher_add(lwc_string_ref(corestring_lwc_http), &ops);
	if (r != NSERROR_OK)
		return r;
	return fetcher_add(lwc_string_ref(corestring_lwc_https), &ops);
}
