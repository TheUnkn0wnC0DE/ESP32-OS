/* ESP32-OS, Browser (B7, Auftrag 13.9): Neu-Layout nach DOM-Änderungen.
 *
 * NetSurf baut den Box-Baum einmal nach dem Laden. Ändern Skripte das DOM
 * danach (Timer, Klicks, nachgeladene Inhalte), baut diese Datei den
 * Box-Baum aus dem aktuellen DOM neu und layoutet die Seite neu.
 *
 * Ablauf (ns_neu_layout):
 * 1. Alten Zustand sichern: Box-Baum (talloc-Kontext), Objekte (Bilder),
 *    iframe-Liste, Formularfelder mit ihren Textfeldern (getippter Text,
 *    Schreibmarke), Fokus-/Auswahl-/Zieh-Besitzer.
 * 2. Box-Zeiger an den DOM-Knoten löschen (box_for_node), Stilauswahl neu
 *    aufsetzen (auch nachgeladene Stylesheets), dom_to_box starten. Bis der
 *    neue Baum fertig ist, zeichnet NetSurf weiter den alten.
 * 3. Fertig (umbau_fertig): Formularfelder übernehmen (alte Textfelder
 *    behalten, frei stehende Felder ohne Formular weiterverwenden),
 *    iframe-Fenster auf die neuen Boxen umhängen (bei geänderter Menge neu
 *    anlegen), Fokus/Auswahl/Ziehen umhängen, alte Objekte und den alten
 *    Baum freigeben, neu layouten (CONTENT_MSG_REFORMAT → Neuzeichnen).
 * Bei Fehler oder Abbruch (Dokument wird geschlossen) wird der alte Baum
 * wiederhergestellt.
 */
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#include <dom/dom.h>

#include "utils/errors.h"
#include "utils/log.h"
#include "utils/talloc.h"
#include "utils/corestrings.h"
#include "netsurf/content.h"
#include "netsurf/browser_window.h"
#include "content/content_protected.h"
#include "content/textsearch.h"
#include "desktop/browser_private.h"
#include "desktop/frames.h"
#include "desktop/selection.h"
#include "desktop/textarea.h"
#include "desktop/textinput.h"
#include "html/private.h"
#include "html/html.h"
#include "html/box.h"
#include "html/box_construct.h"
#include "html/css.h"
#include "html/form_internal.h"
#include "html/object.h"

/* Von js_quickjs.c: prüft nach einem Umbau, ob inzwischen mehr geändert
 * wurde */
void ns_js_neu_layout_fertig(html_content *htmlc);

struct feld {
	dom_node *knoten;
	struct box *alte_box;
	struct form_control *g;
	struct textarea *ta; /* Textfeld des alten Baums (getippter Text) */
	dom_string *initial;
	bool uebernommen;
};

struct umbau {
	struct umbau *next;
	html_content *htmlc;
	struct browser_window *bw;
	int *bctx;
	struct box *layout;
	struct content_html_object *objekte;
	unsigned anzahl_objekte;
	struct content_html_iframe *iframes;
	struct feld *felder;
	size_t n_felder, max_felder;
	dom_node **iframe_knoten; /* Knoten je iframe-Fenster (bw->iframes) */
	int n_iframes;
	dom_node *fokus, *auswahl, *ziehen;
};

static struct umbau *umbauten;
static unsigned neu_layouts;

unsigned ns_neu_layout_anzahl(void)
{
	return neu_layouts;
}

static struct umbau *umbau_von(html_content *htmlc)
{
	struct umbau *u;
	for (u = umbauten; u != NULL; u = u->next)
		if (u->htmlc == htmlc)
			return u;
	return NULL;
}

static void umbau_austragen(struct umbau *u)
{
	struct umbau **pp;
	for (pp = &umbauten; *pp != NULL; pp = &(*pp)->next) {
		if (*pp == u) {
			*pp = u->next;
			return;
		}
	}
}

static bool texty(const struct form_control *g)
{
	return g->type == GADGET_TEXTAREA || g->type == GADGET_TEXTBOX || g->type == GADGET_PASSWORD;
}

/* Box-Baum ohne Rekursion durchlaufen (children/next/parent) */
static struct box *naechste_box(struct box *b, struct box *wurzel)
{
	if (b->children != NULL)
		return b->children;
	while (b != NULL && b != wurzel && b->next == NULL)
		b = b->parent;
	if (b == NULL || b == wurzel)
		return NULL;
	return b->next;
}

static bool feld_merken(struct umbau *u, struct box *b)
{
	struct feld *f;
	size_t i;
	for (i = 0; i < u->n_felder; i++)
		if (u->felder[i].g == b->gadget)
			return true; /* mehrere Boxen je Feld (z. B. Knopf-Inhalt) */
	if (u->n_felder == u->max_felder) {
		size_t n = u->max_felder ? u->max_felder * 2 : 16;
		struct feld *neu = realloc(u->felder, n * sizeof *neu);
		if (neu == NULL)
			return false;
		u->felder = neu;
		u->max_felder = n;
	}
	f = &u->felder[u->n_felder++];
	memset(f, 0, sizeof *f);
	f->knoten = dom_node_ref(b->node);
	f->alte_box = b;
	f->g = b->gadget;
	if (texty(b->gadget)) {
		f->ta = b->gadget->data.text.ta;
		f->initial = b->gadget->data.text.initial;
	}
	return true;
}

static struct feld *feld_fuer(struct umbau *u, dom_node *n)
{
	size_t i;
	for (i = 0; i < u->n_felder; i++)
		if (u->felder[i].knoten == n)
			return &u->felder[i];
	return NULL;
}

/* Box-Zeiger an allen Knoten unter `n` löschen (sonst hielte dom_to_box
 * Knoten für schon umgewandelt) */
static void nutzdaten_loeschen(dom_node *wurzel)
{
	dom_node *n = dom_node_ref(wurzel), *x;
	void *alt;
	while (n != NULL) {
		dom_node_set_user_data(n, corestring_dom___ns_key_box_node_data, NULL, NULL, &alt);
		x = NULL;
		if (dom_node_get_first_child(n, &x) == DOM_NO_ERR && x != NULL) {
			dom_node_unref(n);
			n = x;
			continue;
		}
		for (;;) {
			x = NULL;
			if (n == wurzel) {
				dom_node_unref(n);
				n = NULL;
				break;
			}
			if (dom_node_get_next_sibling(n, &x) == DOM_NO_ERR && x != NULL) {
				dom_node_unref(n);
				n = x;
				break;
			}
			x = NULL;
			dom_node_get_parent_node(n, &x);
			dom_node_unref(n);
			n = x;
			if (n == NULL)
				break;
		}
	}
}

/* Box-Zeiger des (alten) Baums wieder an die Knoten hängen */
static void nutzdaten_setzen(struct box *layout)
{
	struct box *b;
	void *alt;
	for (b = layout; b != NULL; b = naechste_box(b, layout))
		if (b->node != NULL)
			dom_node_set_user_data(b->node, corestring_dom___ns_key_box_node_data, b, NULL, &alt);
}

static int objekte_zaehlen(struct content_html_object *o)
{
	int n = 0;
	for (; o != NULL; o = o->next)
		n++;
	return n;
}

/* Objektliste `liste` freigeben (über die Funktionen von object.c, die
 * auch geplante Aufrufe abmelden); offene Abrufe zählen nicht mehr */
static void objekte_freigeben(html_content *htmlc, struct content_html_object *liste, bool offen)
{
	struct content_html_object *behalten = htmlc->object_list, *o;
	for (o = liste; o != NULL; o = o->next) {
		if (o->content != NULL && o->box != NULL) {
			content_status s = content_get_status(o->content);
			if (s == CONTENT_STATUS_LOADING || s == CONTENT_STATUS_READY)
				htmlc->base.active--;
		}
	}
	htmlc->object_list = liste;
	if (offen)
		html_object_close_objects(htmlc);
	html_object_free_objects(htmlc);
	htmlc->object_list = behalten;
}

static dom_node *knoten_von(struct box *b)
{
	return (b != NULL && b->node != NULL) ? dom_node_ref(b->node) : NULL;
}

static void umbau_freigeben(struct umbau *u)
{
	size_t i;
	for (i = 0; i < u->n_felder; i++)
		dom_node_unref(u->felder[i].knoten);
	free(u->felder);
	for (i = 0; i < (size_t)u->n_iframes; i++)
		if (u->iframe_knoten[i] != NULL)
			dom_node_unref(u->iframe_knoten[i]);
	free(u->iframe_knoten);
	if (u->fokus)
		dom_node_unref(u->fokus);
	if (u->auswahl)
		dom_node_unref(u->auswahl);
	if (u->ziehen)
		dom_node_unref(u->ziehen);
	free(u);
}

/* Das Textfeld des alten Baums (getippter Text, Schreibmarke) wieder ans
 * Feld hängen; ein beim Umbau neu angelegtes wird zerstört */
static void textfeld_zurueck(struct feld *f)
{
	if (f->ta == NULL)
		return;
	if (f->g->data.text.ta != f->ta) {
		if (f->g->data.text.ta != NULL)
			textarea_destroy(f->g->data.text.ta);
		f->g->data.text.ta = f->ta;
	}
	if (f->g->data.text.initial != f->initial) {
		if (f->g->data.text.initial != NULL)
			dom_string_unref(f->g->data.text.initial);
		f->g->data.text.initial = f->initial;
	}
}

/* Alten Baum wiederherstellen (Fehler oder Abbruch des Umbaus) */
static void zurueck(struct umbau *u)
{
	html_content *htmlc = u->htmlc;
	size_t i;
	/* teilweise gebauter neuer Baum und seine Objekte */
	objekte_freigeben(htmlc, htmlc->object_list, false);
	htmlc->object_list = u->objekte;
	htmlc->num_objects = u->anzahl_objekte;
	if (htmlc->bctx != NULL && htmlc->bctx != u->bctx)
		talloc_free(htmlc->bctx);
	htmlc->bctx = u->bctx;
	htmlc->layout = u->layout;
	htmlc->iframe = u->iframes;
	/* Felder: alte Textfelder und Boxen zurück (frei stehende Felder, die
	 * der abgebrochene Umbau neu angelegt hat, hingen nur an Boxen des
	 * teilweisen Baums und sind nicht mehr erreichbar – nur bei Fehler
	 * (Speicher) oder Schliessen während des Umbaus) */
	for (i = 0; i < u->n_felder; i++)
		textfeld_zurueck(&u->felder[i]);
	for (i = 0; i < u->n_felder; i++)
		u->felder[i].g->box = u->felder[i].alte_box;
	nutzdaten_loeschen((dom_node *)htmlc->document);
	nutzdaten_setzen(htmlc->layout);
}

static void umbau_fertig(html_content *htmlc, bool ok)
{
	struct umbau *u = umbau_von(htmlc);
	struct box *b;
	size_t i;
	bool iframes_gleich;
	struct content_html_iframe *fr;
	int n_neu = 0;

	htmlc->box_conversion_context = NULL;
	if (u == NULL)
		return;
	umbau_austragen(u);
	if (!ok || htmlc->layout == u->layout) {
		NSLOG(netsurf, WARNING, "Neu-Layout fehlgeschlagen, alter Baum bleibt");
		zurueck(u);
		umbau_freigeben(u);
		return;
	}

	/* 1. Formularfelder */
	for (b = htmlc->layout; b != NULL; b = naechste_box(b, htmlc->layout)) {
		struct form_control *neu = b->gadget;
		struct feld *f;
		if (neu == NULL || b->node == NULL)
			continue;
		f = feld_fuer(u, b->node);
		if (f == NULL || f->uebernommen)
			continue;
		if (f->g == neu) {
			/* Feld eines Formulars (wiederverwendet): altes Textfeld behalten */
			textfeld_zurueck(f);
			f->uebernommen = true;
		} else if (neu->form == NULL && f->g->form == NULL && neu->type == f->g->type) {
			/* frei stehendes Feld: das alte (mit Inhalt) weiterverwenden */
			struct box *c;
			for (c = htmlc->layout; c != NULL; c = naechste_box(c, htmlc->layout))
				if (c->gadget == neu)
					c->gadget = f->g;
			f->g->box = b;
			if (htmlc->visible_select_menu == neu)
				htmlc->visible_select_menu = f->g;
			form_free_control(neu);
			f->uebernommen = true;
		}
	}
	for (i = 0; i < u->n_felder; i++) {
		struct feld *f = &u->felder[i];
		if (f->uebernommen)
			continue;
		if (f->g->form != NULL) {
			/* Feld eines Formulars ohne neue Box (entfernt/ausgeblendet) */
			if (f->g->box == f->alte_box)
				f->g->box = NULL;
			textfeld_zurueck(f);
		} else {
			/* frei stehendes Feld ohne Nachfolger */
			if (htmlc->visible_select_menu == f->g)
				htmlc->visible_select_menu = NULL;
			form_free_control(f->g);
		}
	}

	/* 2. iframes: Fenster auf die neuen Boxen umhängen */
	for (fr = htmlc->iframe; fr != NULL; fr = fr->next)
		n_neu++;
	iframes_gleich = u->bw != NULL && n_neu == u->n_iframes;
	if (iframes_gleich) {
		int k;
		for (k = 0; k < u->n_iframes && iframes_gleich; k++) {
			for (fr = htmlc->iframe; fr != NULL; fr = fr->next)
				if (fr->box != NULL && fr->box->node == u->iframe_knoten[k])
					break;
			iframes_gleich = fr != NULL;
		}
	}
	if (u->bw != NULL && u->n_iframes > 0 && iframes_gleich) {
		int k;
		for (k = 0; k < u->n_iframes; k++) {
			struct browser_window *w = &u->bw->iframes[k];
			for (fr = htmlc->iframe; fr != NULL; fr = fr->next) {
				if (fr->box != NULL && fr->box->node == u->iframe_knoten[k]) {
					w->box = fr->box;
					fr->box->iframe = w;
					break;
				}
			}
		}
	} else if (u->bw != NULL && u->n_iframes > 0) {
		/* Menge geändert: alte Fenster schliessen (solange ihre alten
		 * Boxen noch leben), neue nach dem Layout anlegen */
		browser_window_destroy_iframes(u->bw);
	}

	/* 3. Fokus, Auswahl, Ziehen */
	if (htmlc->focus_type != HTML_FOCUS_SELF) {
		struct box *n = u->fokus ? box_for_node(u->fokus) : NULL;
		if (n != NULL && (htmlc->focus_type != HTML_FOCUS_TEXTAREA || n->gadget != NULL)) {
			htmlc->focus_owner.textarea = n;
		} else {
			htmlc->focus_type = HTML_FOCUS_SELF;
			htmlc->focus_owner.self = true;
			if (u->bw != NULL)
				browser_window_remove_caret(u->bw, false);
		}
	}
	if (htmlc->selection_type == HTML_SELECTION_TEXTAREA || htmlc->selection_type == HTML_SELECTION_CONTENT) {
		struct box *n = u->auswahl ? box_for_node(u->auswahl) : NULL;
		if (n != NULL) {
			htmlc->selection_owner.textarea = n;
		} else {
			htmlc->selection_type = HTML_SELECTION_NONE;
			htmlc->selection_owner.none = true;
		}
	} else if (htmlc->selection_type == HTML_SELECTION_SELF) {
		selection_clear(htmlc->sel, false);
	}
	if (htmlc->drag_type != HTML_DRAG_NONE) {
		htmlc->drag_type = HTML_DRAG_NONE;
		htmlc->drag_owner.no_owner = true;
	}
	if (htmlc->base.textsearch.context != NULL) {
		content_textsearch_destroy(htmlc->base.textsearch.context);
		htmlc->base.textsearch.context = NULL;
	}

	/* 4. Alte Objekte und alten Baum freigeben */
	objekte_freigeben(htmlc, u->objekte, htmlc->bw != NULL);
	htmlc->num_objects = (unsigned)objekte_zaehlen(htmlc->object_list);
	talloc_free(u->bctx);
	neu_layouts++;

	/* 5. Neu layouten und zeichnen (CONTENT_MSG_REFORMAT), dann neue
	 * iframe-Fenster (brauchen die Lage ihrer Boxen) */
	content__reformat(&htmlc->base, false, htmlc->base.available_width, htmlc->base.available_height);
	if (u->bw != NULL && !iframes_gleich && n_neu > 0)
		browser_window_create_iframes(u->bw);
	umbau_freigeben(u);
	ns_js_neu_layout_fertig(htmlc);
}

bool ns_neu_layout_laeuft(html_content *htmlc)
{
	return umbau_von(htmlc) != NULL;
}

/* Neu-Layout starten; false = jetzt nicht möglich (später erneut) */
bool ns_neu_layout(html_content *htmlc)
{
	struct umbau *u;
	struct box *b;
	dom_node *html = NULL;
	css_select_ctx *auswahl = NULL;
	content_status s = htmlc->base.status;
	nserror err;
	int k;

	if (htmlc->layout == NULL || htmlc->box_conversion_context != NULL || htmlc->aborted ||
	    htmlc->bw == NULL || htmlc->document == NULL || htmlc->reflowing ||
	    (s != CONTENT_STATUS_READY && s != CONTENT_STATUS_DONE) || umbau_von(htmlc) != NULL)
		return false;
	if (dom_document_get_document_element(htmlc->document, (void *)&html) != DOM_NO_ERR || html == NULL)
		return false;
	u = calloc(1, sizeof *u);
	if (u == NULL) {
		dom_node_unref(html);
		return false;
	}
	u->htmlc = htmlc;
	u->bw = htmlc->bw;

	/* 1. Sichern */
	for (b = htmlc->layout; b != NULL; b = naechste_box(b, htmlc->layout)) {
		if (b->gadget != NULL && b->node != NULL && !feld_merken(u, b)) {
			umbau_freigeben(u);
			dom_node_unref(html);
			return false;
		}
	}
	if (u->bw->iframe_count > 0) {
		u->iframe_knoten = calloc((size_t)u->bw->iframe_count, sizeof(dom_node *));
		if (u->iframe_knoten == NULL) {
			umbau_freigeben(u);
			dom_node_unref(html);
			return false;
		}
		u->n_iframes = u->bw->iframe_count;
		for (k = 0; k < u->n_iframes; k++)
			u->iframe_knoten[k] = knoten_von(u->bw->iframes[k].box);
	}
	if (htmlc->focus_type != HTML_FOCUS_SELF)
		u->fokus = knoten_von(htmlc->focus_owner.textarea);
	if (htmlc->selection_type == HTML_SELECTION_TEXTAREA || htmlc->selection_type == HTML_SELECTION_CONTENT)
		u->auswahl = knoten_von(htmlc->selection_owner.textarea);

	/* 2. Stilauswahl neu (Stylesheets, die Skripte eingefügt haben) */
	if (html_css_new_selection_context(htmlc, &auswahl) == NSERROR_OK) {
		if (htmlc->select_ctx != NULL)
			css_select_ctx_destroy(htmlc->select_ctx);
		htmlc->select_ctx = auswahl;
	}

	u->bctx = htmlc->bctx;
	u->layout = htmlc->layout;
	u->objekte = htmlc->object_list;
	u->anzahl_objekte = htmlc->num_objects;
	u->iframes = htmlc->iframe;
	htmlc->bctx = NULL;
	htmlc->object_list = NULL;
	htmlc->num_objects = 0;
	htmlc->iframe = NULL;
	nutzdaten_loeschen((dom_node *)htmlc->document);
	u->next = umbauten;
	umbauten = u;

	err = dom_to_box(html, htmlc, umbau_fertig, &htmlc->box_conversion_context);
	dom_node_unref(html);
	if (err != NSERROR_OK) {
		umbau_austragen(u);
		htmlc->box_conversion_context = NULL;
		zurueck(u);
		umbau_freigeben(u);
		return false;
	}
	return true;
}

/* Laufenden Umbau abbrechen (Dokument wird geschlossen): alter Baum zurück */
void ns_neu_layout_abbrechen(html_content *htmlc)
{
	struct umbau *u = umbau_von(htmlc);
	if (u == NULL)
		return;
	if (htmlc->box_conversion_context != NULL) {
		cancel_dom_to_box(htmlc->box_conversion_context);
		htmlc->box_conversion_context = NULL;
	}
	umbau_austragen(u);
	zurueck(u);
	/* beim Schliessen (html_close) sind nur die Objekte des neuen Baums
	 * geschlossen worden – die wiederhergestellten alten auch schliessen */
	if (htmlc->bw == NULL && u->bw != NULL)
		html_object_close_objects(htmlc);
	umbau_freigeben(u);
}

/* Stylesheets: fertig geladene bzw. noch ladende zählen */
void ns_stylesheets_zaehlen(html_content *htmlc, unsigned *fertig, unsigned *ladend)
{
	unsigned i;
	*fertig = *ladend = 0;
	for (i = 0; i < htmlc->stylesheet_count; i++) {
		struct hlcache_handle *h = htmlc->stylesheets[i].sheet;
		if (h == NULL)
			continue;
		if (content_get_status(h) == CONTENT_STATUS_DONE)
			(*fertig)++;
		else if (content_get_status(h) != CONTENT_STATUS_ERROR)
			(*ladend)++;
	}
}
