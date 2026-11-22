/* ESP32-OS, B1: Prüflauf ohne Bildschirm (PC-Tests). Das unveränderte
 * Framebuffer-Frontend lädt eine Seite und zeichnet in die Prüf-Fläche
 * „probe“; nach `ms` Millisekunden meldet die Fläche „Beenden“, beim
 * Abbau wird das Bild für den Test kopiert. */
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

static long ende_ms;
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
	ende_ms = jetzt_ms() + ms;
	r = netsurf_fb_main(9, argv);
	*aus = bild;
	*b = bild_b;
	*h = bild_h;
	*zeile = bild_zeile;
	*anzahl_updates = updates;
	return r;
}
