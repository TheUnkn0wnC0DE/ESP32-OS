/* ESP32-OS (B8): Downloads. Was NetSurf nicht anzeigen kann (z. B. ZIP,
 * PDF, application/octet-stream), speichert es in den Download-Ordner
 * (ns_download_ordner, c/lesezeichen.c: am Board /daten/Downloads); die
 * Statuszeile des Tabs zeigt den Fortschritt und danach den Pfad.
 * Gibt es die Datei schon, bekommt die neue einen Zusatz „(2)“, „(3)“ … */
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libnsfb.h>
#include <libnsfb_event.h>

#include "utils/errors.h"
#include "utils/log.h"
#include "netsurf/download.h"
#include "desktop/download.h"

#include "framebuffer/gui.h"
#include "framebuffer/fbtk.h"

const char *ns_download_ordner(void);

struct gui_download_window {
	struct download_context *ctx;
	FILE *f;
	struct gui_window *gw;
	char pfad[300];
	unsigned long long geschrieben, gesamt;
	unsigned melde_kb;
	bool fehler;
};

static int anzahl_fertig;

static bool deutsch(void)
{
	const char *l = getenv("LANGUAGE");
	return l != NULL && strncmp(l, "de", 2) == 0;
}

extern struct gui_window *window_list;

/* B8-Review: Ist der Tab des Downloads noch offen? (Schliessen gibt sein
 * gui_window frei, der Download läuft weiter) */
static bool lebt(struct gui_window *gw)
{
	for (struct gui_window *g = window_list; g != NULL; g = g->next)
		if (g == gw)
			return true;
	return false;
}

static void status(struct gui_download_window *dw, const char *text)
{
	if (dw->gw != NULL && lebt(dw->gw) && dw->gw->status != NULL)
		fbtk_set_text(dw->gw->status, text);
	NSLOG(netsurf, INFO, "%s", text);
}

/* Dateiname ohne Pfadteile und ohne Zeichen, die FAT nicht mag */
static void name_saeubern(char *ziel, size_t n, const char *name)
{
	size_t k = 0;
	const char *s = strrchr(name, '/');
	s = s != NULL ? s + 1 : name;
	for (; *s && k + 1 < n; s++) {
		unsigned char c = (unsigned char)*s;
		ziel[k++] = (c < 32 || strchr("\\:*?\"<>|", c) != NULL) ? '_' : (char)c;
	}
	ziel[k] = '\0';
	if (k == 0 || strcmp(ziel, ".") == 0 || strcmp(ziel, "..") == 0)
		snprintf(ziel, n, "download");
}

static bool gibt_es(const char *p)
{
	FILE *f = fopen(p, "rb");
	if (f == NULL)
		return false;
	fclose(f);
	return true;
}

static struct gui_download_window *dl_create(struct download_context *ctx, struct gui_window *parent)
{
	const char *ordner = ns_download_ordner();
	struct gui_download_window *dw;
	char name[120], basis[120];
	const char *punkt;
	if (ordner == NULL)
		return NULL;
	dw = calloc(1, sizeof *dw);
	if (dw == NULL)
		return NULL;
	dw->ctx = ctx;
	dw->gw = parent;
	dw->gesamt = download_context_get_total_length(ctx);
	name_saeubern(name, sizeof name, download_context_get_filename(ctx));
	snprintf(dw->pfad, sizeof dw->pfad, "%s/%s", ordner, name);
	/* vorhandene Datei nicht überschreiben: „name (2).ext“ … */
	punkt = strrchr(name, '.');
	snprintf(basis, sizeof basis, "%.*s", punkt != NULL ? (int)(punkt - name) : (int)strlen(name), name);
	for (int i = 2; gibt_es(dw->pfad) && i < 100; i++)
		snprintf(dw->pfad, sizeof dw->pfad, "%s/%s (%d)%s", ordner, basis, i, punkt != NULL ? punkt : "");
	/* B8-Review: auch „(99)“ vorhanden – nichts überschreiben */
	if (gibt_es(dw->pfad)) {
		status(dw, deutsch() ? "Download: kein freier Dateiname" : "Download: no free file name");
		free(dw);
		return NULL;
	}
	dw->f = fopen(dw->pfad, "wb");
	if (dw->f == NULL) {
		char t[340];
		snprintf(t, sizeof t, deutsch() ? "Download: %s nicht anlegbar" : "Download: cannot create %s", dw->pfad);
		status(dw, t);
		free(dw);
		return NULL;
	}
	{
		char t[340];
		snprintf(t, sizeof t, deutsch() ? "Download: %s …" : "Download: %s …", name);
		status(dw, t);
	}
	return dw;
}

static nserror dl_data(struct gui_download_window *dw, const char *data, unsigned int size)
{
	if (dw->fehler)
		return NSERROR_SAVE_FAILED;
	if (fwrite(data, 1, size, dw->f) != size) {
		/* NetSurf bricht den Abruf ab und meldet sich danach nicht mehr:
		 * halbe Datei gleich weg (der kleine Kontext bleibt liegen) */
		dw->fehler = true;
		fclose(dw->f);
		dw->f = NULL;
		remove(dw->pfad);
		status(dw, deutsch() ? "Download: Schreiben fehlgeschlagen (Speicher voll?)"
				     : "Download: write failed (storage full?)");
		return NSERROR_SAVE_FAILED;
	}
	dw->geschrieben += size;
	/* Fortschritt höchstens alle 64 KB */
	if (dw->geschrieben / 65536 != dw->melde_kb) {
		char t[120];
		dw->melde_kb = (unsigned)(dw->geschrieben / 65536);
		if (dw->gesamt > 0)
			snprintf(t, sizeof t, "Download: %llu / %llu KB", dw->geschrieben / 1024, dw->gesamt / 1024);
		else
			snprintf(t, sizeof t, "Download: %llu KB", dw->geschrieben / 1024);
		status(dw, t);
	}
	return NSERROR_OK;
}

/* nach einem Fehler kommt kein „done“ mehr: hier aufräumen */
static void dl_error(struct gui_download_window *dw, const char *error_msg)
{
	char t[200];
	if (dw->f != NULL) {
		fclose(dw->f);
		remove(dw->pfad);
	}
	snprintf(t, sizeof t, deutsch() ? "Download fehlgeschlagen: %s" : "Download failed: %s",
		 error_msg != NULL ? error_msg : "");
	status(dw, t);
	download_context_destroy(dw->ctx);
	free(dw);
}

static void dl_done(struct gui_download_window *dw)
{
	char t[360];
	/* B8-Review: kürzer als angekündigt (z. B. Zwischenspeicher ohne
	 * Speicher abgebrochen) gilt als fehlgeschlagen */
	bool ok = dw->f != NULL && !dw->fehler && (dw->gesamt == 0 || dw->geschrieben >= dw->gesamt);
	if (dw->f != NULL && fclose(dw->f) != 0)
		ok = false;
	dw->f = NULL;
	if (ok) {
		anzahl_fertig++;
		snprintf(t, sizeof t, deutsch() ? "Gespeichert: %s (%llu KB)" : "Saved: %s (%llu KB)", dw->pfad,
			 (dw->geschrieben + 1023) / 1024);
	} else {
		/* halbe Datei nicht liegen lassen */
		remove(dw->pfad);
		snprintf(t, sizeof t, deutsch() ? "Download abgebrochen" : "Download aborted");
	}
	status(dw, t);
	download_context_destroy(dw->ctx);
	free(dw);
}

struct gui_download_table ns_download_tabelle = {
	.create = dl_create,
	.data = dl_data,
	.error = dl_error,
	.done = dl_done,
};

/* für Tests: fertige Downloads seit dem Start */
int ns_downloads_fertig(void)
{
	return anzahl_fertig;
}
