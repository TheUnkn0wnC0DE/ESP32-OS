/* ESP32-OS, Browser B5 (Auftrag 13.9): Touch-Scrollen und Pinch-Zoom.
 *
 * Die Fläche (am Board c/flaeche_esp32os.c, in den PC-Tests das
 * Eingabe-Skript in c/probe_lauf.c) meldet hier, was die Gesten des
 * Desktops liefern:
 * - Wischen: Scrollschritte des Kerns (Funktion 37.7, ein Schritt =
 *   NS_SCROLL_SCHRITT logische Pixel Fingerweg). Die Pixel werden gesammelt;
 *   die Fläche schickt nur ein Mausrad-Ereignis, die gui.c-Kopie scrollt um
 *   alles Gesammelte. So folgt die Seite dem Finger 1:1, und Schritte, die
 *   während eines Neuzeichnens ankommen, werden zu einem einzigen Schieben
 *   zusammengefasst (das Frontend verschiebt den sichtbaren Teil und
 *   zeichnet nur den neuen Streifen).
 * - Zwei Finger: Zoomfaktor (Tausendstel); die gui.c-Kopie wendet ihn nach
 *   jedem Ereignis-Durchlauf auf das Browserfenster an (0,5- bis 3-fach).
 */
#include <stdbool.h>

/* Fingerweg je Scrollschritt (desktop/src/gesten.rs, SCROLL_SCHRITT); die
 * Seite ist im selben logischen Massstab gezeichnet */
#define NS_SCROLL_SCHRITT 12

static int scroll_px;
static double zoom_f = 1.0;
static bool zoom_neu;

void ns_touch_scroll_schritte(int schritte)
{
	scroll_px += schritte * NS_SCROLL_SCHRITT;
}

void ns_touch_scroll_px(int px)
{
	scroll_px += px;
}

/* gui.c-Kopie beim Mausrad-Ereignis: gesammelte Pixel (0 = echtes Mausrad,
 * dann gilt NetSurfs eigener Schritt) */
int ns_touch_scroll_nehmen(void)
{
	int r = scroll_px;
	scroll_px = 0;
	return r;
}

void ns_touch_zoom(double faktor)
{
	if (faktor > 0.0 && faktor != 1.0) {
		zoom_f *= faktor;
		zoom_neu = true;
	}
}

bool ns_touch_zoom_nehmen(double *faktor)
{
	if (!zoom_neu)
		return false;
	*faktor = zoom_f;
	zoom_f = 1.0;
	zoom_neu = false;
	return true;
}
