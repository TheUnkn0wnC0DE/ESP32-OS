/* ESP32-OS, Browser (Auftrag 13.9; Nutzer 22.11.): Rückfallebene ohne
 * JavaScript.
 *
 * Manche Seiten (Wikipedia mit den MediaWiki-Skripten) werden am Board mit
 * JavaScript nicht oder sehr spät angezeigt, oder ein Skript sprengt die
 * JS-Speichergrenze. Dann lädt der Browser die Seite ohne JavaScript neu:
 *
 * - Liste der Adressen, die in dieser Sitzung ohne JS laden (ohne
 *   Fragment verglichen). Die HTML-Kopie html.c fragt beim Anlegen einer
 *   Seite ns_ohne_js(): dann `enable_scripting = false` – nur für diese
 *   Seite, die nächste Seite läuft wieder mit JS.
 * - Zeitgrenze: Wird eine HTML-Seite nach NS_RUECKFALL_MS (12 s; am PC für
 *   Tests per Umgebungsvariable NS_RUECKFALL_MS) noch nicht angezeigt
 *   (Browserfenster hat noch einen ladenden Inhalt), wird JavaScript für
 *   diese Seite sofort abgeschaltet (ausstehende Skripte laufen nicht mehr)
 *   und die Seite ohne JavaScript neu geladen.
 * - JS-Speichergrenze: js_quickjs.c meldet „out of memory“ → ebenso.
 *   In beiden Fällen kommt die Adresse auf die Liste (Neuladen, Zurück).
 *   B10: Vorher blieb die halb aufgebaute Seite stehen – viele Seiten
 *   verstecken ihren Inhalt, bis ihre Skripte laufen, und blieben leer
 *   (20-Seiten-Test B9: SRF, Spiegel, YouTube, Wikipedia-Hauptseite …).
 *   Das Neuladen passte am Board am 22.11. nicht in den Heap (alte Seite mit
 *   4 MB JS-Laufzeit + neue); seit B10 hat jedes Dokument seine eigene
 *   Laufzeit mit 2 MB Grenze.
 * - Knopf in der Werkzeugleiste (gui.c-Kopie, build.rs): „Ohne JS“ lädt
 *   die angezeigte Seite ohne JavaScript neu (dann auch mit <noscript>),
 *   „Mit JS“ wieder mit.
 *
 * Neu geladen wird über den Planer (nie aus JavaScript oder einem
 * Rückruf des Ladens heraus).
 */
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#include "utils/errors.h"
#include "utils/log.h"
#include "utils/nsoption.h"
#include "utils/nsurl.h"
#include "netsurf/browser_window.h"
#include "netsurf/content.h"
#include "netsurf/misc.h"
#include "content/content.h"
#include "content/hlcache.h"
#include "desktop/browser_private.h"
#include "desktop/gui_internal.h"
#include "html/private.h"
#include "javascript/js.h"

/* gui.c-Kopie: Beschriftung des Knopfs (ohne Neuladen) */
void ns_js_knopf_text(const char *text);

#ifdef ESP_PLATFORM
#include "esp_log.h"
#define MELDEN(...) ESP_LOGW("ns_ohne_js", __VA_ARGS__)
#else
#define MELDEN(...) NSLOG(netsurf, WARNING, __VA_ARGS__)
#endif

#define MAX_SEITEN 32
#define RUECKFALL_MS 12000
/* Zwischen Anhalten und Neuladen: abgebrochene Abrufe laufen aus, NetSurf
 * gibt die alte Seite (DOM, Skripte) frei, bevor die neue Speicher braucht */
#define NEULADEN_NACH_MS 2000

static const nsurl_component OHNE_FRAGMENT = NSURL_SCHEME | NSURL_CREDENTIALS | NSURL_HOST | NSURL_PORT | NSURL_PATH |
					     NSURL_QUERY;

static nsurl *seiten[MAX_SEITEN];
static unsigned n_seiten, aelteste;
static unsigned rueckfaelle;

/* geplantes Neuladen (eines zur Zeit) und geplante Prüfung */
static struct browser_window *lade_bw;
static nsurl *lade_url;
static struct browser_window *pruef_bw;
static int pruef_ms;

static int finden(nsurl *u)
{
	unsigned i;
	for (i = 0; i < n_seiten; i++)
		if (nsurl_compare(seiten[i], u, OHNE_FRAGMENT))
			return (int)i;
	return -1;
}

bool ns_ohne_js(struct nsurl *url)
{
	return url != NULL && finden(url) >= 0;
}

static void aufnehmen(nsurl *u)
{
	if (finden(u) >= 0)
		return;
	if (n_seiten < MAX_SEITEN) {
		seiten[n_seiten++] = nsurl_ref(u);
	} else {
		/* voll: die älteste Adresse ersetzen */
		nsurl_unref(seiten[aelteste]);
		seiten[aelteste] = nsurl_ref(u);
		aelteste = (aelteste + 1) % MAX_SEITEN;
	}
}

static void entfernen(nsurl *u)
{
	int i = finden(u);
	if (i < 0)
		return;
	nsurl_unref(seiten[i]);
	seiten[i] = seiten[--n_seiten];
	seiten[n_seiten] = NULL;
	if (aelteste >= n_seiten)
		aelteste = 0;
}

static void laden(void *p)
{
	struct browser_window *bw = lade_bw;
	nsurl *u = lade_url;
	(void)p;
	lade_bw = NULL;
	lade_url = NULL;
	if (bw == NULL || u == NULL)
		return;
	browser_window_navigate(bw, u, NULL, (enum browser_window_nav_flags)0, NULL, NULL, NULL);
	nsurl_unref(u);
}

static void anhalten(void *p)
{
	struct browser_window *bw = lade_bw;
	(void)p;
	if (bw == NULL)
		return;
	/* den alten Ladevorgang ganz anhalten: seine Abrufe (z. B. grosse
	 * Skriptpakete) und sein JavaScript geben Speicher frei, bevor die
	 * Seite neu geladen wird */
	browser_window_stop(bw);
	guit->misc->schedule(NEULADEN_NACH_MS, laden, NULL);
}

static void neu_laden(struct browser_window *bw, nsurl *u)
{
	if (lade_url != NULL)
		nsurl_unref(lade_url);
	lade_bw = bw;
	lade_url = nsurl_ref(u);
	guit->misc->schedule(-1, laden, NULL);
	guit->misc->schedule(0, anhalten, NULL);
}

/* Interne Seiten (Fehlerseiten about:…, resource:) nie ohne JS neu laden */
static bool intern(nsurl *u)
{
	const char *a = nsurl_access(u);
	return strncmp(a, "about:", 6) == 0 || strncmp(a, "resource:", 9) == 0;
}

/* Adresse der Seite, die das Fenster gerade lädt bzw. zeigt */
static nsurl *seite(struct browser_window *bw)
{
	struct hlcache_handle *h = bw->loading_content != NULL ? bw->loading_content : bw->current_content;
	return h != NULL ? hlcache_handle_get_url(h) : NULL;
}

/* Beschriftung nach der Sprache des Systems (LANGUAGE, wie lesezeichen.c) */
static const char *knopf(bool mit)
{
	const char *s = getenv("LANGUAGE");
	bool de = s != NULL && strncmp(s, "de", 2) == 0;
	if (mit)
		return de ? "Mit JS" : "With JS";
	return de ? "Ohne JS" : "No JS";
}

/* Knopf „Ohne JS“ / „Mit JS“ */
void ns_ohne_js_umschalten(struct browser_window *bw)
{
	nsurl *u;
	if (bw == NULL || (u = seite(bw)) == NULL)
		return;
	if (ns_ohne_js(u)) {
		entfernen(u);
		MELDEN("mit JavaScript neu laden: %s", nsurl_access(u));
	} else {
		aufnehmen(u);
		MELDEN("ohne JavaScript neu laden (Knopf): %s", nsurl_access(u));
	}
	neu_laden(bw, u);
}

const char *ns_ohne_js_knopftext(struct nsurl *url)
{
	return knopf(ns_ohne_js(url));
}

/* JavaScript dieser Seite an Ort und Stelle abschalten */
static void js_aus(html_content *htmlc)
{
	htmlc->enable_scripting = false;
	if (htmlc->jsthread != NULL)
		js_closethread(htmlc->jsthread);
	ns_js_knopf_text(knopf(true));
}

static void pruefen(void *p)
{
	struct browser_window *bw = p;
	struct hlcache_handle *h;
	nsurl *u;
	if (bw != pruef_bw)
		return;
	if (!nsoption_bool(enable_javascript)) {
		pruef_bw = NULL;
		return;
	}
	h = bw->loading_content;
	/* schon angezeigt (kein ladender Inhalt mehr) */
	if (h == NULL) {
		pruef_bw = NULL;
		return;
	}
	/* die Seite selbst ist noch nicht da (Netz langsam – dann hilft das
	 * Abschalten von JavaScript nicht): weiter beobachten */
	if (content_get_type(h) != CONTENT_HTML) {
		guit->misc->schedule(pruef_ms, pruefen, bw);
		return;
	}
	pruef_bw = NULL;
	u = hlcache_handle_get_url(h);
	if (u == NULL || intern(u) || ns_ohne_js(u))
		return;
	rueckfaelle++;
	MELDEN("Seite nach %d ms nicht angezeigt – ohne JavaScript neu laden: %s", pruef_ms, nsurl_access(u));
	aufnehmen(u);
	js_aus((html_content *)hlcache_handle_get_content(h));
	neu_laden(bw, u);
}

/* Beim Beginn jedes Ladens des obersten Fensters (gui.c, Throbber) */
void ns_ohne_js_ladestart(struct browser_window *bw)
{
	const char *e = getenv("NS_RUECKFALL_MS");
	int ms = e != NULL && e[0] != '\0' ? atoi(e) : RUECKFALL_MS;
	if (pruef_bw != NULL)
		guit->misc->schedule(-1, pruefen, pruef_bw);
	pruef_bw = bw;
	pruef_ms = ms < 1 ? 1 : ms;
	guit->misc->schedule(pruef_ms, pruefen, bw);
}

/* Von js_quickjs.c: ein Skript hat die JS-Speichergrenze gesprengt. `bw`
 * ist das Fenster des JS-Threads (htmlc->bw ist während des Parsens noch
 * nicht gesetzt). */
void ns_ohne_js_anfordern(struct browser_window *bw, html_content *htmlc)
{
	nsurl *u;
	if (bw == NULL || htmlc == NULL)
		return;
	u = content_get_url(&htmlc->base);
	if (u == NULL || intern(u) || ns_ohne_js(u))
		return;
	rueckfaelle++;
	MELDEN("JS-Speichergrenze überschritten – ohne JavaScript neu laden: %s", nsurl_access(u));
	aufnehmen(u);
	/* läuft aus dem Skript heraus: js_closethread setzt nur Merker und
	 * hält Zeitgeber/Neu-Layout an, das Skript endet ohnehin mit Fehler;
	 * neu_laden plant nur (Anhalten und Laden kommen über den Planer) */
	js_aus(htmlc);
	neu_laden(bw, u);
}

/* gui.c-Kopie, gui_window_destroy (B8-Review): ein Tab wird geschlossen –
 * geplante Prüfung und geplantes Neuladen dürfen sein Fenster nicht mehr
 * anfassen */
void ns_ohne_js_fenster_weg(struct browser_window *bw)
{
	if (bw == NULL)
		return;
	if (pruef_bw == bw) {
		guit->misc->schedule(-1, pruefen, bw);
		pruef_bw = NULL;
	}
	if (lade_bw == bw) {
		guit->misc->schedule(-1, laden, NULL);
		guit->misc->schedule(-1, anhalten, NULL);
		lade_bw = NULL;
		if (lade_url != NULL) {
			nsurl_unref(lade_url);
			lade_url = NULL;
		}
	}
}

/* Für die Tests: Adresse auf die Liste setzen */
bool ns_ohne_js_test_setzen(const char *adresse)
{
	nsurl *u;
	if (nsurl_create(adresse, &u) != NSERROR_OK)
		return false;
	aufnehmen(u);
	nsurl_unref(u);
	return true;
}

/* Für die Tests: Anzahl automatischer Rückfälle */
unsigned ns_ohne_js_rueckfaelle(void)
{
	return rueckfaelle;
}
