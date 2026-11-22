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
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
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
	JSContext *cx;
	struct browser_window *bw;
	html_content *htmlc;
	dom_document *doc;
	dom_event_listener *hoerer;
	struct tabelle knoten;
	struct tabelle ereignisse;
	struct zeitgeber *zeitgeber;
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
	if (s != NULL && strstr(s, "out of memory") != NULL) {
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
		int r = JS_ExecutePendingJob(t->heap->rt, &c2);
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

static void *js_m(void *o, size_t n)
{
	size_t *p;
	(void)o;
	p = malloc(n + KOPF);
	if (p == NULL)
		return NULL;
	p[0] = n;
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
	(void)o;
	if (p != NULL)
		free((char *)p - KOPF);
}

static void *js_r(void *o, void *p, size_t n)
{
	size_t *q;
	if (p == NULL)
		return js_m(o, n);
	if (n == 0) {
		js_f(o, p);
		return NULL;
	}
	q = realloc((char *)p - KOPF, n + KOPF);
	if (q == NULL)
		return NULL;
	q[0] = n;
	return (char *)q + KOPF;
}

static size_t js_g(const void *p)
{
	return p != NULL ? *(const size_t *)((const char *)p - KOPF) : 0;
}

static const JSMallocFunctions js_speicher = { js_c, js_m, js_f, js_r, js_g };

nserror js_newheap(int timeout, jsheap **heap)
{
	jsheap *h = calloc(1, sizeof *h);
	JSClassDef kd = { .class_name = "Node", .finalizer = knoten_endet };
	JSClassDef ed = { .class_name = "Event", .finalizer = ereignis_endet };
	if (h == NULL)
		return NSERROR_NOMEM;
	h->rt = JS_NewRuntime2(&js_speicher, NULL);
	if (h->rt == NULL) {
		free(h);
		return NSERROR_NOMEM;
	}
	JS_SetRuntimeOpaque(h->rt, h);
	JS_SetMemoryLimit(h->rt, NS_JS_SPEICHER);
	JS_SetMaxStackSize(h->rt, NS_JS_STAPEL);
	h->timeout_ms = (timeout > 0 ? timeout : 10) * 1000;
	JS_SetInterruptHandler(h->rt, unterbrechen, h);
	JS_NewClassID(h->rt, &h->knoten_klasse);
	JS_NewClass(h->rt, h->knoten_klasse, &kd);
	JS_NewClassID(h->rt, &h->ereignis_klasse);
	JS_NewClass(h->rt, h->ereignis_klasse, &ed);
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
	t->cx = JS_NewContext(heap->rt);
	if (t->cx == NULL || dom_event_listener_create(hoerer_ruft, t, &t->hoerer) != DOM_NO_ERR) {
		if (t->cx)
			JS_FreeContext(t->cx);
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
	thread->geschlossen = true;
	zeitgeber_stoppen(thread);
	layout_stoppen(thread);
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
	tabelle_freigeben(thread->cx, &thread->ereignisse);
	tabelle_freigeben(thread->cx, &thread->knoten);
	JS_FreeContext(thread->cx);
	thread->cx = NULL;
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
	r = JS_Eval(thread->cx, quelle, txtlen, name ? name : "<skript>", JS_EVAL_TYPE_GLOBAL);
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
