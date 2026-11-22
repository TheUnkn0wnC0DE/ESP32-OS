/* ESP32-OS: iconv für NetSurf auf den Codecs von libparserutils.
 *
 * Zeichenweise: Die Bytes genau eines Quellzeichens werden nach UCS-4
 * dekodiert und in den Zielzeichensatz kodiert. So zeigt *ein bei einem
 * Fehler genau auf das betroffene Zeichen – NetSurf ersetzt dann z. B. in
 * Formularen nicht darstellbare Zeichen durch "&#x…;" (utils/utf8.c).
 *
 * Fehler wie POSIX: EILSEQ (ungültig bzw. nicht darstellbar), EINVAL
 * (unvollständiges Zeichen am Ende), E2BIG (Ausgabe voll). */
#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <parserutils/charset/codec.h>
#include <parserutils/charset/mibenum.h>
#include "iconv.h"

enum art { EIN_BYTE, UTF8, UTF16_BE, UTF16_LE };

struct esp32os_iconv {
	parserutils_charset_codec *von;
	parserutils_charset_codec *nach;
	enum art art;
};

/* Name ohne "//…"-Zusatz kopieren; 0 = zu lang/leer. */
static int name_ohne_zusatz(const char *name, char *n, size_t max)
{
	size_t l = strcspn(name, "/");
	if (l == 0 || l >= max)
		return 0;
	memcpy(n, name, l);
	n[l] = '\0';
	return 1;
}

static parserutils_charset_codec *codec(const char *name, parserutils_charset_codec_errormode modus)
{
	char n[64];
	parserutils_charset_codec *c = NULL;
	parserutils_charset_codec_optparams p;
	if (!name_ohne_zusatz(name, n, sizeof n))
		return NULL;
	if (parserutils_charset_codec_create(n, &c) != PARSERUTILS_OK)
		return NULL;
	p.error_mode.mode = modus;
	parserutils_charset_codec_setopt(c, PARSERUTILS_CHARSET_CODEC_ERROR_MODE, &p);
	return c;
}

static enum art art_von(const char *name)
{
	char n[64];
	uint16_t mib;
	if (!name_ohne_zusatz(name, n, sizeof n))
		return EIN_BYTE;
	mib = parserutils_charset_mibenum_from_name(n, strlen(n));
	if (mib == parserutils_charset_mibenum_from_name("UTF-8", 5))
		return UTF8;
	if (mib == parserutils_charset_mibenum_from_name("UTF-16LE", 8))
		return UTF16_LE;
	if (mib == parserutils_charset_mibenum_from_name("UTF-16BE", 8) ||
	    mib == parserutils_charset_mibenum_from_name("UTF-16", 6))
		return UTF16_BE;
	return EIN_BYTE;
}

iconv_t iconv_open(const char *nach, const char *von)
{
	struct esp32os_iconv *k;
	parserutils_charset_codec_errormode modus = PARSERUTILS_CHARSET_CODEC_ERROR_STRICT;
	if (strstr(nach, "//TRANSLIT"))
		modus = PARSERUTILS_CHARSET_CODEC_ERROR_TRANSLIT;
	else if (strstr(nach, "//IGNORE"))
		modus = PARSERUTILS_CHARSET_CODEC_ERROR_LOOSE;
	k = calloc(1, sizeof *k);
	if (k == NULL) {
		errno = ENOMEM;
		return (iconv_t)-1;
	}
	k->von = codec(von, PARSERUTILS_CHARSET_CODEC_ERROR_STRICT);
	k->nach = codec(nach, modus);
	k->art = art_von(von);
	if (k->von == NULL || k->nach == NULL) {
		if (k->von)
			parserutils_charset_codec_destroy(k->von);
		if (k->nach)
			parserutils_charset_codec_destroy(k->nach);
		free(k);
		errno = EINVAL;
		return (iconv_t)-1;
	}
	return (iconv_t)k;
}

/* Bytelänge des nächsten Quellzeichens (0 = ungültiges Startbyte). */
static size_t zeichen_laenge(enum art art, const uint8_t *s, size_t n)
{
	switch (art) {
	case UTF8:
		if (s[0] < 0x80)
			return 1;
		if ((s[0] & 0xE0) == 0xC0)
			return 2;
		if ((s[0] & 0xF0) == 0xE0)
			return 3;
		if ((s[0] & 0xF8) == 0xF0)
			return 4;
		return 0;
	case UTF16_BE:
	case UTF16_LE: {
		uint16_t w;
		if (n < 2)
			return 2;
		w = art == UTF16_BE ? (uint16_t)(s[0] << 8 | s[1]) : (uint16_t)(s[1] << 8 | s[0]);
		return (w >= 0xD800 && w <= 0xDBFF) ? 4 : 2;
	}
	default:
		return 1;
	}
}

size_t iconv(iconv_t cd, char **ein, size_t *einlen, char **aus, size_t *auslen)
{
	struct esp32os_iconv *k = (struct esp32os_iconv *)cd;
	if (cd == NULL || cd == (iconv_t)-1) {
		errno = EBADF;
		return (size_t)-1;
	}
	if (ein == NULL || *ein == NULL) {
		parserutils_charset_codec_reset(k->von);
		parserutils_charset_codec_reset(k->nach);
		return 0;
	}
	while (*einlen > 0) {
		const uint8_t *s = (const uint8_t *)*ein;
		size_t l = zeichen_laenge(k->art, s, *einlen);
		uint8_t ucs4[8];
		uint8_t *d = ucs4;
		size_t dl = sizeof ucs4, sl;
		const uint8_t *q;
		uint8_t *o;
		size_t ol;
		parserutils_error e;
		if (l == 0) {
			errno = EILSEQ;
			return (size_t)-1;
		}
		if (l > *einlen) {
			errno = EINVAL;
			return (size_t)-1;
		}
		sl = l;
		e = parserutils_charset_codec_decode(k->von, &s, &sl, &d, &dl);
		if (e != PARSERUTILS_OK || sl != 0 || d == ucs4) {
			parserutils_charset_codec_reset(k->von);
			errno = (e == PARSERUTILS_NEEDDATA) ? EINVAL : EILSEQ;
			return (size_t)-1;
		}
		q = ucs4;
		sl = (size_t)(d - ucs4);
		o = (uint8_t *)*aus;
		ol = *auslen;
		e = parserutils_charset_codec_encode(k->nach, &q, &sl, &o, &ol);
		if (e != PARSERUTILS_OK || sl != 0) {
			parserutils_charset_codec_reset(k->nach);
			errno = (e == PARSERUTILS_NOMEM) ? E2BIG : EILSEQ;
			return (size_t)-1;
		}
		*ein += l;
		*einlen -= l;
		*aus = (char *)o;
		*auslen = ol;
	}
	return 0;
}

int iconv_close(iconv_t cd)
{
	struct esp32os_iconv *k = (struct esp32os_iconv *)cd;
	if (cd == NULL || cd == (iconv_t)-1) {
		errno = EBADF;
		return -1;
	}
	parserutils_charset_codec_destroy(k->von);
	parserutils_charset_codec_destroy(k->nach);
	free(k);
	return 0;
}
