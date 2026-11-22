/* ESP32-OS: <iconv.h> für NetSurf (utils/utf8.c). Umsetzung in
 * c/iconv.c auf den Zeichensatz-Codecs von libparserutils – dieselben
 * Zeichensätze wie beim HTML-Parsing (UTF-8, UTF-16, US-ASCII,
 * ISO-8859-x, Windows-125x …). Weder die MSVC-Laufzeit noch newlib (ESP-IDF)
 * bringen iconv mit. */
#ifndef ESP32OS_ICONV_H
#define ESP32OS_ICONV_H
#include <stddef.h>

typedef void *iconv_t;

/* Ziel "…//TRANSLIT": nicht darstellbare Zeichen umschreiben bzw. "?";
 * "…//IGNORE": ersetzen; sonst wie iconv: Fehler EILSEQ. */
iconv_t iconv_open(const char *nach, const char *von);
size_t iconv(iconv_t cd, char **ein, size_t *einlen, char **aus, size_t *auslen);
int iconv_close(iconv_t cd);

#endif
