/* ESP32-OS, B1: libnsfb-Fläche „esp32os“ und Einstieg des Programms.
 *
 * NetSurf (Framebuffer-Frontend, unverändert) zeichnet in einen Puffer im
 * Format RGB565 – dem Format der Anzeige. Die Fläche bringt ihn über die
 * Kern-Funktion 65 in ein Fenster über die ganze Arbeitsfläche (ohne
 * Taskleiste) und übersetzt die Ereignisse des Kerns (Funktion 23):
 * Neuzeichnen (selbst erledigt), Maus/Touch → Bewegung + Taste 1,
 * Mausrad → Tasten 4/5, Tasten → nsfb-Tastencodes, Schliessen → Beenden.
 * Nur 32 Bit (Kern-Systemaufrufe über die native ksys.h).
 */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* direkt (nicht über den Suchpfad: kolibri/include hätte auch ein
 * KolibriOS-sys/socket.h, das gui.c fälschlich fände) */
#include "../../kolibri/include/sys/ksys.h"

#include <libnsfb.h>
#include <libnsfb_plot.h>
#include <libnsfb_event.h>
#include "nsfb.h"
#include "surface.h"
#include "plot.h"

int netsurf_fb_main(int argc, char **argv);

/* Stil: skinned (3); X-Bits Titel (1), Koordinaten im Arbeitsbereich (2),
 * Arbeitsbereich nicht füllen (4) */
#define STIL 0x73
#define TITEL "NetSurf"

static int fx, fy, fw, fh;	/* Fenster (aussen) */
static bool zu;			/* Schliessen gedrückt */
static uint32_t knoepfe;	/* letzter Stand der Maustasten */
static int mx = -1, my = -1;	/* letzte Mausposition im Arbeitsbereich */
/* Warteschlange: ein Kern-Ereignis kann mehrere nsfb-Ereignisse ergeben */
static nsfb_event_t schlange[16];
static int s_anfang, s_ende;

static void einreihen(enum nsfb_event_type_e typ, int wert)
{
	int n = (s_ende + 1) % (int)(sizeof schlange / sizeof schlange[0]);
	if (n == s_anfang)
		return; /* voll: Ereignis verwerfen */
	memset(&schlange[s_ende], 0, sizeof schlange[0]);
	schlange[s_ende].type = typ;
	schlange[s_ende].value.keycode = wert;
	s_ende = n;
}

static void bewegung(int x, int y)
{
	int n = (s_ende + 1) % (int)(sizeof schlange / sizeof schlange[0]);
	if (n == s_anfang)
		return;
	memset(&schlange[s_ende], 0, sizeof schlange[0]);
	schlange[s_ende].type = NSFB_EVENT_MOVE_ABSOLUTE;
	schlange[s_ende].value.vector.x = x;
	schlange[s_ende].value.vector.y = y;
	schlange[s_ende].value.vector.z = 0;
	s_ende = n;
}

static bool holen(nsfb_event_t *e)
{
	if (s_anfang == s_ende)
		return false;
	*e = schlange[s_anfang];
	s_anfang = (s_anfang + 1) % (int)(sizeof schlange / sizeof schlange[0]);
	return true;
}

/* Arbeitsfläche des Bildschirms ohne Taskleiste (Funktion 48.5) */
static void arbeitsflaeche(int *x, int *y, int *w, int *h)
{
	ksys_regs_t r = { 0 };
	r.eax = 48;
	r.ebx = 5;
	kos_syscall(&r);
	*x = (int)(r.eax >> 16);
	*w = (int)(r.eax & 0xFFFF) - *x + 1;
	*y = (int)(r.ebx >> 16);
	*h = (int)(r.ebx & 0xFFFF) - *y + 1;
}

static void fenster_zeichnen(nsfb_t *nsfb)
{
	_ksys_start_draw();
	_ksys_create_window((uint32_t)fx, (uint32_t)fy, (uint32_t)fw, (uint32_t)fh, TITEL, 0xFFFFFF, STIL);
	if (nsfb->ptr != NULL)
		ksys_draw_bitmap_palette(nsfb->ptr, 0, 0, nsfb->width, nsfb->height, 16, NULL, 0);
	_ksys_end_draw();
}

/* B5: Bereich verschieben (Scrollen) – statt libnsfbs copy() aus
 * plot/generic.c: verschiebt sich der Bereich senkrecht, sind Quell- und
 * Zielzeile verschieden, also memcpy je Zeile (oben beginnend, wenn es nach
 * oben geht, sonst unten). Am Board ist memmove in Vorwärtsrichtung viel
 * langsamer als memcpy (gemessen 233 KB: 5–31 ms gegen 0,8 ms für 129 KB). */
static bool kopieren(nsfb_t *nsfb, nsfb_bbox_t *q, nsfb_bbox_t *z)
{
	int w = z->x1 - z->x0, h = z->y1 - z->y0;
	size_t zeile = (size_t)w * (size_t)nsfb->bpp / 8;
	uint8_t *s, *d;
	nsfb_bbox_t alles;
	if (w <= 0 || h <= 0 || nsfb->ptr == NULL)
		return true;
	nsfb_plot_add_rect(q, z, &alles);
	nsfb->surface_rtns->claim(nsfb, &alles);
	s = nsfb->ptr + (size_t)q->y0 * nsfb->linelen + (size_t)q->x0 * nsfb->bpp / 8;
	d = nsfb->ptr + (size_t)z->y0 * nsfb->linelen + (size_t)z->x0 * nsfb->bpp / 8;
	if (q->y0 > z->y0) {
		for (int i = 0; i < h; i++)
			memcpy(d + (size_t)i * nsfb->linelen, s + (size_t)i * nsfb->linelen, zeile);
	} else if (q->y0 < z->y0) {
		for (int i = h - 1; i >= 0; i--)
			memcpy(d + (size_t)i * nsfb->linelen, s + (size_t)i * nsfb->linelen, zeile);
	} else {
		for (int i = 0; i < h; i++)
			memmove(d + (size_t)i * nsfb->linelen, s + (size_t)i * nsfb->linelen, zeile);
	}
	nsfb->surface_rtns->update(nsfb, z);
	return true;
}

/* libnsfbs Zeichenfunktionen wählen, dann das Verschieben ersetzen */
static void zeichner_waehlen(nsfb_t *nsfb)
{
	select_plotters(nsfb);
	if (nsfb->plotter_fns != NULL)
		nsfb->plotter_fns->copy = kopieren;
}

static int e_defaults(nsfb_t *nsfb)
{
	nsfb->width = 640;
	nsfb->height = 360;
	nsfb->format = NSFB_FMT_RGB565;
	nsfb->bpp = 16;
	zeichner_waehlen(nsfb);
	return 0;
}

static int puffer_anlegen(nsfb_t *nsfb)
{
	size_t n = (size_t)nsfb->width * (size_t)nsfb->height * 2;
	uint8_t *p = realloc(nsfb->ptr, n);
	if (p == NULL)
		return -1;
	memset(p, 0xFF, n);
	nsfb->ptr = p;
	nsfb->linelen = nsfb->width * 2;
	return 0;
}

/* Fenster anlegen (vor NetSurf, damit das Frontend die echte Grösse als
 * -w/-h bekommt) und Grösse des Arbeitsbereichs vom Kern (Funktion 9) */
static void fenster_anlegen(int *breite, int *hoehe)
{
	ksys_thread_t info;
	ksys_regs_t r = { 0 };
	int i, schirm_h;
	/* Die Taskleiste trägt ihre Höhe erst nach ihrem Start ein (Funktion
	 * 48.6). Startet der Browser direkt nach dem Booten, wartet er bis zu
	 * 3 s darauf – sonst läge der untere Fensterrand hinter der Leiste
	 * (das Framebuffer-Frontend kann seine Grösse danach nicht ändern). */
	r.eax = 14;
	kos_syscall(&r);
	schirm_h = (int)(r.eax & 0xFFFF) + 1;
	arbeitsflaeche(&fx, &fy, &fw, &fh);
	for (i = 0; i < 30 && fy + fh >= schirm_h; i++) {
		_ksys_delay(10);
		arbeitsflaeche(&fx, &fy, &fw, &fh);
	}
	_ksys_set_event_mask(KSYS_EVM_REDRAW | KSYS_EVM_KEY | KSYS_EVM_BUTTON | KSYS_EVM_MOUSE);
	_ksys_set_key_input_mode(KSYS_KEY_INPUT_MODE_ASCII);
	_ksys_start_draw();
	_ksys_create_window((uint32_t)fx, (uint32_t)fy, (uint32_t)fw, (uint32_t)fh, TITEL, 0xFFFFFF, STIL);
	_ksys_end_draw();
	memset(&info, 0, sizeof info);
	_ksys_thread_info(&info, -1);
	*breite = info.clientwidth > 0 ? info.clientwidth : 640;
	*hoehe = info.clientheight > 0 ? info.clientheight : 360;
}

static int e_initialise(nsfb_t *nsfb)
{
	/* nsfb->width/height: vom Frontend (-w/-h = Arbeitsbereich) */
	nsfb->ptr = NULL;
	nsfb->format = NSFB_FMT_RGB565;
	nsfb->bpp = 16;
	zeichner_waehlen(nsfb);
	zu = false;
	knoepfe = 0;
	s_anfang = s_ende = 0;
	if (puffer_anlegen(nsfb) != 0)
		return -1;
	fenster_zeichnen(nsfb);
	return 0;
}

static void ausgabe_verwerfen(void);

static int e_finalise(nsfb_t *nsfb)
{
	ausgabe_verwerfen();
	free(nsfb->ptr);
	nsfb->ptr = NULL;
	return 0;
}

static int e_geometry(nsfb_t *nsfb, int width, int height, enum nsfb_format_e format)
{
	ausgabe_verwerfen(); /* Puffer wird neu angelegt */
	/* Grösse vom Frontend (= Arbeitsbereich); Format bleibt RGB565 (das
	 * Format der Anzeige) */
	(void)format;
	if (width > 0)
		nsfb->width = width;
	if (height > 0)
		nsfb->height = height;
	nsfb->format = NSFB_FMT_RGB565;
	nsfb->bpp = 16;
	zeichner_waehlen(nsfb);
	return 0;
}

/* KolibriOS-Tastencode (ASCII-Modus) → nsfb */
/* netsurf/src/lib.rs: Zeichen der Kodierung cp866 als Unicode */
uint32_t ns_cp866_zu_uni(uint8_t c);

static int taste_nsfb(uint8_t c)
{
	switch (c) {
	case 8: return NSFB_KEY_BACKSPACE;
	case 9: return NSFB_KEY_TAB;
	case 13: return NSFB_KEY_RETURN;
	case 27: return NSFB_KEY_ESCAPE;
	case 0xB0: return NSFB_KEY_LEFT;
	case 0xB1: return NSFB_KEY_DOWN;
	case 0xB2: return NSFB_KEY_UP;
	case 0xB3: return NSFB_KEY_RIGHT;
	case 0xB4: return NSFB_KEY_HOME;
	case 0xB5: return NSFB_KEY_END;
	case 0xB6: return NSFB_KEY_DELETE;
	case 0xB7: return NSFB_KEY_PAGEDOWN;
	case 0xB8: return NSFB_KEY_PAGEUP;
	default:
		/* B8: Zeichen als Unicode-Code ab 0x10000 (fbtk_event.c-Kopie
		 * reicht sie durch) – nsfb hat keine Codes für Grossbuchstaben,
		 * Umlaute, „%“, „{|}~“; cp866 über die Tabelle des Kerns */
		if (c >= 32 && c < 127)
			return 0x10000 + c;
		if (c >= 128)
			return 0x10000 + (int)ns_cp866_zu_uni(c);
		return -1;
	}
}

/* Eine Maustaste nachführen: `bit` im Zustand, Ereignis-Bits von 37.3
 * (gedrückt bei bit << 8, losgelassen bei bit << 16). Ein kurzes Tippen, das
 * schon vorbei ist, wenn der Browser liest (Layout dauert), kommt so trotzdem
 * als Drücken + Loslassen an. */
static void taste_nachfuehren(uint32_t e, uint32_t bit, int taste)
{
	bool unten = (knoepfe & bit) != 0;
	bool jetzt = (e & bit) != 0;
	bool gedrueckt = (e & (bit << 8)) != 0, los = (e & (bit << 16)) != 0;
	if (unten) {
		if (los) {
			einreihen(NSFB_EVENT_KEY_UP, taste);
			unten = false;
		}
		if (gedrueckt && !unten) {
			einreihen(NSFB_EVENT_KEY_DOWN, taste);
			unten = true;
		}
	} else {
		if (gedrueckt) {
			einreihen(NSFB_EVENT_KEY_DOWN, taste);
			unten = true;
		}
		if (los && unten) {
			einreihen(NSFB_EVENT_KEY_UP, taste);
			unten = false;
		}
	}
	if (unten != jetzt)
		einreihen(jetzt ? NSFB_EVENT_KEY_DOWN : NSFB_EVENT_KEY_UP, taste);
}

/* netsurf/src/lib.rs (B8): Lage des letzten Drückens (Arbeitsbereich) */
bool ns_druck_lage_holen(int *x, int *y);
/* c/touch.c (B5) */
void ns_touch_scroll_schritte(int schritte);
void ns_touch_zoom(double faktor);
/* netsurf/src/lib.rs: Zoom-Geste des Kerns (Tausendstel, Mitte) */
void ns_zoom_holen(uint32_t *faktor, int *x, int *y);
#ifdef ESP_PLATFORM
static int takt_scroll;
static int wisch_neu; /* Scrollschritte seit dem letzten takt() */
#endif

static void maus(void)
{
	ksys_pos_t p = _ksys_get_mouse_pos(KSYS_MOUSE_WINDOW_POS);
	uint32_t e = _ksys_get_mouse_eventstate();
	uint32_t rad = _ksys_get_mouse_wheels();
	int16_t dy = (int16_t)(rad & 0xFFFF);
	/* B8: neu gedrückt? Erst an die Stelle des Drückens (der Kern merkt sie
	 * sich), dann drücken, dann weiter zur jetzigen Lage – sonst begänne
	 * Markieren (Halten, dann Ziehen) dort, wo der Finger beim Abfragen
	 * schon ist */
	if (e & (1u << 8)) {
		int dx, dyy;
		if (ns_druck_lage_holen(&dx, &dyy) && (dx != mx || dyy != my)) {
			mx = dx;
			my = dyy;
			bewegung(mx, my);
		}
	}
	if (p.x != mx || p.y != my) {
		/* Lage nach dem Drücken erst nach der Taste */
		if (!(e & (1u << 8))) {
			mx = p.x;
			my = p.y;
			bewegung(mx, my);
		}
	}
	taste_nachfuehren(e, 1, NSFB_KEY_MOUSE_1);
	if (p.x != mx || p.y != my) {
		mx = p.x;
		my = p.y;
		bewegung(mx, my);
	}
	taste_nachfuehren(e, 2, NSFB_KEY_MOUSE_3);
	knoepfe = e & 3;
	/* B5: alle Scrollschritte als Pixel sammeln (c/touch.c) und nur ein
	 * Mausrad-Ereignis schicken – die gui.c-Kopie scrollt um alles
	 * Gesammelte, auch was während des Neuzeichnens dazukam */
	if (dy != 0) {
		int t = dy < 0 ? NSFB_KEY_MOUSE_4 : NSFB_KEY_MOUSE_5;
		ns_touch_scroll_schritte(dy);
		einreihen(NSFB_EVENT_KEY_DOWN, t);
		einreihen(NSFB_EVENT_KEY_UP, t);
#ifdef ESP_PLATFORM
		takt_scroll += dy;
		wisch_neu += dy;
#endif
	}
	/* B5: Pinch-Zoom der Desktop-Gesten (Faktor in Tausendstel) */
	{
		uint32_t f;
		int zx, zy;
		ns_zoom_holen(&f, &zx, &zy);
		if (f != 1000 && f != 0)
			ns_touch_zoom((double)f / 1000.0);
	}
}

#ifdef ESP_PLATFORM
/* Board: alle 5 s eine Zeile, wie viel der Browser rechnet und wie lange
 * er auf Ereignisse wartet (Messung zu B2: langsame Seiten) */
#include "esp_log.h"
#include "esp_timer.h"
static int64_t takt_start, takt_warten;
static unsigned takt_runden, takt_bilder;
static bool takt_bild;
static int64_t takt_bild_zeit; /* letztes Bild (e_update) */
void ns_heap_stand_kb(uint32_t *belegt, uint32_t *spitze);
/* Q3 (c/css_luecken.c): nicht verstandene CSS-Deklarationen */
unsigned ns_css_luecken_stand(void);
unsigned ns_css_luecken_text(char *aus, size_t max, unsigned anzahl);
void ns_css_vorbereitung_stand(unsigned *variablen, unsigned *ersetzt, unsigned *unbekannt, unsigned *calc,
			       unsigned *farben, unsigned *logisch);
static unsigned css_stand_gemeldet;
void ns_llcache_stand(uint32_t *groesse, uint32_t *grenze, uint32_t *frei);

/* B5: Bilder je Wischbewegung – vom ersten Scrollschritt bis zum letzten
 * Bild danach; eine Bewegung endet 300 ms nach dem letzten Scrollschritt */
static int64_t wisch_start, wisch_letzter_schritt, wisch_letztes_bild;
static unsigned wisch_bilder;
static int wisch_schritte;

/* gui.c-Kopie (build.rs): Zeiten je Bild beim Zeichnen */
extern int64_t ns_mess_kopie_us, ns_mess_seite_us, ns_mess_ausgabe_us;
extern int64_t ns_mess_planer_us, ns_mess_zeichnen_us, ns_mess_scroll_us;
extern unsigned ns_mess_seite_n, ns_mess_seite_px;
static int64_t mess_flaeche_us; /* e_update: an den Kern (Funktion 65) */
/* netsurf/src/lib.rs: Zeichenaufrufe des Kerns (Warten auf die Sperre,
 * Arbeit darin, Anzahl) seit dem letzten Abholen */
void ns_kern_zeichnen_messung(uint32_t *warten, uint32_t *arbeit, uint32_t *aufrufe);

/* einmal beim Start: wie schnell ist ein grosses memmove im PSRAM? */
static void speicher_messen(void)
{
	size_t n = 630 * 210 * 2;
	uint8_t *p = malloc(n + 4096);
	if (p == NULL)
		return;
	memset(p, 0x55, n + 4096);
	int64_t t0 = esp_timer_get_time();
	memmove(p, p + 2 * 630 * 20, n - 2 * 630 * 20);
	int64_t t1 = esp_timer_get_time();
	memmove(p + 2 * 630 * 20, p, n - 2 * 630 * 20);
	int64_t t2 = esp_timer_get_time();
	memcpy(p, p + n / 2, n / 2);
	int64_t t3 = esp_timer_get_time();
	ESP_LOGI("ns_takt", "Speicher: memmove %u KB vor %lld us, zurück %lld us, memcpy %u KB %lld us",
		 (unsigned)((n - 2 * 630 * 20) / 1024), (long long)(t1 - t0), (long long)(t2 - t1),
		 (unsigned)(n / 2048), (long long)(t3 - t2));
	free(p);
}

static void wischen_abschliessen(void);

static void wischen_messen(int64_t jetzt, bool bild)
{
	/* Bild zuerst: es gehört zu einer schon laufenden Bewegung */
	if (bild && wisch_start != 0) {
		wisch_bilder++;
		wisch_letztes_bild = takt_bild_zeit;
	}
	/* eine Bewegung endet 300 ms nach ihrem letzten Schritt – auch wenn der
	 * Browser so lange schlief und erst der nächste Schritt ihn weckt */
	if (wisch_start != 0 && jetzt - wisch_letzter_schritt >= 300000)
		wischen_abschliessen();
	if (wisch_neu != 0) {
		if (wisch_start == 0) {
			wisch_start = jetzt;
			wisch_letztes_bild = jetzt;
			wisch_bilder = 0;
			wisch_schritte = 0;
			ns_mess_kopie_us = ns_mess_seite_us = ns_mess_ausgabe_us = mess_flaeche_us = 0;
			ns_mess_planer_us = ns_mess_zeichnen_us = ns_mess_scroll_us = 0;
			ns_mess_seite_n = ns_mess_seite_px = 0;
			{
				uint32_t w, a, n;
				ns_kern_zeichnen_messung(&w, &a, &n);
			}
		}
		wisch_schritte += wisch_neu;
		wisch_neu = 0;
		wisch_letzter_schritt = jetzt;
	}
	if (wisch_start != 0 && jetzt - wisch_letzter_schritt >= 300000)
		wischen_abschliessen();
}

static void wischen_abschliessen(void)
{
	{
		int64_t dauer_ms = (wisch_letztes_bild - wisch_start) / 1000;
		if (dauer_ms < 1)
			dauer_ms = 1;
		ESP_LOGI("ns_takt", "Wischen: %d Schritte, %u Bilder in %lld ms (%u.%u Bilder/s)", wisch_schritte,
			 wisch_bilder, (long long)dauer_ms, (unsigned)(wisch_bilder * 1000 / dauer_ms),
			 (unsigned)(wisch_bilder * 10000 / dauer_ms % 10));
		ESP_LOGI("ns_takt",
			 "Wischen je Bild: Verschieben %lld us, Seite %lld us (%u Bereiche, %u px), Ausgabe %lld us, "
			 "davon Kern %lld us",
			 (long long)(ns_mess_kopie_us / (wisch_bilder ? wisch_bilder : 1)),
			 (long long)(ns_mess_seite_us / (wisch_bilder ? wisch_bilder : 1)), ns_mess_seite_n,
			 ns_mess_seite_px / (wisch_bilder ? wisch_bilder : 1),
			 (long long)(ns_mess_ausgabe_us / (wisch_bilder ? wisch_bilder : 1)),
			 (long long)(mess_flaeche_us / (wisch_bilder ? wisch_bilder : 1)));
		{
			uint32_t w, a, n;
			ns_kern_zeichnen_messung(&w, &a, &n);
			ESP_LOGI("ns_takt", "Wischen Kern: %u Zeichenaufrufe, Sperre warten %u us, Arbeit %u us (Summe)", n, w,
				 a);
			ESP_LOGI("ns_takt", "Wischen Hauptschleife (Summe): Planer %lld us, Scrollen %lld us, fbtk_redraw %lld us",
				 (long long)ns_mess_planer_us, (long long)ns_mess_scroll_us, (long long)ns_mess_zeichnen_us);
		}
		wisch_start = 0;
	}
}

static void takt(int64_t warten_us)
{
	int64_t jetzt = esp_timer_get_time();
	if (takt_start == 0) {
		takt_start = jetzt;
		speicher_messen();
	}
	takt_warten += warten_us;
	takt_runden++;
	wischen_messen(jetzt, takt_bild);
	if (takt_bild) {
		takt_bilder++;
		takt_bild = false;
	}
	if (jetzt - takt_start >= 5000000) {
		uint32_t belegt, spitze, cache, grenze, frei;
		ns_heap_stand_kb(&belegt, &spitze);
		ns_llcache_stand(&cache, &grenze, &frei);
		int64_t dauer_ms = (jetzt - takt_start) / 1000;
		ESP_LOGI("ns_takt",
			 "%u Runden, Arbeit %lld ms, Warten %lld ms, %u Bilder (%u.%u/s), Scroll %d Schritte, Heap %u KB "
			 "(Spitze %u KB), Cache %u KB (freigebbar %u KB, Grenze %u KB)",
			 takt_runden, (long long)((jetzt - takt_start - takt_warten) / 1000), (long long)(takt_warten / 1000),
			 takt_bilder, (unsigned)(takt_bilder * 1000 / dauer_ms), (unsigned)(takt_bilder * 10000 / dauer_ms % 10),
			 takt_scroll, (unsigned)belegt, (unsigned)spitze, (unsigned)(cache / 1024), (unsigned)(frei / 1024),
			 (unsigned)(grenze / 1024));
		if (ns_css_luecken_stand() != css_stand_gemeldet) {
			static char css_text[640];
			unsigned arten = ns_css_luecken_text(css_text, sizeof css_text, 25);
			css_stand_gemeldet = ns_css_luecken_stand();
			ESP_LOGI("ns_css", "Lücken (%u Arten, %u Meldungen): %s", arten, css_stand_gemeldet, css_text);
			{
				unsigned v, e, u, c, f, l;
				ns_css_vorbereitung_stand(&v, &e, &u, &c, &f, &l);
				ESP_LOGI("ns_css", "Vorbereitet: %u Variablen, var() %u ersetzt / %u unbekannt, calc() %u, Farben %u, logisch %u",
					 v, e, u, c, f, l);
			}
		}
		takt_start = jetzt;
		takt_warten = 0;
		takt_runden = 0;
		takt_bilder = 0;
		takt_scroll = 0;
	}
}
#endif

static void ausgeben(void);

static bool e_input(nsfb_t *nsfb, nsfb_event_t *event, int timeout)
{
	uint32_t ev;
	/* Gezeichnetes aus dem letzten Durchlauf an den Kern */
	ausgeben();
	if (holen(event))
		return true;
	if (zu) {
		event->type = NSFB_EVENT_CONTROL;
		event->value.controlcode = NSFB_CONTROL_QUIT;
		return true;
	}
	/* timeout in ms (< 0: unbegrenzt); Funktion 23 zählt in 1/100 s */
	/* 0: nur nachsehen (Funktion 11) – Funktion 23 mit 0 meldet im Kern
	 * immer „kein Ereignis“; während langer Arbeit (Box-Aufbau in vielen
	 * Schritten) kämen sonst keine Eingaben an */
#ifdef ESP_PLATFORM
	int64_t warten_ab = esp_timer_get_time();
#endif
	if (timeout == 0)
		ev = _ksys_check_event();
	else if (timeout < 0)
		ev = _ksys_wait_event_timeout(100);
	else
		ev = _ksys_wait_event_timeout((uint32_t)((timeout + 9) / 10));
#ifdef ESP_PLATFORM
	int64_t gewartet = esp_timer_get_time() - warten_ab;
#endif
	switch (ev) {
	case KSYS_EVENT_REDRAW:
		fenster_zeichnen(nsfb);
		break;
	case KSYS_EVENT_KEY: {
		ksys_oskey_t k = _ksys_get_key();
		int c;
		if (k.state == 1)
			break;
		/* Zustand 3: Unicode in den oberen 16 Bit (Zeichen ohne cp866) */
		c = k.state == 3 ? 0x10000 + (int)(k.val >> 16) : taste_nsfb(k.code);
		if (c >= 0) {
			einreihen(NSFB_EVENT_KEY_DOWN, c);
			einreihen(NSFB_EVENT_KEY_UP, c);
		}
		break;
	}
	case KSYS_EVENT_BUTTON:
		if (_ksys_get_button() == 1)
			zu = true; /* Schliessen-Knopf */
		break;
	case KSYS_EVENT_MOUSE:
		maus();
		break;
	default:
		break;
	}
#ifdef ESP_PLATFORM
	takt(gewartet);
#endif
	if (zu) {
		event->type = NSFB_EVENT_CONTROL;
		event->value.controlcode = NSFB_CONTROL_QUIT;
		return true;
	}
	return holen(event);
}

/* B5: geänderte Rechtecke eines Durchlaufs sammeln und einmal an den Kern
 * geben (ausgeben() am Anfang von e_input) – beim Scrollen kamen sonst je
 * Bild drei Aufrufe (verschobener Teil, neuer Streifen, Rollbalken), die
 * jeder auf die Sperre des Compositors warten */
static nsfb_t *offen_fb;
static int offen_x0, offen_y0, offen_x1, offen_y1;
static bool offen_da;

/* Puffer weg oder neu: Gesammeltes nicht mehr ausgeben */
static void ausgabe_verwerfen(void)
{
	offen_da = false;
	offen_fb = NULL;
}

static void ausgeben(void)
{
	nsfb_t *nsfb = offen_fb;
	int x0 = offen_x0, y0 = offen_y0, x1 = offen_x1, y1 = offen_y1;
	if (!offen_da || nsfb == NULL || nsfb->ptr == NULL)
		return;
	offen_da = false;
#ifdef ESP_PLATFORM
	takt_bild = true;
	int64_t mess_ab = esp_timer_get_time();
#endif
	/* Teilrechteck: Zeilenabstand über den Versatz (Funktion 65, ebp) */
	ksys_draw_bitmap_palette(nsfb->ptr + (size_t)y0 * nsfb->linelen + (size_t)x0 * 2, x0, y0, x1 - x0, y1 - y0, 16, NULL,
				 nsfb->linelen - (x1 - x0) * 2);
#ifdef ESP_PLATFORM
	takt_bild_zeit = esp_timer_get_time();
	mess_flaeche_us += takt_bild_zeit - mess_ab;
#endif
}

static int e_update(nsfb_t *nsfb, nsfb_bbox_t *box)
{
	int x0 = box->x0 < 0 ? 0 : box->x0, y0 = box->y0 < 0 ? 0 : box->y0;
	int x1 = box->x1 > nsfb->width ? nsfb->width : box->x1;
	int y1 = box->y1 > nsfb->height ? nsfb->height : box->y1;
	if (x1 <= x0 || y1 <= y0 || nsfb->ptr == NULL)
		return 0;
	if (!offen_da || offen_fb != nsfb) {
		ausgeben();
		offen_fb = nsfb;
		offen_x0 = x0;
		offen_y0 = y0;
		offen_x1 = x1;
		offen_y1 = y1;
		offen_da = true;
		return 0;
	}
	if (x0 < offen_x0)
		offen_x0 = x0;
	if (y0 < offen_y0)
		offen_y0 = y0;
	if (x1 > offen_x1)
		offen_x1 = x1;
	if (y1 > offen_y1)
		offen_y1 = y1;
	return 0;
}

/* libnsfb/src/surface/ram.c */
extern const nsfb_surface_rtns_t ram_rtns;

static const nsfb_surface_rtns_t esp32os_rtns = {
	.defaults = e_defaults,
	.initialise = e_initialise,
	.finalise = e_finalise,
	.input = e_input,
	.geometry = e_geometry,
	.update = e_update,
};

/* Einstieg des Programms „browser“ (vom Rust-Programm nach dem
 * Zurückspielen der statischen Daten gerufen). */
void ns_transport_neue_sitzung(void); /* transport.h */

int ns_esp32os_main(const char *url)
{
	/* -b 16: das Frontend wählt Formate und Plotter passend zur Fläche;
	 * -w/-h: Arbeitsbereich des Fensters */
	static char breite[12], hoehe[12];
	static char *argv[] = { "netsurf", "--enable_javascript=1", "-f", "esp32os", "-b", "16", "-w", breite, "-h", hoehe, NULL, NULL };
	int b, h;
	/* Entwicklung (BR1): ohne JavaScript, wenn NS_JS_AUS gesetzt */
	if (getenv("NS_JS_AUS") != NULL)
		argv[1] = "--enable_javascript=0";
	/* Abrufe eines früheren (z. B. zwangsbeendeten) Browsers verwerfen */
	ns_transport_neue_sitzung();
	_nsfb_register_surface(NSFB_SURFACE_ABLE, &esp32os_rtns, "esp32os");
	/* Bitmaps (Bilder, Baumansicht der Lesezeichen) legt das Frontend auf
	 * libnsfbs RAM-Fläche an. ram.c meldet sie über einen Konstruktor an –
	 * am Board ist diese Anmeldung beim Programmstart nicht mehr da (die
	 * statischen Daten liegen im PSRAM und werden erst danach gesetzt);
	 * ohne sie scheitert jedes guit->bitmap->create (B4, gemessen). */
	if (nsfb_type_from_name("ram") == NSFB_SURFACE_NONE)
		_nsfb_register_surface(NSFB_SURFACE_RAM, &ram_rtns, "ram");
	fenster_anlegen(&b, &h);
	snprintf(breite, sizeof breite, "%d", b);
	snprintf(hoehe, sizeof hoehe, "%d", h);
	/* ohne Adresse: Startseite mit Suchfeld (netsurf/res, B2) */
	argv[10] = (char *)(url != NULL && url[0] != '\0' ? url : "resource:welcome.html");
	return netsurf_fb_main(11, argv);
}
