/* ESP32-OS, B1: Prüfung von libnsfb (PC-Tests): RAM-Fläche anlegen,
 * Rechteck und Linie zeichnen, Pixel zurücklesen. */
#include <stdbool.h>
#include <stdint.h>
#include <libnsfb.h>
#include <libnsfb_plot.h>

/* Bindet ram.c (Fläche registriert sich per Konstruktor) mit ein. */
extern const struct nsfb_surface_rtns_s ram_rtns;
const void *ns_probe_fb_anker = &ram_rtns;

/* Liefert 0 bei Erfolg, sonst die Nummer der fehlgeschlagenen Prüfung. */
int ns_probe_fb(uint32_t *innen, uint32_t *aussen, uint32_t *linie)
{
	nsfb_t *fb = nsfb_new(NSFB_SURFACE_RAM);
	nsfb_bbox_t r = { 10, 5, 20, 15 };
	nsfb_bbox_t l = { 0, 30, 63, 30 };
	nsfb_plot_pen_t stift = { 0 };
	uint8_t *p;
	int zeile;
	if (fb == NULL)
		return 1;
	if (nsfb_set_geometry(fb, 64, 32, NSFB_FMT_XBGR8888) != 0 || nsfb_init(fb) != 0) {
		nsfb_free(fb);
		return 2;
	}
	nsfb_plot_rectangle_fill(fb, &(nsfb_bbox_t){ 0, 0, 64, 32 }, 0xFFFFFFFF);
	/* nsfb_colour_t: 0xAABBGGRR */
	if (!nsfb_plot_rectangle_fill(fb, &r, 0xFF0000FF)) {
		nsfb_free(fb);
		return 3;
	}
	stift.stroke_type = NFSB_PLOT_OPTYPE_SOLID;
	stift.stroke_width = 1;
	stift.stroke_colour = 0xFF00FF00;
	nsfb_plot_line(fb, &l, &stift);
	if (nsfb_get_buffer(fb, &p, &zeile) != 0) {
		nsfb_free(fb);
		return 4;
	}
	*innen = *(uint32_t *)(p + 10 * zeile + 15 * 4);
	*aussen = *(uint32_t *)(p + 2 * zeile + 2 * 4);
	*linie = *(uint32_t *)(p + 30 * zeile + 40 * 4);
	nsfb_free(fb);
	return 0;
}
