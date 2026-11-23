/* ESP32-OS, Browser B4 (Auftrag 13.9): Lesezeichen, Verlauf und die Daten
 * des Browsers im Datenordner.
 *
 * - Datenordner: am Board /daten/netsurf (FAT im internen Flash über das
 *   ESP-IDF-Dateisystem, mit der Flash-Sperre), am PC (Tests) die
 *   Umgebungsvariable NS_DATEN_ORDNER; ohne Ordner bleibt alles im Speicher.
 * - Lesezeichen: NetSurfs Hotlist (Datei „Lesezeichen“, NetSurfs eigenes
 *   HTML-Format; beim ersten Start eine leere Liste statt NetSurfs
 *   Beispiel-Einträgen). Knopf „Merken“ in der Werkzeugleiste fügt die
 *   angezeigte Seite hinzu, „Gemerkt“ nimmt sie wieder heraus.
 * - Verlauf: NetSurf zeichnet jeden Besuch in seiner Adress-Datenbank auf
 *   (Datei „Adressen“: beim Start geladen, beim Öffnen der Liste und beim
 *   Beenden gespeichert). Cookies in der Datei „Cookies“.
 * - Knopf „Liste“: schreibt eine Seite mit grossen, antippbaren Einträgen
 *   – Lesezeichen und Verlauf (neueste zuerst, höchstens 100) – und
 *   öffnet sie.
 */
#include <errno.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#ifdef _WIN32
#include <direct.h>
#endif

#include "utils/errors.h"
#include "utils/log.h"
#include "utils/nsoption.h"
#include "utils/nsurl.h"
#include "netsurf/browser_window.h"
#include "netsurf/content.h"
#include "netsurf/url_db.h"
#include "content/hlcache.h"
#include "desktop/browser_private.h"
#include "desktop/hotlist.h"

#ifdef ESP_PLATFORM
#include "esp_log.h"
#define MELDEN(...) ESP_LOGI("ns_lesezeichen", __VA_ARGS__)
#else
#define MELDEN(...) NSLOG(netsurf, INFO, __VA_ARGS__)
#endif

#define MAX_VERLAUF 100
#define MAX_SAMMELN 400

static char ordner[200];
static bool bereit;

/* gui.c-Kopie: Beschriftung des Knopfs „Merken“ */
void ns_merken_knopf_text(const char *text);

static bool deutsch(void)
{
	const char *s = getenv("LANGUAGE");
	return s != NULL && strncmp(s, "de", 2) == 0;
}

/* Pfad einer Datei im Datenordner; false ohne Ordner */
static bool pfad(char *buf, size_t n, const char *name)
{
	if (ordner[0] == '\0')
		return false;
	snprintf(buf, n, "%s/%s", ordner, name);
	return true;
}

static void ordner_anlegen(const char *p)
{
#ifdef _WIN32
	if (_mkdir(p) != 0 && errno != EEXIST)
#else
	if (mkdir(p, 0755) != 0 && errno != EEXIST)
#endif
		NSLOG(netsurf, WARNING, "Datenordner %s nicht angelegt", p);
}

/* B7 (localStorage, c/js_quickjs.c): Ordner für die Ablage je Herkunft,
 * angelegt; NULL ohne Datenordner. */
const char *ns_speicher_ordner(void)
{
	static char sp[220];
	if (ordner[0] == '\0')
		return NULL;
	snprintf(sp, sizeof sp, "%s/speicher", ordner);
	ordner_anlegen(sp);
	return sp;
}

/* B8 (c/download.c): Ordner für Downloads, angelegt; NULL ohne Datenordner.
 * Am Board neben dem Datenordner (/daten/Downloads), in Tests darin. */
const char *ns_download_ordner(void)
{
	static char dl[220];
	if (ordner[0] == '\0')
		return NULL;
	if (strcmp(ordner, "/daten/netsurf") == 0)
		snprintf(dl, sizeof dl, "/daten/Downloads");
	else
		snprintf(dl, sizeof dl, "%s/Downloads", ordner);
	ordner_anlegen(dl);
	return dl;
}

/* Vor netsurf_init (gui.c-Kopie): Datenordner bestimmen, Cookie-Dateien */
void ns_daten_optionen(void)
{
	const char *e = getenv("NS_DATEN_ORDNER");
	char p[256];
	if (e != NULL && e[0] != '\0')
		snprintf(ordner, sizeof ordner, "%s", e);
#ifdef ESP_PLATFORM
	else
		snprintf(ordner, sizeof ordner, "/daten/netsurf");
#endif
	if (ordner[0] == '\0')
		return;
	ordner_anlegen(ordner);
	if (pfad(p, sizeof p, "Cookies")) {
		nsoption_set_charp(cookie_file, strdup(p));
		nsoption_set_charp(cookie_jar, strdup(p));
	}
}

/* Nach netsurf_init: Adress-Datenbank laden, Lesezeichen anlegen */
void ns_daten_start(void)
{
	char p[256];
	struct stat st;
	nserror e;
	if (pfad(p, sizeof p, "Adressen") && stat(p, &st) == 0)
		urldb_load(p);
	if (pfad(p, sizeof p, "Lesezeichen")) {
		if (stat(p, &st) != 0) {
			/* erste Benutzung: leere Liste (sonst legt NetSurf seine
			 * Beispiel-Lesezeichen an) */
			FILE *f = fopen(p, "w");
			if (f != NULL) {
				fputs("<html><head><title>Lesezeichen</title></head><body>\n<ul>\n</ul>\n</body></html>\n", f);
				fclose(f);
			}
		}
		e = hotlist_init(p, p);
	} else {
		e = hotlist_init(NULL, NULL);
	}
	bereit = e == NSERROR_OK;
	MELDEN("Datenordner %s, Lesezeichen %s (%d)", ordner[0] ? ordner : "(keiner)", bereit ? "bereit" : "FEHLER", (int)e);
}

static void adressen_speichern(void)
{
	char p[256];
	if (pfad(p, sizeof p, "Adressen"))
		urldb_save(p);
}

/* Beim Beenden (gui_quit der gui.c-Kopie) */
void ns_daten_ende(void)
{
	adressen_speichern();
	if (bereit) {
		hotlist_fini();
		bereit = false;
	}
}

/* Adresse der angezeigten bzw. ladenden Seite */
static nsurl *seite(struct browser_window *bw)
{
	struct hlcache_handle *h = bw->current_content != NULL ? bw->current_content : bw->loading_content;
	return h != NULL ? hlcache_handle_get_url(h) : NULL;
}

/* Nur echte Seiten kommen in Lesezeichen und Liste */
static bool merkbar(nsurl *u)
{
	const char *a = nsurl_access(u);
	return strncmp(a, "http://", 7) == 0 || strncmp(a, "https://", 8) == 0;
}

const char *ns_merken_knopftext(struct nsurl *url)
{
	bool gemerkt = bereit && url != NULL && merkbar(url) && hotlist_has_url(url);
	if (deutsch())
		return gemerkt ? "Gemerkt" : "Merken";
	return gemerkt ? "Saved" : "Save";
}

/* Knopf „Merken“: Seite in die Lesezeichen bzw. wieder heraus */
void ns_lesezeichen_umschalten(struct browser_window *bw)
{
	nsurl *u;
	if (!bereit || bw == NULL || (u = seite(bw)) == NULL || !merkbar(u)) {
		MELDEN("Merken: nicht möglich (bereit %d)", bereit);
		return;
	}
	if (hotlist_has_url(u)) {
		hotlist_remove_url(u);
		MELDEN("Lesezeichen entfernt: %s", nsurl_access(u));
	} else {
		nserror e = hotlist_add_url(u);
		MELDEN("Lesezeichen gemerkt: %s (%d)", nsurl_access(u), (int)e);
	}
	ns_merken_knopf_text(ns_merken_knopftext(u));
}

/* --------------------------------------------------- Seite „Liste“ */

static void html_text(FILE *f, const char *s)
{
	for (; s != NULL && *s; s++) {
		switch (*s) {
		case '<': fputs("&lt;", f); break;
		case '>': fputs("&gt;", f); break;
		case '&': fputs("&amp;", f); break;
		case '"': fputs("&quot;", f); break;
		default: fputc(*s, f); break;
		}
	}
}

static void eintrag(FILE *f, nsurl *u, const char *titel, const char *zusatz)
{
	lwc_string *host = nsurl_get_component(u, NSURL_HOST);
	fputs("<a class=\"e\" href=\"", f);
	html_text(f, nsurl_access(u));
	fputs("\">", f);
	html_text(f, titel != NULL && titel[0] != '\0' ? titel : nsurl_access(u));
	fputs("<span class=\"u\">", f);
	if (host != NULL) {
		html_text(f, lwc_string_data(host));
		lwc_string_unref(host);
	}
	if (zusatz != NULL) {
		fputs(" · ", f);
		html_text(f, zusatz);
	}
	fputs("</span></a>\n", f);
}

struct liste_ctx {
	FILE *f;
	unsigned n;
};

static nserror lz_ordner_rein(void *ctx, const char *title)
{
	(void)ctx;
	(void)title;
	return NSERROR_OK;
}

static nserror lz_adresse(void *ctx, struct nsurl *url, const char *title)
{
	struct liste_ctx *l = ctx;
	eintrag(l->f, url, title, NULL);
	l->n++;
	return NSERROR_OK;
}

static nserror lz_ordner_raus(void *ctx)
{
	(void)ctx;
	return NSERROR_OK;
}

struct besuch {
	nsurl *url;
	char *titel;
	time_t zuletzt;
};

static struct besuch *besuche;
static unsigned n_besuche;

static bool besuch_sammeln(struct nsurl *url, const struct url_data *d)
{
	if (d->visits == 0 || !merkbar(url))
		return true;
	if (n_besuche < MAX_SAMMELN) {
		besuche[n_besuche].url = nsurl_ref(url);
		besuche[n_besuche].titel = d->title != NULL ? strdup(d->title) : NULL;
		besuche[n_besuche].zuletzt = d->last_visit;
		n_besuche++;
		return true;
	}
	/* voll (B8-Review): die urldb läuft nach Hosts, nicht nach Zeit – ein
	 * neuerer Besuch ersetzt den ältesten gesammelten */
	{
		unsigned alt = 0;
		for (unsigned i = 1; i < n_besuche; i++)
			if (besuche[i].zuletzt < besuche[alt].zuletzt)
				alt = i;
		if (d->last_visit > besuche[alt].zuletzt) {
			nsurl_unref(besuche[alt].url);
			free(besuche[alt].titel);
			besuche[alt].url = nsurl_ref(url);
			besuche[alt].titel = d->title != NULL ? strdup(d->title) : NULL;
			besuche[alt].zuletzt = d->last_visit;
		}
	}
	return true;
}

static int neueste_zuerst(const void *a, const void *b)
{
	const struct besuch *x = a, *y = b;
	return x->zuletzt < y->zuletzt ? 1 : x->zuletzt > y->zuletzt ? -1 : 0;
}

/* Liste schreiben; Pfad der Datei in `datei` */
static bool liste_schreiben(char *datei, size_t n)
{
	bool de = deutsch();
	struct liste_ctx l;
	unsigned i;
	FILE *f;
	if (!pfad(datei, n, "liste.html"))
		return false;
	f = fopen(datei, "w");
	if (f == NULL)
		return false;
	fputs("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>", f);
	fputs(de ? "Lesezeichen und Verlauf" : "Bookmarks and history", f);
	fputs("</title><style>\n"
	      "body { margin: 0; background: #eef2f7; font-family: sans-serif; color: #223; }\n"
	      "h1 { margin: 0; padding: 10px 14px; background: #3a6ea5; color: white; font-size: 1.4em; }\n"
	      "h2 { margin: 16px 14px 6px 14px; font-size: 1.2em; }\n"
	      "a.e { display: block; margin: 4px 10px; padding: 10px 12px; background: white; "
	      "border: 1px solid #c8d2e0; color: #1a4a8a; text-decoration: none; font-size: 1.15em; }\n"
	      "span.u { display: block; color: #667; font-size: 0.8em; }\n"
	      "p.leer { margin: 6px 14px; color: #667; }\n"
	      "</style></head><body>\n<h1>",
	      f);
	fputs(de ? "Lesezeichen und Verlauf" : "Bookmarks and history", f);
	fputs("</h1>\n<h2>", f);
	fputs(de ? "Lesezeichen" : "Bookmarks", f);
	fputs("</h2>\n", f);
	l.f = f;
	l.n = 0;
	if (bereit)
		hotlist_iterate(&l, lz_ordner_rein, lz_adresse, lz_ordner_raus);
	if (l.n == 0) {
		fputs("<p class=\"leer\">", f);
		fputs(de ? "Noch keine – „Merken“ in der Werkzeugleiste fügt die angezeigte Seite hinzu."
			 : "None yet – “Save” in the toolbar adds the page you are viewing.", f);
		fputs("</p>\n", f);
	}
	fputs("<h2>", f);
	fputs(de ? "Verlauf" : "History", f);
	fputs("</h2>\n", f);
	besuche = calloc(MAX_SAMMELN, sizeof *besuche);
	n_besuche = 0;
	if (besuche != NULL) {
		urldb_iterate_entries(besuch_sammeln);
		qsort(besuche, n_besuche, sizeof *besuche, neueste_zuerst);
		for (i = 0; i < n_besuche; i++) {
			if (i < MAX_VERLAUF) {
				char wann[32];
				struct tm *t = localtime(&besuche[i].zuletzt);
				if (t != NULL)
					strftime(wann, sizeof wann, "%d.%m.%Y %H:%M", t);
				else
					wann[0] = '\0';
				eintrag(f, besuche[i].url, besuche[i].titel, wann);
			}
			nsurl_unref(besuche[i].url);
			free(besuche[i].titel);
		}
		free(besuche);
		besuche = NULL;
	}
	MELDEN("Liste geschrieben: %u Lesezeichen, %u besuchte Seiten", l.n, n_besuche);
	if (n_besuche == 0) {
		fputs("<p class=\"leer\">", f);
		fputs(de ? "Noch keine besuchten Seiten." : "No pages visited yet.", f);
		fputs("</p>\n", f);
	}
	fputs("</body></html>\n", f);
	fclose(f);
	return true;
}

/* Knopf „Liste“: Liste schreiben und öffnen */
void ns_liste_oeffnen(struct browser_window *bw)
{
	char datei[256], adresse[300];
	nsurl *u;
	if (bw == NULL)
		return;
	adressen_speichern();
	if (!liste_schreiben(datei, sizeof datei)) {
		NSLOG(netsurf, WARNING, "Liste nicht geschrieben (kein Datenordner)");
		return;
	}
	/* file:///… (Windows-Pfade mit Laufwerk: file:///C:/…) */
	snprintf(adresse, sizeof adresse, "file://%s%s", datei[0] == '/' ? "" : "/", datei);
	for (char *c = adresse; *c; c++)
		if (*c == '\\')
			*c = '/';
	if (nsurl_create(adresse, &u) == NSERROR_OK) {
		browser_window_navigate(bw, u, NULL, BW_NAVIGATE_HISTORY, NULL, NULL, NULL);
		nsurl_unref(u);
	}
}

/* Für die Tests */
bool ns_lesezeichen_hat(const char *adresse)
{
	nsurl *u;
	bool r;
	if (!bereit || nsurl_create(adresse, &u) != NSERROR_OK)
		return false;
	r = hotlist_has_url(u);
	nsurl_unref(u);
	return r;
}
