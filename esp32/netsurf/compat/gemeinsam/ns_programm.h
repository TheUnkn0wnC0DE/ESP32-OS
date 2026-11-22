/* ESP32-OS, B1: NetSurf als ESP32-OS-Programm (32 Bit: i686-PC-Tests und
 * Board). Wird jeder NetSurf-Quelle vorangestellt (/FI bzw. -include).
 *
 * - Speicher: malloc & Co. (und strdup) aus dem Heap des Programms
 *   (Kern-Funktion 68 über kolibri/c/libc_zusatz.c). Beenden und
 *   Zwangsbeenden geben so allen Speicher frei; was die C-Laufzeit selbst
 *   anlegt (realpath, scandir), gibt kos_free an die Laufzeit zurück.
 * - exit() beendet nur das Programm (die() von NetSurf), nicht das System.
 * - Dateien bleiben bei der C-Laufzeit (open/stat/fopen): NetSurf nutzt
 *   POSIX-Dateifunktionen gemischt, ein halber Umbau auf die
 *   Kern-Dateifunktionen wäre falsch. Die Socket-Makros aus
 *   kolibri_libc.h fehlen hier absichtlich (close(fd) einer Datei wäre
 *   sonst ein Socket-Aufruf); Netz kommt über den eigenen Abrufer (B2).
 * - Abschnitt: ns_abschnitt.h (statische Daten für den Neustart).
 */
#ifndef NS_PROGRAMM_H
#define NS_PROGRAMM_H
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include "ns_abschnitt.h"

void *kos_malloc(size_t n);
void *kos_calloc(size_t n, size_t g);
void *kos_realloc(void *p, size_t n);
void kos_free(void *p);
void kos_exit(int status);

/* strdup der C-Laufzeit legte im Speicher der Laufzeit an; strndup bringt
 * NetSurf selbst mit (utils/utils.c, über malloc → Heap des Programms) */
static inline char *ns_strdup(const char *s)
{
	size_t l = strlen(s);
	char *d = (char *)kos_malloc(l + 1);
	if (d != NULL)
		memcpy(d, s, l + 1);
	return d;
}

#define malloc kos_malloc
#define calloc kos_calloc
#define realloc kos_realloc
#define free kos_free
#define exit kos_exit
#define strdup ns_strdup

#endif
