/* ESP32-OS, B1: Prüfungen des NetSurf-Kerns (PC-Tests): URLs (nsurl mit
 * IDNA über utf8proc) und Zeichensätze (utils/utf8.c über den iconv-Ersatz
 * in c/iconv.c). */
#include <stdlib.h>
#include <string.h>
#include "utils/errors.h"
#include "utils/corestrings.h"
#include "utils/nsurl.h"
#include "utils/utf8.h"

static int bereit;

static int kern_bereit(void)
{
	if (!bereit) {
		if (corestrings_init() != NSERROR_OK)
			return 0;
		bereit = 1;
	}
	return 1;
}

static long kopieren(const char *s, size_t n, char *aus, size_t max)
{
	if (n + 1 > max)
		return -2;
	memcpy(aus, s, n);
	aus[n] = '\0';
	return (long)n;
}

/* `rel` gegen `basis` auflösen (rel = NULL: nur `basis` normalisieren). */
long ns_probe_url(const char *basis, const char *rel, char *aus, size_t max)
{
	nsurl *b = NULL, *j = NULL;
	long r;
	if (!kern_bereit() || nsurl_create(basis, &b) != NSERROR_OK)
		return -1;
	if (rel != NULL) {
		nserror e = nsurl_join(b, rel, &j);
		nsurl_unref(b);
		if (e != NSERROR_OK)
			return -1;
		b = j;
	}
	r = kopieren(nsurl_access(b), strlen(nsurl_access(b)), aus, max);
	nsurl_unref(b);
	return r;
}

/* UTF-8 → `enc` (richtung 0) bzw. `enc` → UTF-8 (1) bzw. UTF-8 → HTML in
 * `enc` mit &#x…;-Ersatz (2). Liefert die Länge oder < 0. */
long ns_probe_zeichensatz(int richtung, const char *ein, size_t n, const char *enc, char *aus, size_t max)
{
	char *r = NULL;
	size_t l = 0;
	nserror e;
	long k;
	switch (richtung) {
	case 0:
		e = utf8_to_enc(ein, enc, n, &r);
		l = r ? strlen(r) : 0;
		break;
	case 1:
		e = utf8_from_enc(ein, enc, n, &r, &l);
		break;
	default:
		e = utf8_to_html(ein, enc, n, &r);
		l = r ? strlen(r) : 0;
		break;
	}
	if (e != NSERROR_OK || r == NULL)
		return -1;
	k = kopieren(r, l, aus, max);
	free(r);
	return k;
}
