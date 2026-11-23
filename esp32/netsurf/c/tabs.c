/* ESP32-OS (B8): Tabs für das Framebuffer-Frontend.
 *
 * Jedes Browserfenster des Kerns (struct gui_window) ist ein Tab; sichtbar
 * ist nur der aktive, die anderen sind ausgeblendet (fbtk_set_mapping).
 * Neue Tabs entstehen über „+ Neuer Tab“ in der Tab-Liste und über Links
 * mit target="_blank" (der Kern legt dann selbst ein Fenster an).
 *
 * Der Knopf „Tabs n“ (gui.c-Kopie, ersetzt NetSurfs Knopf „Fenster
 * schliessen“ – das Fenster hat seinen eigenen Schliessen-Knopf) öffnet die
 * Liste: je Tab eine Zeile mit Titel (antippen = dorthin wechseln) und
 * „X“ (Tab schliessen), oben „+ Neuer Tab“ und „Zurück“.
 *
 * Die Knöpfe „Ohne JS“ und „Merken“ gibt es je Tab; ihre Widgets merkt
 * sich diese Datei (die gui.c-Kopie fragt hier nach). */
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "utils/nsoption.h"
#include "utils/nsurl.h"
#include "netsurf/browser_window.h"
#include "netsurf/window.h"
#include "desktop/gui_internal.h"

#include <libnsfb.h>
#include <libnsfb_event.h>

#include "framebuffer/gui.h"
#include "framebuffer/fbtk.h"
#include "framebuffer/schedule.h"

#define MAX_TABS 16
/* höchstens so viele über „+ Neuer Tab“ – jede Seite braucht Heap */
#define NEU_HOECHSTENS 5
#define ZEILE_H 34
#define X_BREITE 44

extern struct gui_window *window_list;
extern fbtk_widget_t *fbtk;

static struct {
	struct gui_window *gw;
	fbtk_widget_t *js, *merken;
} knoepfe[MAX_TABS];

static struct gui_window *aktiv;
static fbtk_widget_t *liste;

static bool deutsch(void)
{
	const char *l = getenv("LANGUAGE");
	return l != NULL && strncmp(l, "de", 2) == 0;
}

static int anzahl(void)
{
	int n = 0;
	for (struct gui_window *g = window_list; g != NULL; g = g->next)
		n++;
	return n;
}

/* Knöpfe eines Tabs merken bzw. holen ('j' Ohne JS, 'm' Merken) */
void ns_tab_knopf_setzen(struct gui_window *gw, char art, fbtk_widget_t *knopf)
{
	int frei = -1;
	for (int i = 0; i < MAX_TABS; i++) {
		if (knoepfe[i].gw == gw) {
			frei = i;
			break;
		}
		if (frei < 0 && knoepfe[i].gw == NULL)
			frei = i;
	}
	if (frei < 0)
		return;
	knoepfe[frei].gw = gw;
	if (art == 'j')
		knoepfe[frei].js = knopf;
	else
		knoepfe[frei].merken = knopf;
}

fbtk_widget_t *ns_tab_knopf(struct gui_window *gw, char art)
{
	for (int i = 0; i < MAX_TABS; i++)
		if (gw != NULL && knoepfe[i].gw == gw)
			return art == 'j' ? knoepfe[i].js : knoepfe[i].merken;
	return NULL;
}

/* aktiver Tab (für Zoom, Knopftexte und Tests) */
struct gui_window *ns_tab_aktiv(void)
{
	return aktiv != NULL ? aktiv : window_list;
}

struct browser_window *ns_tab_bw(void)
{
	struct gui_window *g = ns_tab_aktiv();
	return g != NULL ? g->bw : NULL;
}

/* höchste Zahl gleichzeitiger Tabs (für Tests) */
static int hoechststand;

/* Beschriftung „Tabs n“ auf allen Tab-Knöpfen (gw->close) */
static void knopftexte(void)
{
	static char text[16];
	if (anzahl() > hoechststand)
		hoechststand = anzahl();
	snprintf(text, sizeof text, "Tabs %d", anzahl());
	for (struct gui_window *g = window_list; g != NULL; g = g->next)
		if (g->close != NULL)
			fbtk_set_text(g->close, text);
}

static void zeigen(struct gui_window *gw)
{
	aktiv = gw;
	for (struct gui_window *g = window_list; g != NULL; g = g->next)
		if (g != gw)
			fbtk_set_mapping(g->window, false);
	if (gw != NULL) {
		fbtk_set_mapping(gw->window, true);
		fbtk_set_zorder(gw->window, -1000); /* ganz nach vorn */
		fbtk_request_redraw(gw->window);
		/* die Seite selbst neu zeichnen (fbtk zeichnet nur seine Widgets,
		 * NetSurf nur vorgemerkte Bereiche) */
		guit->window->invalidate(gw, NULL);
	}
}

/* gui_window_create (gui.c-Kopie): ein neues Fenster ist der neue Tab */
void ns_tab_neu(struct gui_window *gw)
{
	zeigen(gw);
	knopftexte();
}

/* gui_window_destroy (gui.c-Kopie), nachdem gw aus window_list ist */
void ns_tab_weg(struct gui_window *gw)
{
	for (int i = 0; i < MAX_TABS; i++)
		if (knoepfe[i].gw == gw)
			memset(&knoepfe[i], 0, sizeof knoepfe[i]);
	if (aktiv == gw || aktiv == NULL)
		zeigen(window_list);
	knopftexte();
}

/* --- Tab-Liste --------------------------------------------------------- */

static void liste_abbauen(void *p)
{
	(void)p;
	if (liste != NULL) {
		fbtk_destroy_widget(liste);
		liste = NULL;
	}
	if (aktiv != NULL)
		fbtk_request_redraw(aktiv->window);
}

/* aus einem Klick der Liste heraus: erst ausblenden, später abbauen */
static void liste_schliessen(void)
{
	if (liste != NULL)
		fbtk_set_mapping(liste, false);
	framebuffer_schedule(0, liste_abbauen, NULL);
}

static void liste_oeffnen(struct gui_window *gw);

static int wahl_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)
{
	(void)widget;
	if (cbi->event->type != NSFB_EVENT_KEY_UP)
		return 0;
	liste_schliessen();
	for (struct gui_window *g = window_list; g != NULL; g = g->next)
		if (g == cbi->context)
			zeigen(g);
	return 1;
}

static void tab_schliessen_spaeter(void *p)
{
	struct gui_window *gw = p;
	for (struct gui_window *g = window_list; g != NULL; g = g->next) {
		if (g != gw)
			continue;
		if (anzahl() > 1) {
			browser_window_destroy(gw->bw);
		} else {
			/* letzter Tab: bleibt, zeigt die Startseite */
			const char *start = nsoption_charp(homepage_url);
			nsurl *url;
			if (start == NULL || start[0] == '\0')
				start = NETSURF_HOMEPAGE;
			if (nsurl_create(start, &url) == NSERROR_OK) {
				browser_window_navigate(gw->bw, url, NULL, BW_NAVIGATE_HISTORY, NULL, NULL, NULL);
				nsurl_unref(url);
			}
		}
		break;
	}
	if (window_list != NULL && anzahl() > 0)
		liste_oeffnen(ns_tab_aktiv());
}

static int x_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)
{
	(void)widget;
	if (cbi->event->type != NSFB_EVENT_KEY_UP)
		return 0;
	liste_schliessen();
	framebuffer_schedule(0, tab_schliessen_spaeter, cbi->context);
	return 1;
}

static void neu_spaeter(void *p)
{
	const char *start = nsoption_charp(homepage_url);
	nsurl *url;
	(void)p;
	if (start == NULL || start[0] == '\0')
		start = NETSURF_HOMEPAGE;
	if (nsurl_create(start, &url) == NSERROR_OK) {
		browser_window_create(BW_CREATE_HISTORY, url, NULL, NULL, NULL);
		nsurl_unref(url);
	}
}

static int neu_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)
{
	(void)widget;
	if (cbi->event->type != NSFB_EVENT_KEY_UP)
		return 0;
	liste_schliessen();
	if (anzahl() < NEU_HOECHSTENS)
		framebuffer_schedule(0, neu_spaeter, NULL);
	return 1;
}

static int zurueck_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)
{
	(void)widget;
	if (cbi->event->type != NSFB_EVENT_KEY_UP)
		return 0;
	liste_schliessen();
	return 1;
}

static void liste_oeffnen(struct gui_window *gw)
{
	int x, y, b, h, zeile = 0;
	fbtk_widget_t *k;
	if (gw == NULL || liste != NULL)
		return;
	/* über dem Seitenbereich des aktiven Tabs */
	x = fbtk_get_absx(gw->browser);
	y = fbtk_get_absy(gw->browser);
	b = fbtk_get_width(gw->browser);
	h = fbtk_get_height(gw->browser);
	liste = fbtk_create_window(fbtk, x, y, b, h, FB_FRAME_COLOUR);
	if (liste == NULL)
		return;
	k = fbtk_create_text_button(liste, 4, 4, b / 2 - 6, ZEILE_H - 4, FB_FRAME_COLOUR, FB_COLOUR_BLACK, neu_klick,
				    NULL);
	fbtk_set_text(k, anzahl() < NEU_HOECHSTENS ? (deutsch() ? "+ Neuer Tab" : "+ New tab")
						   : (deutsch() ? "(höchstens 5 Tabs)" : "(at most 5 tabs)"));
	k = fbtk_create_text_button(liste, b / 2 + 2, 4, b / 2 - 6, ZEILE_H - 4, FB_FRAME_COLOUR, FB_COLOUR_BLACK,
				    zurueck_klick, NULL);
	fbtk_set_text(k, deutsch() ? "Zurück" : "Back");
	for (struct gui_window *g = window_list; g != NULL; g = g->next) {
		int zy = 4 + (++zeile) * ZEILE_H;
		const char *titel = browser_window_get_title(g->bw);
		static char texte[MAX_TABS][96];
		char *t = texte[(zeile - 1) % MAX_TABS];
		if (zy + ZEILE_H > h)
			break;
		if (titel == NULL || titel[0] == '\0') {
			nsurl *u = browser_window_access_url(g->bw);
			titel = u != NULL ? nsurl_access(u) : "";
		}
		snprintf(t, sizeof texte[0], "%s%s", g == ns_tab_aktiv() ? "> " : "", titel);
		k = fbtk_create_text_button(liste, 4, zy, b - X_BREITE - 12, ZEILE_H - 4, FB_COLOUR_WHITE,
					    FB_COLOUR_BLACK, wahl_klick, g);
		fbtk_set_text(k, t);
		k = fbtk_create_text_button(liste, b - X_BREITE - 4, zy, X_BREITE, ZEILE_H - 4, FB_FRAME_COLOUR,
					    FB_COLOUR_BLACK, x_klick, g);
		fbtk_set_text(k, "X");
	}
	fbtk_set_zorder(liste, -1000);
	fbtk_set_mapping(liste, true);
	fbtk_request_redraw(liste);
}

/* Knopf „Tabs n“ */
int ns_tabs_klick(fbtk_widget_t *widget, fbtk_callback_info *cbi)
{
	(void)widget;
	if (cbi->event->type != NSFB_EVENT_KEY_UP)
		return 0;
	if (liste != NULL)
		liste_schliessen();
	else
		liste_oeffnen(cbi->context);
	return 1;
}

/* für Tests: höchste Zahl gleichzeitiger Tabs seit dem letzten Abholen */
int ns_tabs_hoechststand(void)
{
	int h = hoechststand;
	hoechststand = 0;
	return h;
}
