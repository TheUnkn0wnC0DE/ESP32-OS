/* ESP32-OS (B8): JPEG- und PNG-Bilder in Webseiten über den Dekoder des
 * Systems (kolibri::dekoder, netsurf/src/lib.rs): JPEG am Board über die
 * JPEG-Einheit des P4, progressive JPEGs (B0b) und PNG in Software.
 *
 * Aufbau wie NetSurfs jpeg.c: beim Datenende nur die Masse lesen, das Bild
 * selbst dekodiert erst der Bildzwischenspeicher (image_cache), wenn es
 * gezeichnet wird – und er darf es wieder verwerfen.
 *
 * Grosse Bilder: dekodiert werden höchstens NS_BILD_MAX_PIXEL Bildpunkte;
 * die Bitmap wird um einen ganzzahligen Faktor verkleinert, bis sie
 * höchstens NS_BILD_MAX_BITMAP Bildpunkte hat (gezeichnet wird ohnehin in
 * Layout-Grösse). */
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#include "utils/utils.h"
#include "utils/log.h"
#include "utils/messages.h"
#include "netsurf/bitmap.h"
#include "content/llcache.h"
#include "content/content.h"
#include "content/content_protected.h"
#include "content/content_factory.h"
#include "desktop/gui_internal.h"
#include "desktop/bitmap.h"
#include "image/image_cache.h"
#include "image/jpeg.h"
#include "image/png.h"

/* netsurf/src/lib.rs */
bool ns_bild_masse(const uint8_t *daten, size_t laenge, uint32_t *breite, uint32_t *hoehe);
int ns_bild_dekodieren(const uint8_t *daten, size_t laenge, uint8_t *ziel, uint32_t breite, uint32_t hoehe,
		       size_t zeile, uint32_t teiler, uint8_t r, uint8_t g, uint8_t b, uint8_t a);

#ifndef NS_BILD_MAX_PIXEL
#define NS_BILD_MAX_PIXEL (4u * 1024u * 1024u)
#endif
#ifndef NS_BILD_MAX_BITMAP
#define NS_BILD_MAX_BITMAP (1024u * 1024u)
#endif

/* Verkleinerungsfaktor, damit die Bitmap in NS_BILD_MAX_BITMAP passt */
static uint32_t bild_teiler(uint32_t w, uint32_t h)
{
	uint32_t t = 1;
	while ((uint64_t)((w + t - 1) / t) * ((h + t - 1) / t) > NS_BILD_MAX_BITMAP)
		t++;
	return t;
}

static nserror bild_create(const content_handler *handler, lwc_string *imime_type,
			   const struct http_parameter *params, llcache_handle *llcache,
			   const char *fallback_charset, bool quirks, struct content **c)
{
	struct content *bild = calloc(1, sizeof(struct content));
	nserror error;
	if (bild == NULL)
		return NSERROR_NOMEM;
	error = content__init(bild, handler, imime_type, params, llcache, fallback_charset, quirks);
	if (error != NSERROR_OK) {
		free(bild);
		return error;
	}
	*c = bild;
	return NSERROR_OK;
}

static struct bitmap *bild_cache_convert(struct content *c)
{
	const uint8_t *daten;
	size_t laenge;
	uint32_t w, h, t, bw, bh;
	struct bitmap *bitmap;
	uint8_t *pixel;
	int ergebnis;
	bool opaque;

	daten = content__get_source_data(c, &laenge);
	if (daten == NULL || !ns_bild_masse(daten, laenge, &w, &h))
		return NULL;
	if (w == 0 || h == 0 || (uint64_t)w * h > NS_BILD_MAX_PIXEL)
		return NULL;
	t = bild_teiler(w, h);
	bw = (w + t - 1) / t;
	bh = (h + t - 1) / t;
	bitmap = guit->bitmap->create(bw, bh, BITMAP_NONE);
	if (bitmap == NULL)
		return NULL;
	pixel = guit->bitmap->get_buffer(bitmap);
	if (pixel == NULL) {
		guit->bitmap->destroy(bitmap);
		return NULL;
	}
	ergebnis = ns_bild_dekodieren(daten, laenge, pixel, bw, bh, guit->bitmap->get_rowstride(bitmap), t,
				      bitmap_layout.r, bitmap_layout.g, bitmap_layout.b, bitmap_layout.a);
	if (ergebnis < 0) {
		NSLOG(netsurf, INFO, "Bild nicht dekodiert (%d): %s", ergebnis,
		      nsurl_access(llcache_handle_get_url(c->llcache)));
		guit->bitmap->destroy(bitmap);
		return NULL;
	}
	/* ergebnis 1: alle Bildpunkte deckend */
	opaque = ergebnis == 1;
	guit->bitmap->set_opaque(bitmap, opaque);
	bitmap_format_to_client(bitmap, &(bitmap_fmt_t){
						.layout = bitmap_fmt.layout,
						.pma = opaque ? bitmap_fmt.pma : false,
					});
	guit->bitmap->modified(bitmap);
	return bitmap;
}

static bool bild_convert(struct content *c)
{
	const uint8_t *daten;
	size_t laenge;
	uint32_t w, h;
	union content_msg_data msg_data;

	daten = content__get_source_data(c, &laenge);
	if (daten == NULL || !ns_bild_masse(daten, laenge, &w, &h) || w == 0 || h == 0) {
		msg_data.errordata.errorcode = NSERROR_UNKNOWN;
		msg_data.errordata.errormsg = "Bildformat nicht erkannt";
		content_broadcast(c, CONTENT_MSG_ERROR, &msg_data);
		return false;
	}
	c->width = (int)w;
	c->height = (int)h;
	{
		uint32_t t = bild_teiler(w, h);
		c->size = (size_t)((w + t - 1) / t) * ((h + t - 1) / t) * 4;
	}
	image_cache_add(c, NULL, bild_cache_convert);
	content_set_ready(c);
	content_set_done(c);
	content_set_status(c, "");
	return true;
}

static nserror bild_clone(const struct content *old, struct content **newc)
{
	struct content *neu = calloc(1, sizeof(struct content));
	nserror error;
	if (neu == NULL)
		return NSERROR_NOMEM;
	error = content__clone(old, neu);
	if (error != NSERROR_OK) {
		content_destroy(neu);
		return error;
	}
	if (old->status == CONTENT_STATUS_READY || old->status == CONTENT_STATUS_DONE) {
		if (bild_convert(neu) == false) {
			content_destroy(neu);
			return NSERROR_CLONE_FAILED;
		}
	}
	*newc = neu;
	return NSERROR_OK;
}

static const content_handler bild_content_handler = {
	.create = bild_create,
	.data_complete = bild_convert,
	.destroy = image_cache_destroy,
	.redraw = image_cache_redraw,
	.clone = bild_clone,
	.get_internal = image_cache_get_internal,
	.type = image_cache_content_type,
	.is_opaque = image_cache_is_opaque,
	.no_share = false,
};

/* nsjpeg_init / nspng_init (aus image.c mit WITH_JPEG / WITH_PNG gerufen) */
static const char *nsjpeg_types[] = { "image/jpeg", "image/jpg", "image/pjpeg" };
CONTENT_FACTORY_REGISTER_TYPES(nsjpeg, nsjpeg_types, bild_content_handler);

static const char *nspng_types[] = { "image/png", "image/x-png" };
CONTENT_FACTORY_REGISTER_TYPES(nspng, nspng_types, bild_content_handler);
