/* ESP32-OS, Browser (B7, Auftrag 13.9): JavaScript für NetSurf mit
 * QuickJS-ng statt Duktape. Bedient NetSurfs Schnittstelle
 * content/handlers/javascript/js.h.
 *
 * Aufbau:
 * - jsheap = eine QuickJS-Laufzeit je Browserfenster mit Speichergrenze
 *   (NS_JS_SPEICHER) und Zeitgrenze je Aufruf (Option script_timeout):
 *   eine Seite bricht ab, Browser und System laufen weiter.
 * - jsthread = ein Kontext je Dokument.
 * - Diese Datei stellt nur Grundfunktionen auf libdom bereit (Objekt
 *   `__ns`): Knoten-Hüllen (eine je Knoten, Identität bleibt), Baum,
 *   Attribute, Text, Fragment-Parsing, Ereignisse über das Ereignissystem
 *   von libdom, Timer über den Planer von NetSurf, Protokoll, Adresse.
 * - Die DOM-Klassen (Node, Element, Document, EventTarget, querySelector,
 *   classList, Timer-API …) stehen in JavaScript in js/vorspann.js.
 */
#include <limits.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <time.h>

#include <dom/dom.h>
#include <dom/bindings/hubbub/parser.h>

#include "quickjs.h"
#ifdef ESP_PLATFORM
/* Board: Meldungen der Skripte und Laufzeiten auf die serielle Konsole
 * (NetSurfs eigenes Protokoll ist am Board ohne -v stumm) */
#include "esp_log.h"
#include "esp_timer.h"
#endif

#include "utils/errors.h"
#include "utils/log.h"
#include "utils/nsoption.h"
#include "utils/nsurl.h"
#include "netsurf/misc.h"
#include "netsurf/browser_window.h"
#include "desktop/gui_internal.h"
#include "content/content.h"
#include "html/private.h"
#include "javascript/js.h"
#include "javascript/content.h"
#include "content/llcache.h"
#include "content/urldb.h"

#include "vorspann.h"

/* Speichergrenze je Browserfenster (Laufzeit) und Stapelgrenze; am Board
 * kleiner (netsurf/build.rs: Heap und Stapel des Programms „browser“) */
#ifndef NS_JS_SPEICHER
#define NS_JS_SPEICHER (16u * 1024u * 1024u)
#endif
#ifndef NS_JS_STAPEL
#define NS_JS_STAPEL (1024u * 1024u)
#endif

struct jsheap {
	JSRuntime *rt;
	int timeout_ms;
	int64_t ende_ms;
	JSClassID knoten_klasse;
	JSClassID ereignis_klasse;
	unsigned threads;
	unsigned tiefe; /* verschachtelte Eintritte (dispatchEvent → Hörer) */
	bool weg; /* js_destroyheap gerufen, letzter Thread gibt frei */
	size_t belegt; /* Bytes der Laufzeit `rt` des Fensters (Q3) */
};

struct eintrag {
	void *schluessel;
	JSValue wert;
};

struct tabelle {
	struct eintrag *e;
	size_t n, belegt;
};

struct zeitgeber;

struct jsthread {
	jsheap *heap;
	/* B10: eigene Laufzeit je Dokument (Speichergrenze je Seite; beim
	 * Schliessen wird alles frei, auch Kreise, die erst die Speicher-
	 * bereinigung fände) – vorher teilten sich alle Dokumente eines
	 * Fensters eine Laufzeit, am Board war sie von vorigen Seiten voll und
	 * die Grundumgebung (window, document) der nächsten liess sich nicht
	 * mehr anlegen („Fehler: null“) */
	JSRuntime *rt;
	size_t belegt; /* Bytes dieser Laufzeit (Q3, Opaque von `rt`) */
	JSContext *cx;
	struct browser_window *bw;
	html_content *htmlc;
	dom_document *doc;
	dom_event_listener *hoerer;
	struct tabelle knoten;
	struct tabelle ereignisse;
	struct zeitgeber *zeitgeber;
	/* laufende fetch/XHR-Abrufe (B7) */
	struct abruf *abrufe;
	bool geschlossen;
	unsigned aenderungen;
	/* Neu-Layout (c/neu_layout.c): Stand der Änderungen und fertig
	 * geladenen Stylesheets beim letzten Aufbau, Prüfung geplant, erstes
	 * Layout begonnen (load) */
	unsigned stand;
	unsigned css_fertig;
	bool geplant;
	bool geladen;
	/* Zeitpunkt der letzten DOM-Änderung (Ruhe abwarten) */
	int64_t letzte_aenderung_ms;
};

bool ns_neu_layout(html_content *htmlc);
bool ns_neu_layout_laeuft(html_content *htmlc);
void ns_neu_layout_abbrechen(html_content *htmlc);
void ns_stylesheets_zaehlen(html_content *htmlc, unsigned *fertig, unsigned *ladend);
/* c/ohne_js.c: Seite ohne JavaScript neu laden (JS-Speichergrenze) */
void ns_ohne_js_anfordern(struct browser_window *bw, html_content *htmlc);
static jsthread *thread_von(JSContext *cx);

/* Neu aufgebaut wird erst, wenn das DOM so lange ruhig war (keine Änderung):
 * Seiten wie Wikipedia ändern nach dem Laden lange Zeit immer wieder etwas;
 * ein Umbau nach jeder Änderung käme am Board nie zur Ruhe (Nutzer 22.11.) */
#define NEU_LAYOUT_MS 500
/* Abstand der Prüfung, solange eingefügte Stylesheets noch laden */
#define CSS_WARTEN_MS 300

struct zeitgeber {
	jsthread *t;
	int32_t id;
	struct zeitgeber *next;
};

/* ------------------------------------------------------------ Protokoll */

/* Für die PC-Tests: letzte Ausgaben von console.* und Fehlern */
static char konsole[8192];
static size_t konsole_n;

static void protokoll(const char *art, const char *text)
{
	size_t l = strlen(art) + strlen(text) + 3;
	NSLOG(netsurf, WARNING, "JS %s: %s", art, text);
#ifdef ESP_PLATFORM
	ESP_LOGW("ns_js", "%s: %.200s", art, text);
#endif
	if (konsole_n + l >= sizeof konsole)
		konsole_n = 0;
	konsole_n += (size_t)snprintf(konsole + konsole_n, sizeof konsole - konsole_n, "%s: %s\n", art, text);
}

const char *ns_js_konsole(void)
{
	return konsole;
}

void ns_js_konsole_leeren(void)
{
	konsole_n = 0;
	konsole[0] = '\0';
}

/* ------------------------------------------------------------ Tabelle */

static size_t streuen(void *p, size_t n)
{
	uintptr_t x = (uintptr_t)p;
	x ^= x >> 17;
	x *= 0x9E3779B1u;
	return (size_t)(x ^ (x >> 13)) & (n - 1);
}

static JSValue *tabelle_finden(struct tabelle *t, void *k)
{
	size_t i;
	if (t->n == 0)
		return NULL;
	for (i = streuen(k, t->n);; i = (i + 1) & (t->n - 1)) {
		if (t->e[i].schluessel == k)
			return &t->e[i].wert;
		if (t->e[i].schluessel == NULL)
			return NULL;
	}
}

static bool tabelle_setzen(struct tabelle *t, void *k, JSValue v);

static bool tabelle_wachsen(struct tabelle *t)
{
	struct tabelle neu = { calloc(t->n ? t->n * 2 : 64, sizeof(struct eintrag)), t->n ? t->n * 2 : 64, 0 };
	size_t i;
	if (neu.e == NULL)
		return false;
	for (i = 0; i < t->n; i++)
		if (t->e[i].schluessel != NULL)
			tabelle_setzen(&neu, t->e[i].schluessel, t->e[i].wert);
	free(t->e);
	*t = neu;
	return true;
}

static bool tabelle_setzen(struct tabelle *t, void *k, JSValue v)
{
	size_t i;
	if ((t->belegt + 1) * 2 > t->n && !tabelle_wachsen(t))
		return false;
	for (i = streuen(k, t->n);; i = (i + 1) & (t->n - 1)) {
		if (t->e[i].schluessel == NULL || t->e[i].schluessel == k) {
			if (t->e[i].schluessel == NULL)
				t->belegt++;
			t->e[i].schluessel = k;
			t->e[i].wert = v;
			return true;
		}
	}
}

/* Eintrag entfernen (lineares Sondieren: Folgeeinträge neu einsortieren) */
static bool tabelle_entfernen(struct tabelle *t, void *k, JSValue *alt)
{
	size_t i, j;
	if (t->n == 0)
		return false;
	for (i = streuen(k, t->n);; i = (i + 1) & (t->n - 1)) {
		if (t->e[i].schluessel == NULL)
			return false;
		if (t->e[i].schluessel == k)
			break;
	}
	*alt = t->e[i].wert;
	t->e[i].schluessel = NULL;
	t->belegt--;
	for (j = (i + 1) & (t->n - 1); t->e[j].schluessel != NULL; j = (j + 1) & (t->n - 1)) {
		struct eintrag e = t->e[j];
		t->e[j].schluessel = NULL;
		t->belegt--;
		tabelle_setzen(t, e.schluessel, e.wert);
	}
	return true;
}

static void tabelle_freigeben(JSContext *cx, struct tabelle *t)
{
	size_t i;
	for (i = 0; i < t->n; i++)
		if (t->e[i].schluessel != NULL)
			JS_FreeValue(cx, t->e[i].wert);
	free(t->e);
	t->e = NULL;
	t->n = t->belegt = 0;
}

/* ------------------------------------------------------------ Zeitgrenze */

/* Millisekunden einer gleichmässigen Uhr (64 Bit: auf 32-Bit-Zielen liefe
 * long über). newlib (Board) hat kein timespec_get, die MSVC-Laufzeit kein
 * clock_gettime. */
static int64_t jetzt_ms(void)
{
	struct timespec t;
#ifdef _WIN32
	timespec_get(&t, TIME_UTC);
#else
	clock_gettime(CLOCK_MONOTONIC, &t);
#endif
	return (int64_t)t.tv_sec * 1000 + t.tv_nsec / 1000000;
}

static int unterbrechen(JSRuntime *rt, void *pw)
{
	jsheap *h = pw;
	(void)rt;
	return h->ende_ms != 0 && jetzt_ms() > h->ende_ms;
}

/* Vor jedem Eintritt in JavaScript; die Zeitgrenze gilt ab dem äussersten */
static void eintreten(jsthread *t)
{
	if (t->heap->tiefe++ == 0)
		t->heap->ende_ms = jetzt_ms() + t->heap->timeout_ms;
}

static void ausnahme_melden(JSContext *cx)
{
	JSValue e = JS_GetException(cx);
	const char *s = JS_ToCString(cx, e);
	JSValue stapel = JS_IsObject(e) ? JS_GetPropertyStr(cx, e, "stack") : JS_UNDEFINED;
	const char *st = JS_IsString(stapel) ? JS_ToCString(cx, stapel) : NULL;
	char text[1024];
	snprintf(text, sizeof text, "%s%s%s", s ? s : "(Ausnahme)", st ? "\n" : "", st ? st : "");
	protokoll("Fehler", text);
	/* Skript hat die Speichergrenze der Seite gesprengt: Rückfallebene,
	 * die Seite ohne JavaScript neu laden (c/ohne_js.c) */
	/* B10: Ist selbst für das Fehlerobjekt kein Speicher mehr da, kommt die
	 * Ausnahme als null an – ebenfalls die Speichergrenze */
	if (JS_IsNull(e) || (s != NULL && strstr(s, "out of memory") != NULL)) {
		jsthread *t = thread_von(cx);
		if (t != NULL && t->htmlc != NULL)
			ns_ohne_js_anfordern(t->bw, t->htmlc);
	}
	if (st)
		JS_FreeCString(cx, st);
	if (s)
		JS_FreeCString(cx, s);
	JS_FreeValue(cx, stapel);
	JS_FreeValue(cx, e);
}

static void layout_pruefen(jsthread *t);

/* Nach jedem Eintritt: beim äussersten Promise-Aufträge abarbeiten */
static void austreten(jsthread *t)
{
	if (--t->heap->tiefe > 0)
		return;
	for (;;) {
		JSContext *c2;
		int r = JS_ExecutePendingJob(t->rt, &c2);
		if (r == 0)
			break;
		if (r < 0)
			ausnahme_melden(c2);
	}
	t->heap->ende_ms = 0;
	layout_pruefen(t);
}

/* --- Neu-Layout nach DOM-Änderungen (Ausführung in c/neu_layout.c) */

static void layout_faellig(void *p);

static void layout_planen(jsthread *t, int ms)
{
	t->geplant = true;
	guit->misc->schedule(ms, layout_faellig, t);
}

static void layout_pruefen(jsthread *t)
{
	unsigned fertig, ladend;
	if (t->geschlossen || t->htmlc == NULL || !t->geladen || t->geplant)
		return;
	ns_stylesheets_zaehlen(t->htmlc, &fertig, &ladend);
	if (t->aenderungen != t->stand || fertig != t->css_fertig)
		layout_planen(t, NEU_LAYOUT_MS);
	else if (ladend > 0)
		layout_planen(t, CSS_WARTEN_MS);
}

static void layout_faellig(void *p)
{
	jsthread *t = p;
	unsigned fertig, ladend, stand;
	t->geplant = false;
	if (t->geschlossen)
		return;
	/* noch keine Ruhe: Rest der Ruhezeit abwarten */
	{
		int64_t still = jetzt_ms() - t->letzte_aenderung_ms;
		if (t->aenderungen != t->stand && still < NEU_LAYOUT_MS) {
			layout_planen(t, (int)(NEU_LAYOUT_MS - still));
			return;
		}
	}
	ns_stylesheets_zaehlen(t->htmlc, &fertig, &ladend);
	if (t->aenderungen == t->stand && fertig == t->css_fertig) {
		if (ladend > 0)
			layout_planen(t, CSS_WARTEN_MS);
		return;
	}
	/* läuft schon ein Umbau, prüft dessen Ende erneut */
	if (ns_neu_layout_laeuft(t->htmlc))
		return;
	stand = t->aenderungen;
	if (ns_neu_layout(t->htmlc)) {
		t->stand = stand;
		t->css_fertig = fertig;
	} else {
		/* gerade nicht möglich (erstes Layout läuft, Seite noch nicht
		 * offen …): später erneut */
		layout_planen(t, NEU_LAYOUT_MS);
	}
}

/* Von c/neu_layout.c nach einem Umbau */
void ns_js_neu_layout_fertig(html_content *htmlc)
{
	if (htmlc != NULL && htmlc->jsthread != NULL)
		layout_pruefen(htmlc->jsthread);
}

static void layout_stoppen(jsthread *t)
{
	if (t->geplant) {
		guit->misc->schedule(-1, layout_faellig, t);
		t->geplant = false;
	}
	if (t->htmlc != NULL)
		ns_neu_layout_abbrechen(t->htmlc);
}

/* Globale Funktion `name` des Vorspanns rufen */
static JSValue rufen(jsthread *t, const char *name, int argc, JSValueConst *argv)
{
	JSValue g = JS_GetGlobalObject(t->cx);
	JSValue f = JS_GetPropertyStr(t->cx, g, name);
	JSValue r = JS_UNDEFINED;
	if (JS_IsFunction(t->cx, f)) {
		r = JS_Call(t->cx, f, g, argc, argv);
		if (JS_IsException(r))
			ausnahme_melden(t->cx);
	}
	JS_FreeValue(t->cx, f);
	JS_FreeValue(t->cx, g);
	return r;
}

/* ------------------------------------------------------------ Hüllen */

static jsthread *thread_von(JSContext *cx)
{
	return JS_GetContextOpaque(cx);
}

static void knoten_endet(JSRuntime *rt, JSValueConst v)
{
	jsheap *h = JS_GetRuntimeOpaque(rt);
	dom_node *n = JS_GetOpaque(v, h->knoten_klasse);
	if (n != NULL)
		dom_node_unref(n);
}

static void ereignis_endet(JSRuntime *rt, JSValueConst v)
{
	jsheap *h = JS_GetRuntimeOpaque(rt);
	dom_event *e = JS_GetOpaque(v, h->ereignis_klasse);
	if (e != NULL)
		dom_event_unref(e);
}

/* Hülle eines Knotens (eine je Knoten, bleibt bis zum Dokumentende) */
static JSValue huelle(jsthread *t, dom_node *n)
{
	JSValue *da, proto, o, arg[2];
	dom_node_type typ;
	dom_string *name = NULL;
	if (n == NULL)
		return JS_NULL;
	da = tabelle_finden(&t->knoten, n);
	if (da != NULL)
		return JS_DupValue(t->cx, *da);
	if (dom_node_get_node_type(n, &typ) != DOM_NO_ERR)
		return JS_NULL;
	dom_node_get_node_name(n, &name);
	arg[0] = JS_NewInt32(t->cx, (int32_t)typ);
	arg[1] = name ? JS_NewStringLen(t->cx, dom_string_data(name), dom_string_byte_length(name)) : JS_NULL;
	if (name)
		dom_string_unref(name);
	proto = rufen(t, "__ns_proto", 2, arg);
	JS_FreeValue(t->cx, arg[1]);
	if (!JS_IsObject(proto)) {
		JS_FreeValue(t->cx, proto);
		return JS_NULL;
	}
	o = JS_NewObjectProtoClass(t->cx, proto, t->heap->knoten_klasse);
	JS_FreeValue(t->cx, proto);
	if (JS_IsException(o))
		return o;
	JS_SetOpaque(o, dom_node_ref(n));
	tabelle_setzen(&t->knoten, n, JS_DupValue(t->cx, o));
	return o;
}

static JSValue ereignis_huelle(jsthread *t, dom_event *e)
{
	JSValue *da, proto, o;
	if (e == NULL)
		return JS_NULL;
	da = tabelle_finden(&t->ereignisse, e);
	if (da != NULL)
		return JS_DupValue(t->cx, *da);
	proto = rufen(t, "__ns_ereignis_proto", 0, NULL);
	o = JS_NewObjectProtoClass(t->cx, proto, t->heap->ereignis_klasse);
	JS_FreeValue(t->cx, proto);
	if (JS_IsException(o))
		return o;
	dom_event_ref(e);
	JS_SetOpaque(o, e);
	tabelle_setzen(&t->ereignisse, e, JS_DupValue(t->cx, o));
	return o;
}

static dom_node *knoten(JSContext *cx, JSValueConst v)
{
	jsthread *t = thread_von(cx);
	return JS_IsObject(v) ? JS_GetOpaque(v, t->heap->knoten_klasse) : NULL;
}

static dom_event *ereignis(JSContext *cx, JSValueConst v)
{
	jsthread *t = thread_von(cx);
	return JS_IsObject(v) ? JS_GetOpaque(v, t->heap->ereignis_klasse) : NULL;
}

static JSValue als_text(JSContext *cx, dom_string *s)
{
	JSValue v;
	if (s == NULL)
		return JS_NULL;
	v = JS_NewStringLen(cx, dom_string_data(s), dom_string_byte_length(s));
	dom_string_unref(s);
	return v;
}

/* JS-Wert → dom_string (NULL bei Fehler) */
static dom_string *zu_dom(JSContext *cx, JSValueConst v)
{
	size_t l;
	const char *s = JS_ToCStringLen(cx, &l, v);
	dom_string *d = NULL;
	if (s == NULL)
		return NULL;
	dom_string_create((const uint8_t *)s, l, &d);
	JS_FreeCString(cx, s);
	return d;
}

static JSValue fehler(JSContext *cx, const char *was, dom_exception e)
{
	return JS_ThrowTypeError(cx, "DOM %s: Fehler %d", was, (int)e);
}

/* DOM wurde geändert (Neu-Layout plant html_relayout_schedule) */
static void geaendert(jsthread *t)
{
	t->aenderungen++;
	t->letzte_aenderung_ms = jetzt_ms();
}

/* ------------------------------------------------------------ Grundfunktionen */

#define ARGS JSContext *cx, JSValueConst this_val, int argc, JSValueConst *argv
#define UNBENUTZT (void)this_val; (void)argc

static JSValue g_typ(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_node_type typ;
	UNBENUTZT;
	if (n == NULL || dom_node_get_node_type(n, &typ) != DOM_NO_ERR)
		return JS_NULL;
	return JS_NewInt32(cx, (int32_t)typ);
}

static JSValue g_name(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s = NULL;
	UNBENUTZT;
	if (n == NULL || dom_node_get_node_name(n, &s) != DOM_NO_ERR)
		return JS_NULL;
	return als_text(cx, s);
}

static JSValue g_wert(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s = NULL;
	UNBENUTZT;
	if (n == NULL || dom_node_get_node_value(n, &s) != DOM_NO_ERR)
		return JS_NULL;
	return als_text(cx, s);
}

static JSValue g_wert_setzen(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s = zu_dom(cx, argv[1]);
	dom_exception e;
	UNBENUTZT;
	if (n == NULL || s == NULL)
		return JS_UNDEFINED;
	e = dom_node_set_node_value(n, s);
	dom_string_unref(s);
	geaendert(thread_von(cx));
	return e == DOM_NO_ERR ? JS_UNDEFINED : fehler(cx, "nodeValue", e);
}

static JSValue g_text(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s = NULL;
	UNBENUTZT;
	if (n == NULL || dom_node_get_text_content(n, &s) != DOM_NO_ERR)
		return JS_NULL;
	return s ? als_text(cx, s) : JS_NewString(cx, "");
}

static JSValue g_text_setzen(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s = zu_dom(cx, argv[1]);
	dom_exception e;
	UNBENUTZT;
	if (n == NULL || s == NULL)
		return JS_UNDEFINED;
	e = dom_node_set_text_content(n, s);
	dom_string_unref(s);
	geaendert(thread_von(cx));
	return e == DOM_NO_ERR ? JS_UNDEFINED : fehler(cx, "textContent", e);
}

/* Nachbarn: 0 Eltern, 1 erstes Kind, 2 letztes Kind, 3 nächstes, 4 voriges */
static JSValue g_nachbar(ARGS)
{
	dom_node *n = knoten(cx, argv[0]), *r = NULL;
	int32_t welcher = 0;
	dom_exception e = DOM_NO_ERR;
	JSValue v;
	UNBENUTZT;
	if (n == NULL)
		return JS_NULL;
	JS_ToInt32(cx, &welcher, argv[1]);
	switch (welcher) {
	case 0: e = dom_node_get_parent_node(n, &r); break;
	case 1: e = dom_node_get_first_child(n, &r); break;
	case 2: e = dom_node_get_last_child(n, &r); break;
	case 3: e = dom_node_get_next_sibling(n, &r); break;
	default: e = dom_node_get_previous_sibling(n, &r); break;
	}
	if (e != DOM_NO_ERR || r == NULL)
		return JS_NULL;
	v = huelle(thread_von(cx), r);
	dom_node_unref(r);
	return v;
}

/* Baumänderung: 0 anhängen(p,c), 1 einfügen(p,c,ref), 2 entfernen(p,c),
 * 3 ersetzen(p,neu,alt) */
static JSValue g_baum(ARGS)
{
	int32_t op = 0;
	dom_node *p = knoten(cx, argv[1]), *c = knoten(cx, argv[2]), *r = NULL, *x = NULL;
	dom_exception e;
	UNBENUTZT;
	JS_ToInt32(cx, &op, argv[0]);
	if (p == NULL || c == NULL)
		return JS_ThrowTypeError(cx, "Knoten erwartet");
	if (argc > 3)
		x = knoten(cx, argv[3]);
	switch (op) {
	case 0: e = dom_node_append_child(p, c, &r); break;
	case 1: e = dom_node_insert_before(p, c, x, &r); break;
	case 2: e = dom_node_remove_child(p, c, &r); break;
	default: e = dom_node_replace_child(p, c, x, &r); break;
	}
	if (r != NULL)
		dom_node_unref(r);
	geaendert(thread_von(cx));
	return e == DOM_NO_ERR ? JS_UNDEFINED : fehler(cx, "Baum", e);
}

static JSValue g_klonen(ARGS)
{
	dom_node *n = knoten(cx, argv[0]), *r = NULL;
	JSValue v;
	UNBENUTZT;
	if (n == NULL || dom_node_clone_node(n, JS_ToBool(cx, argv[1]), &r) != DOM_NO_ERR)
		return JS_NULL;
	v = huelle(thread_von(cx), r);
	dom_node_unref(r);
	return v;
}

static JSValue g_attr(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *k = zu_dom(cx, argv[1]), *w = NULL;
	dom_exception e;
	UNBENUTZT;
	if (n == NULL || k == NULL) {
		if (k)
			dom_string_unref(k);
		return JS_NULL;
	}
	e = dom_element_get_attribute((dom_element *)n, k, &w);
	dom_string_unref(k);
	if (e != DOM_NO_ERR || w == NULL)
		return JS_NULL;
	return als_text(cx, w);
}

static JSValue g_attr_setzen(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *k = zu_dom(cx, argv[1]), *w = NULL;
	dom_exception e = DOM_NO_ERR;
	UNBENUTZT;
	if (n == NULL || k == NULL) {
		if (k)
			dom_string_unref(k);
		return JS_UNDEFINED;
	}
	if (argc > 2 && !JS_IsUndefined(argv[2])) {
		w = zu_dom(cx, argv[2]);
		if (w != NULL) {
			e = dom_element_set_attribute((dom_element *)n, k, w);
			dom_string_unref(w);
		}
	} else {
		e = dom_element_remove_attribute((dom_element *)n, k);
	}
	dom_string_unref(k);
	geaendert(thread_von(cx));
	return e == DOM_NO_ERR ? JS_UNDEFINED : fehler(cx, "Attribut", e);
}

static JSValue g_attrs(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_namednodemap *m = NULL;
	dom_ulong l = 0, i;
	JSValue a;
	UNBENUTZT;
	a = JS_NewArray(cx);
	if (n == NULL || dom_node_get_attributes(n, &m) != DOM_NO_ERR || m == NULL)
		return a;
	dom_namednodemap_get_length(m, &l);
	for (i = 0; i < l; i++) {
		dom_attr *at = NULL;
		dom_string *k = NULL, *w = NULL;
		JSValue paar;
		if (dom_namednodemap_item(m, i, (dom_node **)&at) != DOM_NO_ERR || at == NULL)
			continue;
		dom_attr_get_name(at, &k);
		dom_attr_get_value(at, &w);
		paar = JS_NewArray(cx);
		JS_SetPropertyUint32(cx, paar, 0, als_text(cx, k));
		JS_SetPropertyUint32(cx, paar, 1, w ? als_text(cx, w) : JS_NewString(cx, ""));
		JS_SetPropertyUint32(cx, a, i, paar);
		dom_node_unref(at);
	}
	dom_namednodemap_unref(m);
	return a;
}

static JSValue g_dokument(ARGS)
{
	jsthread *t = thread_von(cx);
	(void)this_val; (void)argc; (void)argv;
	return huelle(t, (dom_node *)t->doc);
}

/* Erzeugen: 0 Element(tag), 1 Text, 2 Kommentar, 3 Fragment */
static JSValue g_erzeugen(ARGS)
{
	jsthread *t = thread_von(cx);
	int32_t art = 0;
	dom_string *s = NULL;
	dom_node *n = NULL;
	dom_exception e;
	JSValue v;
	UNBENUTZT;
	JS_ToInt32(cx, &art, argv[0]);
	if (art != 3) {
		s = zu_dom(cx, argv[1]);
		if (s == NULL)
			return JS_EXCEPTION;
	}
	switch (art) {
	case 0: e = dom_document_create_element(t->doc, s, (dom_element **)&n); break;
	case 1: e = dom_document_create_text_node(t->doc, s, (dom_text **)&n); break;
	case 2: e = dom_document_create_comment(t->doc, s, (dom_comment **)&n); break;
	default: e = dom_document_create_document_fragment(t->doc, (dom_document_fragment **)&n); break;
	}
	if (s)
		dom_string_unref(s);
	if (e != DOM_NO_ERR || n == NULL)
		return fehler(cx, "erzeugen", e);
	v = huelle(t, n);
	dom_node_unref(n);
	return v;
}

static JSValue g_nach_id(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_string *s = zu_dom(cx, argv[0]);
	dom_element *el = NULL;
	JSValue v;
	UNBENUTZT;
	if (s == NULL)
		return JS_NULL;
	dom_document_get_element_by_id(t->doc, s, &el);
	dom_string_unref(s);
	if (el == NULL)
		return JS_NULL;
	v = huelle(t, (dom_node *)el);
	dom_node_unref(el);
	return v;
}

/* Erstes Kind-Element mit dem Namen `name` (HTML-Namen sind gross) */
static dom_node *kind_namens(dom_node *p, const char *name)
{
	dom_node *c = NULL, *n;
	if (p == NULL || dom_node_get_first_child(p, &c) != DOM_NO_ERR)
		return NULL;
	while (c != NULL) {
		dom_string *s = NULL;
		dom_node_type typ;
		bool passt = false;
		if (dom_node_get_node_type(c, &typ) == DOM_NO_ERR && typ == DOM_ELEMENT_NODE &&
		    dom_node_get_node_name(c, &s) == DOM_NO_ERR && s != NULL) {
			passt = dom_string_byte_length(s) == strlen(name) &&
				strncasecmp(dom_string_data(s), name, strlen(name)) == 0;
			dom_string_unref(s);
		}
		if (passt)
			return c;
		n = NULL;
		dom_node_get_next_sibling(c, &n);
		dom_node_unref(c);
		c = n;
	}
	return NULL;
}

/* Alle Kinder von `von` ans Ende von `nach` verschieben */
static void kinder_verschieben(dom_node *von, dom_node *nach)
{
	dom_node *c = NULL, *r;
	while (von != NULL && dom_node_get_first_child(von, &c) == DOM_NO_ERR && c != NULL) {
		r = NULL;
		if (dom_node_append_child(nach, c, &r) != DOM_NO_ERR) {
			dom_node_unref(c);
			break;
		}
		if (r != NULL)
			dom_node_unref(r);
		dom_node_unref(c);
	}
}

/* HTML-Text als Fragment parsen (innerHTML, insertAdjacentHTML).
 * Der Fragment-Parser von libdom baut wie für ein Dokument html/head/body
 * auf; der Inhalt wird daraus in ein neues Fragment geholt. Für Tabellen-
 * und Auswahl-Kontexte (innerHTML einer <tr>, <tbody>, <select>) wird der
 * Text in die passenden Elemente gehüllt, sonst verwürfe der Parser
 * <tr>/<td>/<option> ausserhalb ihres Kontexts. */
static JSValue g_fragment(ARGS)
{
	static const struct {
		const char *kontext, *vor;
		const char *pfad[3];
	} huellen[] = {
		{ "TABLE", "<table>", { "TABLE" } },
		{ "TBODY", "<table><tbody>", { "TABLE", "TBODY" } },
		{ "THEAD", "<table><tbody>", { "TABLE", "TBODY" } },
		{ "TFOOT", "<table><tbody>", { "TABLE", "TBODY" } },
		{ "TR", "<table><tbody><tr>", { "TABLE", "TBODY", "TR" } },
		{ "SELECT", "<select>", { "SELECT" } },
		{ "DATALIST", "<select>", { "SELECT" } },
	};
	jsthread *t = thread_von(cx);
	dom_hubbub_parser_params p;
	dom_hubbub_parser *parser = NULL;
	dom_document_fragment *frag = NULL, *aus = NULL;
	dom_node *html, *body, *ziel;
	size_t l, i, h = sizeof huellen / sizeof huellen[0];
	const char *s, *kontext = NULL;
	JSValue v;
	(void)this_val;
	s = JS_ToCStringLen(cx, &l, argv[0]);
	if (s == NULL)
		return JS_EXCEPTION;
	if (argc > 1 && JS_IsString(argv[1]))
		kontext = JS_ToCString(cx, argv[1]);
	for (i = 0; kontext != NULL && i < sizeof huellen / sizeof huellen[0]; i++)
		if (strcasecmp(kontext, huellen[i].kontext) == 0)
			break;
	if (kontext == NULL || i == sizeof huellen / sizeof huellen[0])
		i = h; /* keine Hülle: Inhalt von head + body */
	if (kontext)
		JS_FreeCString(cx, kontext);
	memset(&p, 0, sizeof p);
	p.enc = "UTF-8";
	p.fix_enc = true;
	p.enable_script = false;
	if (dom_hubbub_fragment_parser_create(&p, t->doc, &parser, &frag) != DOM_HUBBUB_OK) {
		JS_FreeCString(cx, s);
		return JS_ThrowInternalError(cx, "Fragment-Parser");
	}
	if (i < h)
		dom_hubbub_parser_parse_chunk(parser, (const uint8_t *)huellen[i].vor, strlen(huellen[i].vor));
	dom_hubbub_parser_parse_chunk(parser, (const uint8_t *)s, l);
	dom_hubbub_parser_completed(parser);
	dom_hubbub_parser_destroy(parser);
	JS_FreeCString(cx, s);
	if (dom_document_create_document_fragment(t->doc, &aus) != DOM_NO_ERR) {
		dom_node_unref(frag);
		return JS_ThrowOutOfMemory(cx);
	}
	html = kind_namens((dom_node *)frag, "HTML");
	if (i < h) {
		size_t k;
		ziel = kind_namens(html, "BODY");
		for (k = 0; k < 3 && huellen[i].pfad[k] != NULL && ziel != NULL; k++) {
			dom_node *n = kind_namens(ziel, huellen[i].pfad[k]);
			dom_node_unref(ziel);
			ziel = n;
		}
		kinder_verschieben(ziel, (dom_node *)aus);
		if (ziel)
			dom_node_unref(ziel);
	} else {
		dom_node *kopf = kind_namens(html, "HEAD");
		body = kind_namens(html, "BODY");
		/* Elemente wie <style>, <meta> am Anfang legt der Parser in den Kopf */
		kinder_verschieben(kopf, (dom_node *)aus);
		kinder_verschieben(body, (dom_node *)aus);
		if (kopf)
			dom_node_unref(kopf);
		if (body)
			dom_node_unref(body);
	}
	if (html)
		dom_node_unref(html);
	dom_node_unref(frag);
	v = huelle(t, (dom_node *)aus);
	dom_node_unref(aus);
	return v;
}

/* --- Ereignisse über libdom */

static void hoerer_ruft(dom_event *evt, void *pw)
{
	jsthread *t = pw;
	dom_event_target *cur = NULL;
	dom_event_flow_phase phase = 0;
	JSValue arg[3], r;
	if (t->geschlossen || t->cx == NULL)
		return;
	if (dom_event_get_current_target(evt, &cur) != DOM_NO_ERR || cur == NULL)
		return;
	dom_event_get_event_phase(evt, &phase);
	eintreten(t);
	arg[0] = huelle(t, (dom_node *)cur);
	arg[1] = ereignis_huelle(t, evt);
	arg[2] = JS_NewInt32(t->cx, (int32_t)phase);
	r = rufen(t, "__ns_feuern", 3, arg);
	JS_FreeValue(t->cx, r);
	JS_FreeValue(t->cx, arg[0]);
	JS_FreeValue(t->cx, arg[1]);
	dom_node_unref(cur);
	austreten(t);
}

/* Für (Knoten, Typ, Phase) einmal bei libdom anmelden (der Vorspann
 * merkt sich, wofür schon angemeldet ist) */
static JSValue g_anmelden(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_node *n = knoten(cx, argv[0]);
	dom_string *typ = zu_dom(cx, argv[1]);
	dom_exception e;
	UNBENUTZT;
	if (n == NULL || typ == NULL) {
		if (typ)
			dom_string_unref(typ);
		return JS_FALSE;
	}
	e = dom_event_target_add_event_listener(n, typ, t->hoerer, JS_ToBool(cx, argv[2]));
	dom_string_unref(typ);
	return JS_NewBool(cx, e == DOM_NO_ERR);
}

/* Ereignis auslösen (dispatchEvent, click()): Typ, sprudelt, abbrechbar;
 * Rückgabe false, wenn ein Hörer preventDefault gerufen hat. Das
 * Ereignis-Objekt geht dem Vorspann vorher über `__ns_neues_ereignis`
 * zu (eigene Felder wie detail). */
static JSValue g_ausloesen(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_node *n = knoten(cx, argv[0]);
	dom_string *typ = zu_dom(cx, argv[1]);
	dom_event *evt = NULL;
	bool ok = true;
	JSValue h, aus_tabelle;
	UNBENUTZT;
	if (n == NULL || typ == NULL || dom_event_create(&evt) != DOM_NO_ERR) {
		if (typ)
			dom_string_unref(typ);
		return JS_TRUE;
	}
	dom_event_init(evt, typ, JS_ToBool(cx, argv[2]), JS_ToBool(cx, argv[3]));
	dom_string_unref(typ);
	h = ereignis_huelle(t, evt);
	if (argc > 4) {
		JSValue a[2] = { h, argv[4] };
		JS_FreeValue(cx, rufen(t, "__ns_neues_ereignis", 2, a));
	}
	dom_event_target_dispatch_event(n, evt, &ok);
	if (tabelle_entfernen(&t->ereignisse, evt, &aus_tabelle))
		JS_FreeValue(cx, aus_tabelle);
	JS_FreeValue(cx, h);
	dom_event_unref(evt);
	return JS_NewBool(cx, ok);
}

/* Ereignis-Abfragen: 0 Typ, 1 Ziel, 2 aktuelles Ziel, 3 Phase, 4 sprudelt,
 * 5 abbrechbar, 6 abgebrochen, 7 stopPropagation, 8 stopImmediate,
 * 9 preventDefault */
static JSValue g_ereignis(ARGS)
{
	dom_event *e = ereignis(cx, argv[0]);
	int32_t op = 0;
	bool b = false;
	dom_string *s = NULL;
	dom_event_target *et = NULL;
	dom_event_flow_phase ph = 0;
	JSValue v;
	UNBENUTZT;
	if (e == NULL)
		return JS_UNDEFINED;
	JS_ToInt32(cx, &op, argv[1]);
	switch (op) {
	case 0:
		dom_event_get_type(e, &s);
		return als_text(cx, s);
	case 1:
	case 2:
		if (op == 1)
			dom_event_get_target(e, &et);
		else
			dom_event_get_current_target(e, &et);
		if (et == NULL)
			return JS_NULL;
		v = huelle(thread_von(cx), (dom_node *)et);
		dom_node_unref(et);
		return v;
	case 3:
		dom_event_get_event_phase(e, &ph);
		return JS_NewInt32(cx, (int32_t)ph);
	case 4:
		dom_event_get_bubbles(e, &b);
		return JS_NewBool(cx, b);
	case 5:
		dom_event_get_cancelable(e, &b);
		return JS_NewBool(cx, b);
	case 6:
		dom_event_is_default_prevented(e, &b);
		return JS_NewBool(cx, b);
	case 7:
		dom_event_stop_propagation(e);
		return JS_UNDEFINED;
	case 8:
		dom_event_stop_immediate_propagation(e);
		return JS_UNDEFINED;
	default:
		dom_event_prevent_default(e);
		return JS_UNDEFINED;
	}
}

/* --- Timer über den Planer von NetSurf */

static void zeitgeber_ruft(void *p)
{
	struct zeitgeber *z = p, **pp;
	jsthread *t = z->t;
	JSValue a;
	for (pp = &t->zeitgeber; *pp != NULL; pp = &(*pp)->next) {
		if (*pp == z) {
			*pp = z->next;
			break;
		}
	}
	if (!t->geschlossen) {
		eintreten(t);
		a = JS_NewInt32(t->cx, z->id);
		JS_FreeValue(t->cx, rufen(t, "__ns_zeitgeber", 1, &a));
		austreten(t);
	}
	free(z);
}

static JSValue g_zeitgeber(ARGS)
{
	jsthread *t = thread_von(cx);
	int32_t ms = 0, id = 0;
	struct zeitgeber *z;
	UNBENUTZT;
	JS_ToInt32(cx, &ms, argv[0]);
	JS_ToInt32(cx, &id, argv[1]);
	z = malloc(sizeof *z);
	if (z == NULL)
		return JS_ThrowOutOfMemory(cx);
	z->t = t;
	z->id = id;
	z->next = t->zeitgeber;
	t->zeitgeber = z;
	guit->misc->schedule(ms < 0 ? 0 : ms, zeitgeber_ruft, z);
	return JS_UNDEFINED;
}

static JSValue g_zeitgeber_weg(ARGS)
{
	jsthread *t = thread_von(cx);
	int32_t id = 0;
	struct zeitgeber **pp;
	UNBENUTZT;
	JS_ToInt32(cx, &id, argv[0]);
	for (pp = &t->zeitgeber; *pp != NULL; pp = &(*pp)->next) {
		if ((*pp)->id == id) {
			struct zeitgeber *z = *pp;
			*pp = z->next;
			guit->misc->schedule(-1, zeitgeber_ruft, z);
			free(z);
			break;
		}
	}
	return JS_UNDEFINED;
}

/* --- fetch/XMLHttpRequest (B7) über NetSurfs llcache
 *
 * Abgerufen wird wie bei Bildern und Stylesheets (Cookies, HTTPS mit
 * Zertifikatsprüfung, Zwischenspeicher). Die Antwort geht an
 * __ns_abruf(id, status, typ, text, daten, fehler) in vorspann.js.
 * Fremde Herkunft: nur lesbar, wenn Access-Control-Allow-Origin sie
 * erlaubt. NetSurf schickt vorhandene Cookies immer mit – dann gilt wie
 * bei Anfragen mit Anmeldedaten: genau die Herkunft der Seite und
 * Access-Control-Allow-Credentials: true; „*“ nur ohne Cookies. Sonst
 * könnte eine Seite fremde Inhalte mit der Anmeldung des Nutzers lesen. Methoden: GET, HEAD, POST (Rumpf als
 * Text, Content-Type application/x-www-form-urlencoded wie bei NetSurf). */

long ns_llcache_http_code(const llcache_handle *h);

struct abruf {
	jsthread *t;
	int id;
	llcache_handle *h;
	bool kopf;		/* HEAD: Rumpf verwerfen */
	bool mit_cookies;	/* Anfrage trug Cookies (CORS) */
	struct abruf *next;
};

/* Antwort bzw. Fehler an JavaScript; Abruf aus der Liste nehmen */
static void abruf_melden(struct abruf *a, long status, const char *typ, const uint8_t *daten, size_t n, const char *fehler)
{
	jsthread *t = a->t;
	struct abruf **pp;
	for (pp = &t->abrufe; *pp != NULL; pp = &(*pp)->next) {
		if (*pp == a) {
			*pp = a->next;
			break;
		}
	}
	if (!t->geschlossen) {
		JSValue argv[6];
		eintreten(t);
		argv[0] = JS_NewInt32(t->cx, a->id);
		argv[1] = JS_NewInt32(t->cx, (int32_t)status);
		argv[2] = typ ? JS_NewString(t->cx, typ) : JS_NULL;
		argv[3] = daten ? JS_NewStringLen(t->cx, (const char *)daten, n) : JS_NewString(t->cx, "");
		argv[4] = daten ? JS_NewArrayBufferCopy(t->cx, daten, n) : JS_NewArrayBufferCopy(t->cx, (const uint8_t *)"", 0);
		argv[5] = fehler ? JS_NewString(t->cx, fehler) : JS_NULL;
		JS_FreeValue(t->cx, rufen(t, "__ns_abruf", 6, (JSValueConst *)argv));
		for (int i = 0; i < 6; i++)
			JS_FreeValue(t->cx, argv[i]);
		austreten(t);
	}
}

/* Darf die Seite die Antwort lesen? Gleiche Herkunft oder CORS-Kopf */
static bool abruf_erlaubt(jsthread *t, llcache_handle *h, bool mit_cookies)
{
	nsurl *seite = t->htmlc ? content_get_url(&t->htmlc->base) : NULL;
	nsurl *ziel = llcache_handle_get_url(h);
	const char *acao, *acac;
	char *herkunft = NULL;
	size_t l = 0;
	bool ok;
	if (seite == NULL || ziel == NULL)
		return false;
	if (nsurl_compare(seite, ziel, NSURL_SCHEME | NSURL_HOST | NSURL_PORT))
		return true;
	acao = llcache_handle_get_header(h, "Access-Control-Allow-Origin");
	if (acao == NULL)
		return false;
	while (*acao == ' ')
		acao++;
	if (strcmp(acao, "*") == 0)
		return !mit_cookies;
	if (mit_cookies) {
		acac = llcache_handle_get_header(h, "Access-Control-Allow-Credentials");
		while (acac != NULL && *acac == ' ')
			acac++;
		if (acac == NULL || strcmp(acac, "true") != 0)
			return false;
	}
	if (nsurl_get(seite, NSURL_SCHEME | NSURL_HOST | NSURL_PORT, &herkunft, &l) != NSERROR_OK)
		return false;
	/* nsurl_get liefert „https://host/“ – ohne den Schrägstrich vergleichen */
	if (l > 0 && herkunft[l - 1] == '/')
		herkunft[--l] = '\0';
	ok = strcmp(acao, herkunft) == 0;
	free(herkunft);
	return ok;
}

static nserror abruf_ereignis(llcache_handle *h, const llcache_event *e, void *pw)
{
	struct abruf *a = pw;
	switch (e->type) {
	case LLCACHE_EVENT_DONE: {
		size_t n = 0;
		const uint8_t *d = llcache_handle_get_source_data(h, &n);
		if (!abruf_erlaubt(a->t, h, a->mit_cookies)) {
			abruf_melden(a, 0, NULL, NULL, 0, "CORS: fremde Herkunft nicht freigegeben (Access-Control-Allow-Origin/-Credentials)");
		} else {
			/* Status aus dem Objekt (llcache_kopie, build.rs); unbekannt
			 * (z. B. data:-Adresse) gilt als 200 */
			long code = ns_llcache_http_code(h);
			long status = code > 0 ? code : 200;
			abruf_melden(a, status, llcache_handle_get_header(h, "Content-Type"), a->kopf ? NULL : d, a->kopf ? 0 : n, NULL);
		}
		llcache_handle_release(h);
		free(a);
		break;
	}
	case LLCACHE_EVENT_ERROR:
		abruf_melden(a, 0, NULL, NULL, 0, e->data.error.msg ? e->data.error.msg : "Netzwerkfehler");
		llcache_handle_abort(h);
		llcache_handle_release(h);
		free(a);
		break;
	default:
		break;
	}
	return NSERROR_OK;
}

/* N.abruf(id, url, methode, rumpf|null) – Fehler beim Start: Ausnahme */
static JSValue g_abruf(ARGS)
{
	jsthread *t = thread_von(cx);
	int32_t id = 0;
	const char *url = NULL, *methode = NULL, *rumpf = NULL;
	nsurl *basis, *ziel = NULL;
	struct abruf *a = NULL;
	llcache_post_data post;
	nserror err;
	JSValue r = JS_UNDEFINED;
	UNBENUTZT;
	JS_ToInt32(cx, &id, argv[0]);
	url = JS_ToCString(cx, argv[1]);
	methode = JS_ToCString(cx, argv[2]);
	if (!JS_IsNull(argv[3]) && !JS_IsUndefined(argv[3]))
		rumpf = JS_ToCString(cx, argv[3]);
	if (url == NULL || methode == NULL || t->htmlc == NULL) {
		r = JS_ThrowTypeError(cx, "fetch: ungültige Anfrage");
		goto ende;
	}
	if (strcmp(methode, "GET") != 0 && strcmp(methode, "HEAD") != 0 && strcmp(methode, "POST") != 0) {
		r = JS_ThrowTypeError(cx, "fetch: Methode %s nicht unterstützt (GET, HEAD, POST)", methode);
		goto ende;
	}
	basis = t->htmlc->base_url ? t->htmlc->base_url : content_get_url(&t->htmlc->base);
	if (basis == NULL || nsurl_join(basis, url, &ziel) != NSERROR_OK) {
		r = JS_ThrowTypeError(cx, "fetch: ungültige Adresse %s", url);
		goto ende;
	}
	a = calloc(1, sizeof *a);
	if (a == NULL) {
		r = JS_ThrowOutOfMemory(cx);
		goto ende;
	}
	a->t = t;
	a->id = id;
	a->kopf = strcmp(methode, "HEAD") == 0;
	{
		char *c = urldb_get_cookie(ziel, true);
		a->mit_cookies = c != NULL && c[0] != '\0';
		free(c);
	}
	post.type = LLCACHE_POST_URL_ENCODED;
	post.data.urlenc = (char *)(rumpf ? rumpf : "");
	/* ohne LLCACHE_RETRIEVE_NO_ERROR_PAGES: auch 404 usw. kommen als Antwort */
	err = llcache_handle_retrieve(ziel, 0, content_get_url(&t->htmlc->base),
			strcmp(methode, "POST") == 0 ? &post : NULL, abruf_ereignis, a, &a->h);
	if (err != NSERROR_OK) {
		free(a);
		r = JS_ThrowTypeError(cx, "fetch: Abruf von %s nicht möglich", nsurl_access(ziel));
		goto ende;
	}
	a->next = t->abrufe;
	t->abrufe = a;
ende:
	if (ziel)
		nsurl_unref(ziel);
	JS_FreeCString(cx, url);
	JS_FreeCString(cx, methode);
	JS_FreeCString(cx, rumpf);
	return r;
}

/* N.abrufStoppen(id): einen laufenden Abruf abbrechen (AbortController,
 * XMLHttpRequest.abort) – kommt danach nichts mehr an JavaScript */
static JSValue g_abruf_stoppen(ARGS)
{
	jsthread *t = thread_von(cx);
	struct abruf **pp;
	int32_t id = 0;
	UNBENUTZT;
	JS_ToInt32(cx, &id, argv[0]);
	for (pp = &t->abrufe; *pp != NULL; pp = &(*pp)->next) {
		struct abruf *a = *pp;
		if (a->id == id) {
			*pp = a->next;
			llcache_handle_abort(a->h);
			llcache_handle_release(a->h);
			free(a);
			return JS_TRUE;
		}
	}
	return JS_FALSE;
}

/* Seite geschlossen: laufende Abrufe abbrechen */
static void abrufe_stoppen(jsthread *t)
{
	while (t->abrufe != NULL) {
		struct abruf *a = t->abrufe;
		t->abrufe = a->next;
		llcache_handle_abort(a->h);
		llcache_handle_release(a->h);
		free(a);
	}
}

/* --- document.cookie (B7) über NetSurfs Cookie-Ablage (urldb): dieselben
 * Cookies wie bei Abrufen, dauerhaft gespeichert. Nur für http(s)-Seiten
 * – sonst null (vorspann.js
 * nimmt dann eine Ablage nur für die Seite). */

static nsurl *seiten_url_http(jsthread *t)
{
	nsurl *u = t->htmlc ? content_get_url(&t->htmlc->base) : NULL;
	return (u != NULL && strncmp(nsurl_access(u), "http", 4) == 0) ? u : NULL;
}

static JSValue g_cookie(ARGS)
{
	jsthread *t = thread_von(cx);
	nsurl *u = seiten_url_http(t);
	char *c;
	JSValue r;
	(void)this_val; (void)argc; (void)argv;
	if (u == NULL)
		return JS_NULL;
	c = urldb_get_cookie(u, false);
	r = JS_NewString(cx, c ? c : "");
	free(c);
	return r;
}

/* Skripte (Nicht-HTTP-Schnittstelle, RFC 6265 5.3) dürfen nur diese
 * Merkmale setzen; ein Cookie, das Skripten verborgen ist (in der Ablage,
 * aber nicht in document.cookie), dürfen sie nicht überschreiben */
static bool name_in(const char *liste, const char *name, size_t nl)
{
	const char *p = liste;
	while (p != NULL && *p != '\0') {
		while (*p == ' ' || *p == ';')
			p++;
		if (strncmp(p, name, nl) == 0 && p[nl] == '=')
			return true;
		p = strchr(p, ';');
	}
	return false;
}

static bool skript_cookie_erlaubt(const char *s, nsurl *u)
{
	static const char *const merkmale[] = { "expires", "max-age", "domain", "path", "secure", "samesite" };
	const char *p = strchr(s, ';'), *gl = strchr(s, '=');
	char name[256], *alle, *offen;
	size_t nl;
	bool verborgen;
	while (p != NULL) {
		size_t i, l;
		bool bekannt = false;
		p++;
		while (*p == ' ')
			p++;
		l = strcspn(p, "=; ");
		for (i = 0; i < sizeof merkmale / sizeof merkmale[0]; i++)
			if (l == strlen(merkmale[i]) && strncasecmp(p, merkmale[i], l) == 0)
				bekannt = true;
		if (!bekannt && l > 0)
			return false;
		p = strchr(p, ';');
	}
	if (gl == NULL)
		return false;
	while (*s == ' ')
		s++;
	nl = (size_t)(gl - s);
	while (nl > 0 && s[nl - 1] == ' ')
		nl--;
	if (nl == 0 || nl >= sizeof name)
		return false;
	memcpy(name, s, nl);
	name[nl] = '\0';
	alle = urldb_get_cookie(u, true);
	offen = urldb_get_cookie(u, false);
	verborgen = name_in(alle, name, nl) && !name_in(offen, name, nl);
	free(alle);
	free(offen);
	return !verborgen;
}

static JSValue g_cookie_setzen(ARGS)
{
	jsthread *t = thread_von(cx);
	nsurl *u = seiten_url_http(t);
	const char *s;
	bool ok;
	UNBENUTZT;
	if (u == NULL)
		return JS_NULL;
	s = JS_ToCString(cx, argv[0]);
	if (s == NULL)
		return JS_EXCEPTION;
	ok = skript_cookie_erlaubt(s, u) && urldb_set_cookie(s, u, NULL);
	JS_FreeCString(cx, s);
	return JS_NewBool(cx, ok);
}

/* --- localStorage (B7): dauerhaft je Herkunft (Schema, Rechner, Port) als
 * JSON-Datei im Datenordner (c/lesezeichen.c, ns_speicher_ordner); Name =
 * FNV-1a der Herkunft. Höchstens SPEICHER_MAX Byte je Herkunft. Nur für
 * http(s)-Seiten – sonst null (vorspann.js hält die Ablage dann nur für die
 * Seite). */

#define SPEICHER_MAX (512u * 1024u)
const char *ns_speicher_ordner(void);

static bool speicher_datei(jsthread *t, char *buf, size_t n)
{
	nsurl *u = seiten_url_http(t);
	const char *o = ns_speicher_ordner();
	char *herkunft = NULL;
	size_t l = 0;
	uint64_t h = 0xcbf29ce484222325ull;
	if (u == NULL || o == NULL || nsurl_get(u, NSURL_SCHEME | NSURL_HOST | NSURL_PORT, &herkunft, &l) != NSERROR_OK)
		return false;
	for (size_t i = 0; i < l; i++)
		h = (h ^ (uint8_t)herkunft[i]) * 0x100000001b3ull;
	free(herkunft);
	snprintf(buf, n, "%s/%016llx.json", o, (unsigned long long)h);
	return true;
}

static JSValue g_speicher_lesen(ARGS)
{
	jsthread *t = thread_von(cx);
	char pfad[300];
	FILE *f;
	long n;
	char *d;
	JSValue r;
	(void)this_val; (void)argc; (void)argv;
	if (!speicher_datei(t, pfad, sizeof pfad))
		return JS_NULL;
	f = fopen(pfad, "rb");
	if (f == NULL)
		return JS_NewString(cx, "");
	fseek(f, 0, SEEK_END);
	n = ftell(f);
	fseek(f, 0, SEEK_SET);
	if (n < 0 || n > (long)SPEICHER_MAX) {
		fclose(f);
		return JS_NewString(cx, "");
	}
	d = malloc((size_t)n + 1);
	if (d == NULL) {
		fclose(f);
		return JS_ThrowOutOfMemory(cx);
	}
	n = (long)fread(d, 1, (size_t)n, f);
	fclose(f);
	r = JS_NewStringLen(cx, d, (size_t)n);
	free(d);
	return r;
}

/* true geschrieben, false zu gross/Fehler, null ohne Ablage */
static JSValue g_speicher_schreiben(ARGS)
{
	jsthread *t = thread_von(cx);
	char pfad[300], neu[310];
	size_t n = 0;
	const char *s;
	FILE *f;
	bool ok;
	UNBENUTZT;
	if (!speicher_datei(t, pfad, sizeof pfad))
		return JS_NULL;
	s = JS_ToCStringLen(cx, &n, argv[0]);
	if (s == NULL)
		return JS_EXCEPTION;
	if (n > SPEICHER_MAX) {
		JS_FreeCString(cx, s);
		return JS_FALSE;
	}
	/* erst neben die alte Datei schreiben, dann tauschen (Stromausfall) */
	snprintf(neu, sizeof neu, "%s.neu", pfad);
	f = fopen(neu, "wb");
	ok = f != NULL && fwrite(s, 1, n, f) == n;
	if (f != NULL)
		ok = (fclose(f) == 0) && ok;
	JS_FreeCString(cx, s);
	if (ok) {
		remove(pfad);
		ok = rename(neu, pfad) == 0;
	}
	return JS_NewBool(cx, ok);
}

/* --- Masse aus dem Layout (B7): getBoundingClientRect, offset*, client*,
 * getComputedStyle, Rollen. Die Box eines Knotens steht in seinen
 * Nutzdaten (box_for_node); c/neu_layout.c hält sie beim Neuaufbau
 * aktuell – aber nur für Knoten im Dokument: ein entfernter Knoten
 * behielte den Zeiger in einen schon freigegebenen Baum, daher vorher
 * prüfen, dass er noch im Dokument hängt. */

#include "html/box.h"
#include "html/box_inspect.h"
#include "html/box_construct.h"
#include "netsurf/window.h"
#include "desktop/browser_private.h"
#include "desktop/textarea.h"
#include "desktop/browser_history.h"
#include "netsurf/keypress.h"
#include "html/form_internal.h"
#include "html/interaction.h"

static bool im_dokument(dom_node *n)
{
	dom_node *x = n ? dom_node_ref(n) : NULL;
	while (x != NULL) {
		dom_node_type typ;
		dom_node *eltern = NULL;
		if (dom_node_get_node_type(x, &typ) == DOM_NO_ERR && typ == DOM_DOCUMENT_NODE) {
			dom_node_unref(x);
			return true;
		}
		dom_node_get_parent_node(x, &eltern);
		dom_node_unref(x);
		x = eltern;
	}
	return false;
}

/* Box zum Knoten; während eines Neu-Layouts (c/neu_layout.c) keine –
 * box_for_node zeigte dann in den halb gebauten neuen Baum */
static struct box *box_von(jsthread *t, dom_node *n)
{
	if (n == NULL || t->htmlc == NULL || t->htmlc->layout == NULL || !im_dokument(n) ||
	    ns_neu_layout_laeuft(t->htmlc))
		return NULL;
	return box_for_node(n);
}

/* Fenster des Browsers (bei Rahmen das oberste) */
static struct browser_window *oberstes(jsthread *t)
{
	struct browser_window *bw = t->bw;
	while (bw != NULL && bw->window == NULL && bw->parent != NULL)
		bw = bw->parent;
	return (bw != NULL && bw->window != NULL) ? bw : NULL;
}

/* N.masse(knoten) → [x, y, breite, höhe, client-breite, client-höhe,
 * inhalt-breite, inhalt-höhe] (Rahmenbox in Dokumentkoordinaten; client =
 * Polsterbox) oder null */
static JSValue g_masse(ARGS)
{
	jsthread *t = thread_von(cx);
	struct box *b;
	int x = 0, y = 0, cw, ch;
	JSValue a;
	UNBENUTZT;
	b = box_von(t, knoten(cx, argv[0]));
	if (b == NULL)
		return JS_NULL;
	box_coords(b, &x, &y);
	cw = b->padding[LEFT] + b->width + b->padding[RIGHT];
	ch = b->padding[TOP] + b->height + b->padding[BOTTOM];
	a = JS_NewArray(cx);
	JS_SetPropertyUint32(cx, a, 0, JS_NewInt32(cx, x - b->border[LEFT].width));
	JS_SetPropertyUint32(cx, a, 1, JS_NewInt32(cx, y - b->border[TOP].width));
	JS_SetPropertyUint32(cx, a, 2, JS_NewInt32(cx, cw + b->border[LEFT].width + b->border[RIGHT].width));
	JS_SetPropertyUint32(cx, a, 3, JS_NewInt32(cx, ch + b->border[TOP].width + b->border[BOTTOM].width));
	JS_SetPropertyUint32(cx, a, 4, JS_NewInt32(cx, cw));
	JS_SetPropertyUint32(cx, a, 5, JS_NewInt32(cx, ch));
	JS_SetPropertyUint32(cx, a, 6, JS_NewInt32(cx, b->width));
	JS_SetPropertyUint32(cx, a, 7, JS_NewInt32(cx, b->height));
	return a;
}

static const char *farbe_text(css_color c, char *buf, size_t n)
{
	unsigned a = (c >> 24) & 0xff;
	if (a == 0)
		return "rgba(0, 0, 0, 0)";
	if (a == 0xff)
		snprintf(buf, n, "rgb(%u, %u, %u)", (unsigned)((c >> 16) & 0xff), (unsigned)((c >> 8) & 0xff), (unsigned)(c & 0xff));
	else
		snprintf(buf, n, "rgba(%u, %u, %u, %.2f)", (unsigned)((c >> 16) & 0xff), (unsigned)((c >> 8) & 0xff), (unsigned)(c & 0xff), a / 255.0);
	return buf;
}

/* N.stil(knoten) → {display, visibility, position, color, backgroundColor,
 * fontSize} aus dem berechneten Stil der Box oder null */
static JSValue g_stil(ARGS)
{
	jsthread *t = thread_von(cx);
	struct box *b;
	const css_computed_style *st;
	const char *anz = "", *sicht = "", *lage = "";
	char f[48], hg[48], gr[24];
	css_color farbe = 0;
	css_fixed laenge = 0;
	css_unit einheit = CSS_UNIT_PX;
	JSValue o;
	UNBENUTZT;
	b = box_von(t, knoten(cx, argv[0]));
	if (b == NULL || b->style == NULL)
		return JS_NULL;
	st = b->style;
	switch (css_computed_display(st, b->parent == NULL)) {
	case CSS_DISPLAY_INLINE: anz = "inline"; break;
	case CSS_DISPLAY_BLOCK: anz = "block"; break;
	case CSS_DISPLAY_LIST_ITEM: anz = "list-item"; break;
	case CSS_DISPLAY_RUN_IN: anz = "run-in"; break;
	case CSS_DISPLAY_INLINE_BLOCK: anz = "inline-block"; break;
	case CSS_DISPLAY_TABLE: anz = "table"; break;
	case CSS_DISPLAY_INLINE_TABLE: anz = "inline-table"; break;
	case CSS_DISPLAY_TABLE_ROW_GROUP: anz = "table-row-group"; break;
	case CSS_DISPLAY_TABLE_HEADER_GROUP: anz = "table-header-group"; break;
	case CSS_DISPLAY_TABLE_FOOTER_GROUP: anz = "table-footer-group"; break;
	case CSS_DISPLAY_TABLE_ROW: anz = "table-row"; break;
	case CSS_DISPLAY_TABLE_COLUMN_GROUP: anz = "table-column-group"; break;
	case CSS_DISPLAY_TABLE_COLUMN: anz = "table-column"; break;
	case CSS_DISPLAY_TABLE_CELL: anz = "table-cell"; break;
	case CSS_DISPLAY_TABLE_CAPTION: anz = "table-caption"; break;
	case CSS_DISPLAY_NONE: anz = "none"; break;
	case CSS_DISPLAY_FLEX: anz = "flex"; break;
	case CSS_DISPLAY_INLINE_FLEX: anz = "inline-flex"; break;
	case CSS_DISPLAY_GRID: anz = "grid"; break;
	case CSS_DISPLAY_INLINE_GRID: anz = "inline-grid"; break;
	default: break;
	}
	switch (css_computed_visibility(st)) {
	case CSS_VISIBILITY_VISIBLE: sicht = "visible"; break;
	case CSS_VISIBILITY_HIDDEN: sicht = "hidden"; break;
	case CSS_VISIBILITY_COLLAPSE: sicht = "collapse"; break;
	default: break;
	}
	switch (css_computed_position(st)) {
	case CSS_POSITION_STATIC: lage = "static"; break;
	case CSS_POSITION_RELATIVE: lage = "relative"; break;
	case CSS_POSITION_ABSOLUTE: lage = "absolute"; break;
	case CSS_POSITION_FIXED: lage = "fixed"; break;
	case CSS_POSITION_STICKY: lage = "sticky"; break;
	default: break;
	}
	o = JS_NewObject(cx);
	JS_SetPropertyStr(cx, o, "display", JS_NewString(cx, anz));
	JS_SetPropertyStr(cx, o, "visibility", JS_NewString(cx, sicht));
	JS_SetPropertyStr(cx, o, "position", JS_NewString(cx, lage));
	css_computed_color(st, &farbe);
	JS_SetPropertyStr(cx, o, "color", JS_NewString(cx, farbe_text(farbe, f, sizeof f)));
	farbe = 0;
	css_computed_background_color(st, &farbe);
	JS_SetPropertyStr(cx, o, "backgroundColor", JS_NewString(cx, farbe_text(farbe, hg, sizeof hg)));
	if (css_computed_font_size(st, &laenge, &einheit) == CSS_FONT_SIZE_DIMENSION && einheit == CSS_UNIT_PX) {
		snprintf(gr, sizeof gr, "%gpx", FIXTOFLT(laenge));
		JS_SetPropertyStr(cx, o, "fontSize", JS_NewString(cx, gr));
	}
	return o;
}

/* N.ansicht() → [breite, höhe, rollen-x, rollen-y] des Fensters */
static JSValue g_ansicht(ARGS)
{
	jsthread *t = thread_von(cx);
	struct browser_window *bw = oberstes(t);
	int w = 0, h = 0, sx = 0, sy = 0;
	JSValue a;
	(void)this_val; (void)argc; (void)argv;
	if (bw != NULL) {
		/* Fenster rechnet mit Zoom, Skripte in CSS-Pixeln (B8) */
		float z = browser_window_get_scale(bw);
		browser_window_get_dimensions(bw, &w, &h);
		guit->window->get_scroll(bw->window, &sx, &sy);
		if (z > 0.0f) {
			w = (int)(w / z);
			h = (int)(h / z);
			sx = (int)(sx / z);
			sy = (int)(sy / z);
		}
	}
	a = JS_NewArray(cx);
	JS_SetPropertyUint32(cx, a, 0, JS_NewInt32(cx, w));
	JS_SetPropertyUint32(cx, a, 1, JS_NewInt32(cx, h));
	JS_SetPropertyUint32(cx, a, 2, JS_NewInt32(cx, sx));
	JS_SetPropertyUint32(cx, a, 3, JS_NewInt32(cx, sy));
	return a;
}

/* N.rollenNach(x, y): Fenster dorthin rollen (scrollTo, scrollIntoView) */
static JSValue g_rollen_nach(ARGS)
{
	jsthread *t = thread_von(cx);
	struct browser_window *bw = oberstes(t);
	int32_t x = 0, y = 0;
	struct rect r;
	UNBENUTZT;
	JS_ToInt32(cx, &x, argv[0]);
	JS_ToInt32(cx, &y, argv[1]);
	if (bw == NULL)
		return JS_UNDEFINED;
	{
		float z = browser_window_get_scale(bw);
		if (z > 0.0f) {
			x = (int32_t)(x * z);
			y = (int32_t)(y * z);
		}
	}
	r.x0 = x < 0 ? 0 : x;
	r.y0 = y < 0 ? 0 : y;
	r.x1 = r.x0;
	r.y1 = r.y0;
	guit->window->set_scroll(bw->window, &r);
	return JS_UNDEFINED;
}

/* ------------------------------------------------ Verlauf (B7) */

struct verlauf_suche {
	int rest;
	int anzahl;
	const struct history_entry *treffer;
};

static bool verlauf_zaehlen(const struct browser_window *bw, int x0, int y0, int x1, int y1,
		const struct history_entry *e, void *pw)
{
	struct verlauf_suche *s = pw;
	(void)bw; (void)x0; (void)y0; (void)x1; (void)y1;
	s->anzahl++;
	if (s->treffer == NULL && --s->rest == 0)
		s->treffer = e;
	return true;
}

/* N.verlauf(n): history.go(n) – 0 lädt neu, sonst n Schritte zurück bzw.
 * vor über NetSurfs Verlauf; ausserhalb des Verlaufs nichts (Standard) */
static JSValue g_verlauf(ARGS)
{
	jsthread *t = thread_von(cx);
	struct browser_window *bw = oberstes(t);
	struct verlauf_suche s = { 0, 0, NULL };
	int32_t n = 0;
	UNBENUTZT;
	JS_ToInt32(cx, &n, argv[0]);
	if (bw == NULL)
		return JS_UNDEFINED;
	if (n == 0) {
		browser_window_reload(bw, false);
		return JS_UNDEFINED;
	}
	s.rest = n < 0 ? -n : n;
	if (n < 0)
		browser_window_history_enumerate_back(bw, verlauf_zaehlen, &s);
	else
		browser_window_history_enumerate_forward(bw, verlauf_zaehlen, &s);
	if (s.treffer != NULL)
		browser_window_history_go(bw, (struct history_entry *)s.treffer, false);
	return JS_UNDEFINED;
}

/* N.verlaufLaenge() → Einträge zurück + aktueller + vor (history.length) */
static JSValue g_verlauf_laenge(ARGS)
{
	jsthread *t = thread_von(cx);
	struct browser_window *bw = oberstes(t);
	struct verlauf_suche s = { 0, 0, NULL };
	(void)this_val; (void)argc; (void)argv;
	if (bw == NULL)
		return JS_NewInt32(cx, 1);
	browser_window_history_enumerate_back(bw, verlauf_zaehlen, &s);
	browser_window_history_enumerate_forward(bw, verlauf_zaehlen, &s);
	return JS_NewInt32(cx, s.anzahl + 1);
}

/* ------------------------------------------------ Formularfelder (B7) */

void ns_feld_vorgabe_sichern(dom_node *n);

static dom_html_element_type elementart(dom_node *n)
{
	dom_node_type typ;
	dom_html_element_type art = DOM_HTML_ELEMENT_TYPE__UNKNOWN;
	if (n != NULL && dom_node_get_node_type(n, &typ) == DOM_NO_ERR && typ == DOM_ELEMENT_NODE)
		dom_html_element_get_tag_type((dom_html_element *)n, &art);
	return art;
}

/* Bedienelement NetSurfs zum Knoten (nur mit Box im Layout) oder NULL */
static struct form_control *bedienelement(jsthread *t, dom_node *n)
{
	struct box *b = box_von(t, n);
	return b != NULL ? b->gadget : NULL;
}

static bool ist_textfeld(struct form_control *g)
{
	return g != NULL && g->box != NULL && g->data.text.ta != NULL &&
		(g->type == GADGET_TEXTBOX || g->type == GADGET_PASSWORD || g->type == GADGET_TEXTAREA);
}

static JSValue text_oder_leer(JSContext *cx, dom_string *s)
{
	return s != NULL ? als_text(cx, s) : JS_NewString(cx, "");
}

/* N.textareaWert(el) → aktueller Text eines textarea (mit Getipptem) */
static JSValue g_textarea_wert(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s = NULL;
	UNBENUTZT;
	if (elementart(n) != DOM_HTML_ELEMENT_TYPE_TEXTAREA ||
	    dom_html_text_area_element_get_value((dom_html_text_area_element *)n, &s) != DOM_NO_ERR)
		s = NULL;
	return text_oder_leer(cx, s);
}

/* N.feldSetzen(el, text): Wert von input/textarea setzen und NetSurfs
 * Textfeld nachziehen (Vorgabewert bleibt) */
static JSValue g_feld_setzen(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_node *n = knoten(cx, argv[0]);
	dom_html_element_type art = elementart(n);
	struct form_control *g;
	dom_string *s;
	UNBENUTZT;
	if (art != DOM_HTML_ELEMENT_TYPE_INPUT && art != DOM_HTML_ELEMENT_TYPE_TEXTAREA)
		return JS_UNDEFINED;
	s = zu_dom(cx, argv[1]);
	if (s == NULL)
		return JS_EXCEPTION;
	if (art == DOM_HTML_ELEMENT_TYPE_TEXTAREA) {
		dom_html_text_area_element_set_value((dom_html_text_area_element *)n, s);
	} else {
		ns_feld_vorgabe_sichern(n);
		dom_html_input_element_set_value((dom_html_input_element *)n, s);
	}
	dom_string_unref(s);
	g = bedienelement(t, n);
	if (ist_textfeld(g)) {
		form_gadget_sync_with_dom(g);
		html__redraw_a_box(t->htmlc, g->box);
	}
	return JS_UNDEFINED;
}

/* N.vorgabe(el) → [Vorgabewert, vorgabe angehakt] eines input
 * (defaultValue, defaultChecked, reset(); bei textarea ist die Vorgabe der
 * Inhalt, das regelt vorspann.js) */
static JSValue g_vorgabe(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_html_element_type art = elementart(n);
	dom_string *s = NULL;
	bool an = false;
	JSValue a;
	UNBENUTZT;
	if (art == DOM_HTML_ELEMENT_TYPE_INPUT) {
		if (dom_html_input_element_get_default_value((dom_html_input_element *)n, &s) != DOM_NO_ERR)
			s = NULL;
		if (dom_html_input_element_get_default_checked((dom_html_input_element *)n, &an) != DOM_NO_ERR)
			an = false;
	}
	a = JS_NewArray(cx);
	JS_SetPropertyUint32(cx, a, 0, text_oder_leer(cx, s));
	JS_SetPropertyUint32(cx, a, 1, JS_NewBool(cx, an));
	return a;
}

/* N.vorgabeSetzen(input, text): defaultValue setzen */
static JSValue g_vorgabe_setzen(ARGS)
{
	dom_node *n = knoten(cx, argv[0]);
	dom_string *s;
	UNBENUTZT;
	if (elementart(n) != DOM_HTML_ELEMENT_TYPE_INPUT)
		return JS_UNDEFINED;
	s = zu_dom(cx, argv[1]);
	if (s == NULL)
		return JS_EXCEPTION;
	dom_html_input_element_set_default_value((dom_html_input_element *)n, s);
	dom_string_unref(s);
	return JS_UNDEFINED;
}

/* N.anhaken(el, an): Kästchen/Optionsfeld setzen und NetSurfs Anzeige
 * nachziehen (die Gruppe eines Optionsfelds regelt vorspann.js) */
static JSValue g_anhaken(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_node *n = knoten(cx, argv[0]);
	bool an = JS_ToBool(cx, argv[1]);
	struct form_control *g;
	UNBENUTZT;
	if (elementart(n) != DOM_HTML_ELEMENT_TYPE_INPUT)
		return JS_UNDEFINED;
	dom_html_input_element_set_checked((dom_html_input_element *)n, an);
	g = bedienelement(t, n);
	if (g != NULL && (g->type == GADGET_CHECKBOX || g->type == GADGET_RADIO)) {
		g->selected = an;
		html__redraw_a_box(t->htmlc, g->box);
	}
	return JS_UNDEFINED;
}

/* N.auswahlNachziehen(select, zuruecksetzen): NetSurfs Auswahlliste nach
 * dem DOM richten (selected der option-Elemente, von Skripten gesetzt);
 * mit zuruecksetzen vorher jede Option auf ihre Vorgabe (reset()) */
static JSValue g_auswahl_nachziehen(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_node *n = knoten(cx, argv[0]);
	bool zurueck = JS_ToBool(cx, argv[1]);
	struct form_control *g;
	struct form_option *o;
	int i, erste = -1;
	UNBENUTZT;
	if (elementart(n) != DOM_HTML_ELEMENT_TYPE_SELECT)
		return JS_UNDEFINED;
	g = bedienelement(t, n);
	if (g == NULL || g->type != GADGET_SELECT || g->box == NULL)
		return JS_UNDEFINED;
	for (i = 0, o = g->data.select.items; o != NULL; i++, o = o->next) {
		bool im_dom = false;
		if (zurueck)
			dom_html_option_element_set_selected(o->node, o->initial_selected);
		if (dom_html_option_element_get_selected(o->node, &im_dom) != DOM_NO_ERR)
			im_dom = false;
		if (g->data.select.multiple) {
			/* schaltet um, wo NetSurf und DOM verschieden sind */
			if (im_dom != o->selected)
				form_select_process_selection(g, i);
		} else if (im_dom) {
			erste = i;
		}
	}
	/* eine Auswahl: die letzte gewählte (wie NetSurfs form_add_option),
	 * sonst die erste Option (Standard) */
	if (!g->data.select.multiple && g->data.select.items != NULL)
		form_select_process_selection(g, erste < 0 ? 0 : erste);
	return JS_UNDEFINED;
}

/* N.feldFokus(el, an) → true bei einem Textfeld: Schreibmarke ans Ende
 * setzen (öffnet am Board die Bildschirmtastatur) bzw. wegnehmen */
static JSValue g_feld_fokus(ARGS)
{
	jsthread *t = thread_von(cx);
	struct form_control *g = bedienelement(t, knoten(cx, argv[0]));
	UNBENUTZT;
	if (!ist_textfeld(g) || t->htmlc->bw == NULL)
		return JS_FALSE;
	if (JS_ToBool(cx, argv[1])) {
		textarea_set_caret(g->data.text.ta, INT_MAX);
	} else if (t->htmlc->focus_type == HTML_FOCUS_TEXTAREA && t->htmlc->focus_owner.textarea == g->box) {
		union html_focus_owner selbst;
		selbst.self = true;
		textarea_set_caret(g->data.text.ta, -1);
		html_set_focus(t->htmlc, HTML_FOCUS_SELF, selbst, true, 0, 0, 0, NULL);
	}
	return JS_TRUE;
}

/* N.feldAuswahl(el) → true bei einem Textfeld: Fokus und ganzen Text
 * auswählen (select()) */
static JSValue g_feld_auswahl(ARGS)
{
	jsthread *t = thread_von(cx);
	struct form_control *g = bedienelement(t, knoten(cx, argv[0]));
	UNBENUTZT;
	if (!ist_textfeld(g))
		return JS_FALSE;
	if (t->htmlc->bw != NULL)
		textarea_set_caret(g->data.text.ta, INT_MAX);
	textarea_keypress(g->data.text.ta, NS_KEY_SELECT_ALL);
	return JS_TRUE;
}

/* N.aktivesFeld() → Textfeld mit der Schreibmarke oder null */
static JSValue g_aktives_feld(ARGS)
{
	jsthread *t = thread_von(cx);
	struct box *b;
	(void)this_val; (void)argc; (void)argv;
	if (t->htmlc == NULL || t->htmlc->focus_type != HTML_FOCUS_TEXTAREA)
		return JS_NULL;
	b = t->htmlc->focus_owner.textarea;
	if (b == NULL || b->gadget == NULL || b->gadget->node == NULL)
		return JS_NULL;
	return huelle(t, b->gadget->node);
}

/* N.absenden(form) → true, wenn NetSurf das Formular abschickt
 * (form.submit(): ohne „submit“-Ereignis, wie im Standard) */
static JSValue g_absenden(ARGS)
{
	jsthread *t = thread_von(cx);
	dom_node *n = knoten(cx, argv[0]);
	struct form *f;
	UNBENUTZT;
	if (n == NULL || t->htmlc == NULL || t->htmlc->bw == NULL)
		return JS_FALSE;
	for (f = t->htmlc->forms; f != NULL; f = f->prev)
		if (f->node == n)
			break;
	if (f == NULL)
		return JS_FALSE;
	return JS_NewBool(cx, form_submit(content_get_url(&t->htmlc->base), t->htmlc->bw, f, NULL) == NSERROR_OK);
}

/* --- Sonstiges */

static JSValue g_protokoll(ARGS)
{
	const char *art = JS_ToCString(cx, argv[0]);
	const char *text = JS_ToCString(cx, argv[1]);
	UNBENUTZT;
	protokoll(art ? art : "log", text ? text : "");
	if (art)
		JS_FreeCString(cx, art);
	if (text)
		JS_FreeCString(cx, text);
	return JS_UNDEFINED;
}

static JSValue g_adresse(ARGS)
{
	jsthread *t = thread_von(cx);
	nsurl *u;
	(void)this_val; (void)argc; (void)argv;
	if (t->htmlc == NULL)
		return JS_NewString(cx, "about:blank");
	u = content_get_url(&t->htmlc->base);
	return u ? JS_NewString(cx, nsurl_access(u)) : JS_NewString(cx, "about:blank");
}

static JSValue g_gehe_zu(ARGS)
{
	jsthread *t = thread_von(cx);
	const char *s = JS_ToCString(cx, argv[0]);
	nsurl *basis, *ziel = NULL;
	UNBENUTZT;
	if (s == NULL)
		return JS_EXCEPTION;
	if (t->bw != NULL && t->htmlc != NULL) {
		basis = content_get_url(&t->htmlc->base);
		if (nsurl_join(basis, s, &ziel) == NSERROR_OK) {
			browser_window_navigate(t->bw, ziel, basis, BW_NAVIGATE_HISTORY, NULL, NULL, NULL);
			nsurl_unref(ziel);
		}
	}
	JS_FreeCString(cx, s);
	return JS_UNDEFINED;
}

static JSValue g_aenderungen(ARGS)
{
	(void)this_val; (void)argc; (void)argv;
	return JS_NewInt32(cx, (int32_t)thread_von(cx)->aenderungen);
}

static const JSCFunctionListEntry grundfunktionen[] = {
	JS_CFUNC_DEF("typ", 1, g_typ),
	JS_CFUNC_DEF("name", 1, g_name),
	JS_CFUNC_DEF("wert", 1, g_wert),
	JS_CFUNC_DEF("wertSetzen", 2, g_wert_setzen),
	JS_CFUNC_DEF("text", 1, g_text),
	JS_CFUNC_DEF("textSetzen", 2, g_text_setzen),
	JS_CFUNC_DEF("nachbar", 2, g_nachbar),
	JS_CFUNC_DEF("baum", 4, g_baum),
	JS_CFUNC_DEF("klonen", 2, g_klonen),
	JS_CFUNC_DEF("attr", 2, g_attr),
	JS_CFUNC_DEF("attrSetzen", 3, g_attr_setzen),
	JS_CFUNC_DEF("attrs", 1, g_attrs),
	JS_CFUNC_DEF("dokument", 0, g_dokument),
	JS_CFUNC_DEF("erzeugen", 2, g_erzeugen),
	JS_CFUNC_DEF("nachId", 1, g_nach_id),
	JS_CFUNC_DEF("fragment", 2, g_fragment),
	JS_CFUNC_DEF("anmelden", 3, g_anmelden),
	JS_CFUNC_DEF("ausloesen", 5, g_ausloesen),
	JS_CFUNC_DEF("ereignis", 2, g_ereignis),
	JS_CFUNC_DEF("zeitgeber", 2, g_zeitgeber),
	JS_CFUNC_DEF("zeitgeberWeg", 1, g_zeitgeber_weg),
	JS_CFUNC_DEF("protokoll", 2, g_protokoll),
	JS_CFUNC_DEF("adresse", 0, g_adresse),
	JS_CFUNC_DEF("geheZu", 1, g_gehe_zu),
	JS_CFUNC_DEF("aenderungen", 0, g_aenderungen),
	JS_CFUNC_DEF("abruf", 4, g_abruf),
	JS_CFUNC_DEF("cookie", 0, g_cookie),
	JS_CFUNC_DEF("cookieSetzen", 1, g_cookie_setzen),
	JS_CFUNC_DEF("speicherLesen", 0, g_speicher_lesen),
	JS_CFUNC_DEF("speicherSchreiben", 1, g_speicher_schreiben),
	JS_CFUNC_DEF("masse", 1, g_masse),
	JS_CFUNC_DEF("stil", 1, g_stil),
	JS_CFUNC_DEF("ansicht", 0, g_ansicht),
	JS_CFUNC_DEF("rollenNach", 2, g_rollen_nach),
	JS_CFUNC_DEF("textareaWert", 1, g_textarea_wert),
	JS_CFUNC_DEF("feldSetzen", 2, g_feld_setzen),
	JS_CFUNC_DEF("vorgabe", 1, g_vorgabe),
	JS_CFUNC_DEF("anhaken", 2, g_anhaken),
	JS_CFUNC_DEF("feldFokus", 2, g_feld_fokus),
	JS_CFUNC_DEF("feldAuswahl", 1, g_feld_auswahl),
	JS_CFUNC_DEF("aktivesFeld", 0, g_aktives_feld),
	JS_CFUNC_DEF("absenden", 1, g_absenden),
	JS_CFUNC_DEF("abrufStoppen", 1, g_abruf_stoppen),
	JS_CFUNC_DEF("vorgabeSetzen", 2, g_vorgabe_setzen),
	JS_CFUNC_DEF("auswahlNachziehen", 2, g_auswahl_nachziehen),
	JS_CFUNC_DEF("verlauf", 1, g_verlauf),
	JS_CFUNC_DEF("verlaufLaenge", 0, g_verlauf_laenge),
};

/* ------------------------------------------------------------ js.h */

void js_initialise(void)
{
	/* Laufzeiten entstehen je Browserfenster; hier nur der Inhaltstyp
	 * für externe Skripte (application/javascript …) */
	javascript_init();
	NSLOG(netsurf, INFO, "JavaScript: QuickJS-ng %s", JS_GetVersion());
}

void js_finalise(void)
{
	NSLOG(netsurf, INFO, "JavaScript beendet");
}

/* Speicher der Laufzeiten: über malloc (im Programm-Modus der Heap des
 * Programms, compat/gemeinsam/ns_programm.h – QuickJS selbst wird ohne diese Makros
 * übersetzt). QuickJS braucht für die Speichergrenze die Grösse jedes
 * Blocks: sie steht in einem Kopf vor dem Block. */
#define KOPF (sizeof(size_t) > 4 ? 16u : 8u)

/* Q3: Reserve im Heap des Programms, die JavaScript nicht verbrauchen darf
 * (Seite, Layout, Bilder und das Neuladen ohne JS nach einem Abbruch).
 * Bis NS_JS_GRUND je Laufzeit (eine je Dokument, auch iframes) gilt sie
 * nicht – so viel wie die frühere feste Grenze je Laufzeit: keine Seite
 * bekommt weniger als vorher, auch das Anlegen einer Laufzeit scheitert
 * nicht an ihr; darüber darf JS so viel nehmen, wie die Seite übrig lässt –
 * bis NS_JS_SPEICHER. 0 = keine Reserve. Gezählt wird je Laufzeit über ihr
 * Opaque (Zeiger auf `belegt` im jsthread bzw. jsheap). */
#ifndef NS_JS_RESERVE
#define NS_JS_RESERVE 0u
#endif
#ifndef NS_JS_GRUND
#define NS_JS_GRUND (2u * 1024u * 1024u)
#endif
size_t ns_programm_frei(void);
static size_t js_reserve = NS_JS_RESERVE;

/* Tests (32 Bit, Programm im Kern): Reserve setzen */
void ns_js_reserve_setzen(size_t r)
{
	js_reserve = r;
}

/* Darf die Laufzeit mit dem Zähler `z` `n` weitere Bytes belegen? Bis
 * NS_JS_GRUND immer, darüber nur, wenn danach die Reserve frei bleibt. */
static bool js_platz(const size_t *z, size_t n)
{
	size_t frei;
	if (js_reserve == 0)
		return true;
	if (z != NULL && *z <= NS_JS_GRUND && n <= NS_JS_GRUND - *z)
		return true;
	frei = ns_programm_frei();
	return frei >= js_reserve && frei - js_reserve >= n + KOPF;
}

static void *js_m(void *o, size_t n)
{
	size_t *p;
	size_t *z = o;
	if (!js_platz(z, n))
		return NULL;
	p = malloc(n + KOPF);
	if (p == NULL)
		return NULL;
	p[0] = n;
	if (z != NULL)
		*z += n;
	return (char *)p + KOPF;
}

static void *js_c(void *o, size_t anzahl, size_t g)
{
	size_t n;
	void *p;
	if (g != 0 && anzahl > (size_t)-1 / g)
		return NULL;
	n = anzahl * g;
	p = js_m(o, n);
	if (p != NULL)
		memset(p, 0, n);
	return p;
}

static void js_f(void *o, void *p)
{
	size_t *z = o;
	if (p == NULL)
		return;
	if (z != NULL)
		*z -= *(const size_t *)((const char *)p - KOPF);
	free((char *)p - KOPF);
}

static void *js_r(void *o, void *p, size_t n)
{
	size_t *q, alt;
	if (p == NULL)
		return js_m(o, n);
	if (n == 0) {
		js_f(o, p);
		return NULL;
	}
	alt = *(const size_t *)((const char *)p - KOPF);
	if (n > alt && !js_platz(o, n - alt))
		return NULL;
	q = realloc((char *)p - KOPF, n + KOPF);
	if (q == NULL)
		return NULL;
	q[0] = n;
	if (o != NULL)
		*(size_t *)o = *(size_t *)o - alt + n;
	return (char *)q + KOPF;
}

static size_t js_g(const void *p)
{
	return p != NULL ? *(const size_t *)((const char *)p - KOPF) : 0;
}

static const JSMallocFunctions js_speicher = { js_c, js_m, js_f, js_r, js_g };

static const JSClassDef knoten_def = { .class_name = "Node", .finalizer = knoten_endet };
static const JSClassDef ereignis_def = { .class_name = "Event", .finalizer = ereignis_endet };

/* Laufzeit eines Dokuments: Grenzen und Zeitgrenze wie das Fenster */
static JSRuntime *laufzeit_neu(jsheap *h, size_t *zaehler)
{
	/* Opaque = Zähler der Laufzeit (Q3, im jsthread): die Laufzeit wird
	 * vor dem jsthread freigegeben (js_destroythread) */
	JSRuntime *rt = JS_NewRuntime2(&js_speicher, zaehler);
	if (rt == NULL)
		return NULL;
	JS_SetRuntimeOpaque(rt, h);
	JS_SetMemoryLimit(rt, NS_JS_SPEICHER);
	JS_SetMaxStackSize(rt, NS_JS_STAPEL);
	JS_SetInterruptHandler(rt, unterbrechen, h);
	if (JS_NewClass(rt, h->knoten_klasse, &knoten_def) < 0 ||
	    JS_NewClass(rt, h->ereignis_klasse, &ereignis_def) < 0) {
		JS_FreeRuntime(rt);
		return NULL;
	}
	return rt;
}

nserror js_newheap(int timeout, jsheap **heap)
{
	jsheap *h = calloc(1, sizeof *h);
	if (h == NULL)
		return NSERROR_NOMEM;
	h->rt = JS_NewRuntime2(&js_speicher, &h->belegt);
	if (h->rt == NULL) {
		free(h);
		return NSERROR_NOMEM;
	}
	JS_SetRuntimeOpaque(h->rt, h);
	JS_SetMemoryLimit(h->rt, NS_JS_SPEICHER);
	JS_SetMaxStackSize(h->rt, NS_JS_STAPEL);
	h->timeout_ms = (timeout > 0 ? timeout : 10) * 1000;
	JS_SetInterruptHandler(h->rt, unterbrechen, h);
	/* Klassennummern einmal je Fenster; angemeldet werden die Klassen in
	 * der Laufzeit jedes Dokuments (laufzeit_neu) */
	JS_NewClassID(h->rt, &h->knoten_klasse);
	JS_NewClassID(h->rt, &h->ereignis_klasse);
	*heap = h;
	return NSERROR_OK;
}

static void heap_freigeben(jsheap *h)
{
	JS_FreeRuntime(h->rt);
	free(h);
}

void js_destroyheap(jsheap *heap)
{
	if (heap == NULL)
		return;
	/* Dokumente können ihren Thread noch halten (werden später zerstört) */
	if (heap->threads == 0)
		heap_freigeben(heap);
	else
		heap->weg = true;
}

nserror js_newthread(jsheap *heap, void *win_priv, void *doc_priv, jsthread **thread)
{
	jsthread *t;
	JSValue g, ns, r;
	if (heap == NULL)
		return NSERROR_BAD_PARAMETER;
	t = calloc(1, sizeof *t);
	if (t == NULL)
		return NSERROR_NOMEM;
	t->heap = heap;
	t->bw = win_priv;
	t->htmlc = doc_priv;
	t->doc = t->htmlc ? t->htmlc->document : NULL;
	t->rt = laufzeit_neu(heap, &t->belegt);
	t->cx = t->rt != NULL ? JS_NewContext(t->rt) : NULL;
	if (t->cx == NULL || dom_event_listener_create(hoerer_ruft, t, &t->hoerer) != DOM_NO_ERR) {
		if (t->cx)
			JS_FreeContext(t->cx);
		if (t->rt)
			JS_FreeRuntime(t->rt);
		free(t);
		return NSERROR_NOMEM;
	}
	heap->threads++;
	JS_SetContextOpaque(t->cx, t);
	g = JS_GetGlobalObject(t->cx);
	ns = JS_NewObject(t->cx);
	JS_SetPropertyFunctionList(t->cx, ns, grundfunktionen, (int)(sizeof grundfunktionen / sizeof grundfunktionen[0]));
	JS_SetPropertyStr(t->cx, g, "__ns", ns);
	JS_FreeValue(t->cx, g);
	eintreten(t);
	r = JS_Eval(t->cx, ns_vorspann, sizeof(ns_vorspann) - 1, "vorspann.js", JS_EVAL_TYPE_GLOBAL);
	if (JS_IsException(r))
		ausnahme_melden(t->cx);
	JS_FreeValue(t->cx, r);
	austreten(t);
	*thread = t;
	return NSERROR_OK;
}

static void zeitgeber_stoppen(jsthread *t)
{
	while (t->zeitgeber != NULL) {
		struct zeitgeber *z = t->zeitgeber;
		t->zeitgeber = z->next;
		guit->misc->schedule(-1, zeitgeber_ruft, z);
		free(z);
	}
}

nserror js_closethread(jsthread *thread)
{
	if (thread == NULL)
		return NSERROR_OK;
	/* localStorage: noch nicht geschriebene Änderungen sichern (B7) */
	if (!thread->geschlossen && thread->cx != NULL) {
		eintreten(thread);
		JS_FreeValue(thread->cx, rufen(thread, "__ns_speicher_sichern", 0, NULL));
		austreten(thread);
	}
	thread->geschlossen = true;
	zeitgeber_stoppen(thread);
	layout_stoppen(thread);
	abrufe_stoppen(thread);
	return NSERROR_OK;
}

void js_destroythread(jsthread *thread)
{
	jsheap *h;
	if (thread == NULL)
		return;
	h = thread->heap;
	thread->geschlossen = true;
	zeitgeber_stoppen(thread);
	layout_stoppen(thread);
	abrufe_stoppen(thread);
	tabelle_freigeben(thread->cx, &thread->ereignisse);
	tabelle_freigeben(thread->cx, &thread->knoten);
	JS_FreeContext(thread->cx);
	thread->cx = NULL;
	JS_FreeRuntime(thread->rt);
	thread->rt = NULL;
	dom_event_listener_unref(thread->hoerer);
	free(thread);
	h->threads--;
	if (h->weg && h->threads == 0)
		heap_freigeben(h);
}

bool js_exec(jsthread *thread, const uint8_t *txt, size_t txtlen, const char *name)
{
	char *quelle;
	JSValue r;
	if (thread == NULL || thread->geschlossen)
		return false;
	/* QuickJS verlangt ein Nullbyte hinter dem Quelltext */
	quelle = malloc(txtlen + 1);
	if (quelle == NULL)
		return false;
	memcpy(quelle, txt, txtlen);
	quelle[txtlen] = '\0';
	eintreten(thread);
#ifdef ESP_PLATFORM
	int64_t beginn = esp_timer_get_time();
#endif
	{
		/* Q3: Adresse des laufenden Skripts für document.currentScript */
		JSValue g = JS_GetGlobalObject(thread->cx);
		JS_SetPropertyStr(thread->cx, g, "__ns_skript", name ? JS_NewString(thread->cx, name) : JS_UNDEFINED);
		r = JS_Eval(thread->cx, quelle, txtlen, name ? name : "<skript>", JS_EVAL_TYPE_GLOBAL);
		JS_SetPropertyStr(thread->cx, g, "__ns_skript", JS_UNDEFINED);
		JS_FreeValue(thread->cx, g);
	}
#ifdef ESP_PLATFORM
	ESP_LOGI("ns_js", "Skript %u Byte, %lld ms: %.80s", (unsigned)txtlen, (long long)((esp_timer_get_time() - beginn) / 1000),
		 name ? name : "<skript>");
#endif
	free(quelle);
	if (JS_IsException(r))
		ausnahme_melden(thread->cx);
	JS_FreeValue(thread->cx, r);
	austreten(thread);
	return true;
}

bool js_fire_event(jsthread *thread, const char *type, struct dom_document *doc, struct dom_node *target)
{
	JSValue a[2];
	(void)doc;
	if (thread == NULL || thread->geschlossen)
		return true;
	eintreten(thread);
	a[0] = JS_NewString(thread->cx, type);
	a[1] = huelle(thread, target);
	JS_FreeValue(thread->cx, rufen(thread, "__ns_fenster_ereignis", 2, a));
	JS_FreeValue(thread->cx, a[0]);
	JS_FreeValue(thread->cx, a[1]);
	if (strcmp(type, "load") == 0 && thread->htmlc != NULL) {
		/* NetSurf baut direkt danach den Box-Baum aus dem DOM (html.c,
		 * html_begin_conversion): bis hier ist alles enthalten */
		unsigned ladend;
		thread->geladen = true;
		thread->stand = thread->aenderungen;
		ns_stylesheets_zaehlen(thread->htmlc, &thread->css_fertig, &ladend);
	}
	austreten(thread);
	return true;
}

bool js_dom_event_add_listener(jsthread *thread, struct dom_document *document, struct dom_node *node,
			       struct dom_string *event_type_dom, void *js_funcval)
{
	(void)document;
	(void)js_funcval;
	/* NetSurf ruft das nicht auf (nur die alte Duktape-Anbindung); Hörer
	 * meldet der Vorspann selbst über __ns.anmelden an */
	if (thread == NULL || node == NULL || event_type_dom == NULL)
		return false;
	return dom_event_target_add_event_listener(node, event_type_dom, thread->hoerer, false) == DOM_NO_ERR;
}

/* Hat das Element Inline-Handler (onclick="…")? Nur dann braucht es beim
 * Parsen eine Hülle – sonst kostete jede Seite eine Hülle je Element. */
static bool hat_handler(dom_node *n)
{
	dom_namednodemap *m = NULL;
	dom_ulong l = 0, i;
	bool ja = false;
	if (dom_node_get_attributes(n, &m) != DOM_NO_ERR || m == NULL)
		return false;
	dom_namednodemap_get_length(m, &l);
	for (i = 0; i < l && !ja; i++) {
		dom_attr *at = NULL;
		dom_string *k = NULL;
		if (dom_namednodemap_item(m, i, (dom_node **)&at) != DOM_NO_ERR || at == NULL)
			continue;
		if (dom_attr_get_name(at, &k) == DOM_NO_ERR && k != NULL) {
			const char *d = dom_string_data(k);
			ja = dom_string_byte_length(k) > 2 && (d[0] == 'o' || d[0] == 'O') && (d[1] == 'n' || d[1] == 'N');
			dom_string_unref(k);
		}
		dom_node_unref(at);
	}
	dom_namednodemap_unref(m);
	return ja;
}

void js_handle_new_element(jsthread *thread, struct dom_element *node)
{
	JSValue a;
	if (thread == NULL || thread->geschlossen || !hat_handler((dom_node *)node))
		return;
	eintreten(thread);
	a = huelle(thread, (dom_node *)node);
	JS_FreeValue(thread->cx, rufen(thread, "__ns_neues_element", 1, &a));
	JS_FreeValue(thread->cx, a);
	austreten(thread);
}

void js_event_cleanup(jsthread *thread, struct dom_event *evt)
{
	JSValue h;
	/* Das Ereignis endet: zwischengespeicherte Hülle freigeben */
	if (thread != NULL && thread->cx != NULL && tabelle_entfernen(&thread->ereignisse, evt, &h))
		JS_FreeValue(thread->cx, h);
}
