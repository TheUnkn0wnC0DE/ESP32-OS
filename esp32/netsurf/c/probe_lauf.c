/* ESP32-OS, B1: Prüflauf ohne Bildschirm (PC-Tests). Das unveränderte
 * Framebuffer-Frontend lädt eine Seite und zeichnet in die Prüf-Fläche
 * „probe“; nach `ms` Millisekunden meldet die Fläche „Beenden“, beim
 * Abbau wird das Bild für den Test kopiert.
 *
 * B4: Eingaben per Umgebungsvariable NS_TEST_EINGABE, Schritte mit „;“
 * getrennt, Zeit in ms ab Start:
 *   „ms:x:y“      Tippen (Zeiger hin, Taste 1 drücken und loslassen)
 *   „ms:T:text“   Text tippen, „\n“ = Eingabetaste, „\b“ = Rücktaste
 * B5:
 *   „ms:S:px:x:y“ Wischen: um px scrollen (Zeiger bei x, y), wie die Fläche
 *                 am Board über c/touch.c
 *   „ms:Z:prozent“ Pinch-Zoom um den Faktor prozent/100
 * B8:
 *   „ms:D:x0:y0:x1:y1“ Ziehen mit gedrückter Taste 1 (Text markieren), wie
 *                 die Geste „Halten, dann Ziehen“ am Board */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <libnsfb.h>
#include <libnsfb_plot.h>
#include <libnsfb_event.h>
#include "nsfb.h"
#include "surface.h"
#include "plot.h"

int netsurf_fb_main(int argc, char **argv);

#ifdef _WIN32
/* Sleep aus kernel32 (ohne <windows.h>, das u. a. „small“ umdefiniert) */
__declspec(dllimport) void __stdcall Sleep(unsigned long ms);
#define kurz_warten() Sleep(5)
#else
#include <unistd.h>
#define kurz_warten() usleep(5000)
#endif

static long ende_ms, start_ms;
static long jetzt_ms(void);

/* Eingabe-Skript (NS_TEST_EINGABE) und Warteschlange der Ereignisse */
#define MAX_SCHRITTE 32
#define MAX_EREIGNISSE 512
static struct {
	long ms;
	char art; /* 'K' Tippen, 'T' Text, 'S' Scrollen, 'Z' Zoom, 'D' Ziehen */
	int x, y, wert, x1, y1;
	char text[96];
} schritte[MAX_SCHRITTE];

/* c/touch.c (B5) */
void ns_touch_scroll_px(int px);
void ns_touch_zoom(double faktor);
static int n_schritte, naechster_schritt;
static nsfb_event_t ereignisse[MAX_EREIGNISSE];
static int erstes, letztes;

static void skript_lesen(void)
{
	const char *e = getenv("NS_TEST_EINGABE");
	n_schritte = naechster_schritt = 0;
	erstes = letztes = 0;
	while (e != NULL && *e && n_schritte < MAX_SCHRITTE) {
		const char *ende = strchr(e, ';');
		size_t l = ende ? (size_t)(ende - e) : strlen(e);
		char teil[128];
		if (l >= sizeof teil)
			l = sizeof teil - 1;
		memcpy(teil, e, l);
		teil[l] = '\0';
		schritte[n_schritte].text[0] = '\0';
		schritte[n_schritte].art = 'K';
		if (strstr(teil, ":D:") != NULL &&
		    sscanf(teil, "%ld:D:%d:%d:%d:%d", &schritte[n_schritte].ms, &schritte[n_schritte].x,
			   &schritte[n_schritte].y, &schritte[n_schritte].x1, &schritte[n_schritte].y1) == 5) {
			schritte[n_schritte].art = 'D';
			n_schritte++;
		} else if (strstr(teil, ":S:") != NULL &&
		    sscanf(teil, "%ld:S:%d:%d:%d", &schritte[n_schritte].ms, &schritte[n_schritte].wert,
			   &schritte[n_schritte].x, &schritte[n_schritte].y) == 4) {
			schritte[n_schritte].art = 'S';
			n_schritte++;
		} else if (strstr(teil, ":Z:") != NULL &&
			   sscanf(teil, "%ld:Z:%d", &schritte[n_schritte].ms, &schritte[n_schritte].wert) == 2) {
			schritte[n_schritte].art = 'Z';
			n_schritte++;
		} else if (sscanf(teil, "%ld:T:", &schritte[n_schritte].ms) == 1 && strstr(teil, ":T:") != NULL) {
			schritte[n_schritte].art = 'T';
			const char *t = strstr(teil, ":T:") + 3;
			size_t k = 0;
			while (*t && k + 1 < sizeof schritte[n_schritte].text) {
				if (t[0] == '\\' && t[1] == 'n') {
					schritte[n_schritte].text[k++] = '\n';
					t += 2;
				} else if (t[0] == '\\' && t[1] == 'b') {
					schritte[n_schritte].text[k++] = '\b';
					t += 2;
				} else if (t[0] == '\\' && t[1] == 'u' && strlen(t) >= 6 && k + 4 < sizeof schritte[n_schritte].text) {
					/* „\uXXXX“: Zeichen als UTF-8 (Umgebungsvariablen
					 * bleiben so reines ASCII) */
					char hex[5] = { t[2], t[3], t[4], t[5], 0 };
					unsigned u = (unsigned)strtoul(hex, NULL, 16);
					if (u < 0x80) {
						schritte[n_schritte].text[k++] = (char)u;
					} else if (u < 0x800) {
						schritte[n_schritte].text[k++] = (char)(0xC0 | (u >> 6));
						schritte[n_schritte].text[k++] = (char)(0x80 | (u & 0x3F));
					} else {
						schritte[n_schritte].text[k++] = (char)(0xE0 | (u >> 12));
						schritte[n_schritte].text[k++] = (char)(0x80 | ((u >> 6) & 0x3F));
						schritte[n_schritte].text[k++] = (char)(0x80 | (u & 0x3F));
					}
					t += 6;
				} else {
					schritte[n_schritte].text[k++] = *t++;
				}
			}
			schritte[n_schritte].text[k] = '\0';
			n_schritte++;
		} else if (sscanf(teil, "%ld:%d:%d", &schritte[n_schritte].ms, &schritte[n_schritte].x,
				  &schritte[n_schritte].y) == 3) {
			n_schritte++;
		}
		e = ende ? ende + 1 : NULL;
	}
}

static void dazu(enum nsfb_event_type_e typ, int wert, int x, int y)
{
	nsfb_event_t *ev;
	if ((letztes + 1) % MAX_EREIGNISSE == erstes)
		return;
	ev = &ereignisse[letztes];
	memset(ev, 0, sizeof *ev);
	ev->type = typ;
	if (typ == NSFB_EVENT_MOVE_ABSOLUTE) {
		ev->value.vector.x = x;
		ev->value.vector.y = y;
	} else {
		ev->value.keycode = wert;
	}
	letztes = (letztes + 1) % MAX_EREIGNISSE;
}

static void schritte_ausloesen(void)
{
	while (naechster_schritt < n_schritte && jetzt_ms() >= start_ms + schritte[naechster_schritt].ms) {
		const char *t = schritte[naechster_schritt].text;
		int x = schritte[naechster_schritt].x, y = schritte[naechster_schritt].y;
		if (schritte[naechster_schritt].art == 'K') {
			dazu(NSFB_EVENT_MOVE_ABSOLUTE, 0, x, y);
			dazu(NSFB_EVENT_KEY_DOWN, NSFB_KEY_MOUSE_1, 0, 0);
			dazu(NSFB_EVENT_KEY_UP, NSFB_KEY_MOUSE_1, 0, 0);
		} else if (schritte[naechster_schritt].art == 'S') {
			/* wie die Fläche am Board: Pixel sammeln, ein Mausrad-Ereignis */
			int px = schritte[naechster_schritt].wert;
			int rad = px < 0 ? NSFB_KEY_MOUSE_4 : NSFB_KEY_MOUSE_5;
			ns_touch_scroll_px(px);
			dazu(NSFB_EVENT_MOVE_ABSOLUTE, 0, x, y);
			dazu(NSFB_EVENT_KEY_DOWN, rad, 0, 0);
			dazu(NSFB_EVENT_KEY_UP, rad, 0, 0);
		} else if (schritte[naechster_schritt].art == 'D') {
			/* gedrückt halten, in 10 Schritten hinziehen, loslassen */
			int x1 = schritte[naechster_schritt].x1, y1 = schritte[naechster_schritt].y1;
			dazu(NSFB_EVENT_MOVE_ABSOLUTE, 0, x, y);
			dazu(NSFB_EVENT_KEY_DOWN, NSFB_KEY_MOUSE_1, 0, 0);
			for (int i = 1; i <= 10; i++)
				dazu(NSFB_EVENT_MOVE_ABSOLUTE, 0, x + (x1 - x) * i / 10, y + (y1 - y) * i / 10);
			dazu(NSFB_EVENT_KEY_UP, NSFB_KEY_MOUSE_1, 0, 0);
		} else if (schritte[naechster_schritt].art == 'Z') {
			ns_touch_zoom(schritte[naechster_schritt].wert / 100.0);
			/* ein Ereignis, damit die Hauptschleife einen Durchlauf macht */
			dazu(NSFB_EVENT_MOVE_ABSOLUTE, 0, x, y);
		} else {
			while (*t) {
				/* druckbare Zeichen wie am Board als Unicode-Code (B8);
				 * der Text ist UTF-8 */
				unsigned char b = (unsigned char)*t++;
				int u = b, k;
				if (b >= 0xC0) {
					int n = b >= 0xF0 ? 3 : b >= 0xE0 ? 2 : 1;
					u = b & (0x3F >> n);
					while (n-- > 0 && (*t & 0xC0) == 0x80)
						u = (u << 6) | (*t++ & 0x3F);
				}
				k = u == '\n' ? NSFB_KEY_RETURN : u == '\b' ? NSFB_KEY_BACKSPACE : 0x10000 + u;
				dazu(NSFB_EVENT_KEY_DOWN, k, 0, 0);
				dazu(NSFB_EVENT_KEY_UP, k, 0, 0);
			}
		}
		naechster_schritt++;
	}
}
static uint8_t *bild;
static int bild_b, bild_h, bild_zeile;
static int updates;

static long jetzt_ms(void)
{
	struct timespec t;
	timespec_get(&t, TIME_UTC);
	return (long)(t.tv_sec * 1000 + t.tv_nsec / 1000000);
}

static int probe_defaults(nsfb_t *nsfb)
{
	nsfb->width = 0;
	nsfb->height = 0;
	nsfb->format = NSFB_FMT_XBGR8888;
	select_plotters(nsfb);
	return 0;
}

static int probe_initialise(nsfb_t *nsfb)
{
	size_t n = (size_t)nsfb->width * nsfb->height * nsfb->bpp / 8;
	uint8_t *p = realloc(nsfb->ptr, n);
	if (p == NULL)
		return -1;
	memset(p, 0, n);
	nsfb->ptr = p;
	nsfb->linelen = nsfb->width * nsfb->bpp / 8;
	return 0;
}

static int probe_geometry(nsfb_t *nsfb, int width, int height, enum nsfb_format_e format)
{
	if (width > 0)
		nsfb->width = width;
	if (height > 0)
		nsfb->height = height;
	if (format != NSFB_FMT_ANY)
		nsfb->format = format;
	select_plotters(nsfb);
	if (nsfb->ptr != NULL)
		return probe_initialise(nsfb);
	nsfb->linelen = nsfb->width * nsfb->bpp / 8;
	return 0;
}

static int probe_finalise(nsfb_t *nsfb)
{
	size_t n = (size_t)nsfb->linelen * nsfb->height;
	free(bild);
	bild = malloc(n);
	if (bild != NULL && nsfb->ptr != NULL) {
		memcpy(bild, nsfb->ptr, n);
		bild_b = nsfb->width;
		bild_h = nsfb->height;
		bild_zeile = nsfb->linelen;
	}
	free(nsfb->ptr);
	nsfb->ptr = NULL;
	return 0;
}

static bool probe_input(nsfb_t *nsfb, nsfb_event_t *event, int timeout)
{
	(void)nsfb;
	if (jetzt_ms() >= ende_ms) {
		event->type = NSFB_EVENT_CONTROL;
		event->value.controlcode = NSFB_CONTROL_QUIT;
		return true;
	}
	schritte_ausloesen();
	if (erstes != letztes) {
		*event = ereignisse[erstes];
		erstes = (erstes + 1) % MAX_EREIGNISSE;
		return true;
	}
	if (timeout != 0)
		kurz_warten();
	return false;
}

static int probe_update(nsfb_t *nsfb, nsfb_bbox_t *box)
{
	(void)nsfb;
	(void)box;
	updates++;
	return 0;
}

static const nsfb_surface_rtns_t probe_rtns = {
	.defaults = probe_defaults,
	.initialise = probe_initialise,
	.finalise = probe_finalise,
	.input = probe_input,
	.geometry = probe_geometry,
	.update = probe_update,
};

/* Umgebungsvariable für die C-Laufzeit setzen (unter Windows liest getenv
 * eine eigene Kopie der Umgebung; std::env::set_var erreicht sie nicht). */
int ns_probe_setenv(const char *name, const char *wert)
{
#ifdef _WIN32
	return _putenv_s(name, wert);
#else
	return setenv(name, wert, 1);
#endif
}

/* Seite `url` in einem Fenster breite × hoehe laden, nach `ms` beenden;
 * js != 0 schaltet JavaScript ein (Option enable_javascript).
 * Liefert den Rückgabewert des Frontends; *aus zeigt auf das Bild
 * (XBGR8888, gehört dem Prüflauf bis zum nächsten Aufruf). */
int ns_probe_lauf(const char *url, int breite, int hoehe, int ms, int js,
		  const uint8_t **aus, int *b, int *h, int *zeile, int *anzahl_updates)
{
	static bool angemeldet;
	char wtext[16], htext[16], jstext[32];
	char *argv[] = { "netsurf", jstext, "-f", "probe", "-w", wtext, "-h", htext, (char *)url, NULL };
	int r;
	snprintf(jstext, sizeof jstext, "--enable_javascript=%d", js ? 1 : 0);
	if (!angemeldet) {
		/* Typ ABLE wird sonst nicht gebaut; gewählt wird über den Namen */
		_nsfb_register_surface(NSFB_SURFACE_ABLE, &probe_rtns, "probe");
		angemeldet = true;
	}
	snprintf(wtext, sizeof wtext, "%d", breite);
	snprintf(htext, sizeof htext, "%d", hoehe);
	updates = 0;
	start_ms = jetzt_ms();
	ende_ms = start_ms + ms;
	skript_lesen();
	r = netsurf_fb_main(9, argv);
	*aus = bild;
	*b = bild_b;
	*h = bild_h;
	*zeile = bild_zeile;
	*anzahl_updates = updates;
	return r;
}
