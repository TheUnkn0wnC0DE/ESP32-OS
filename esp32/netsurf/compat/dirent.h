/* ESP32-OS: <dirent.h> (POSIX) für die MSVC-Laufzeit der PC-Tests, über
 * _findfirst/_findnext. Am Board bringt newlib/ESP-IDF dirent mit. */
#ifndef ESP32OS_COMPAT_DIRENT_H
#define ESP32OS_COMPAT_DIRENT_H
#include <io.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

struct dirent {
	char d_name[260];
};

typedef struct {
	intptr_t griff;
	struct _finddata_t daten;
	int erstes;
	struct dirent eintrag;
} DIR;

static __inline DIR *opendir(const char *pfad)
{
	char muster[300];
	size_t l = strlen(pfad);
	DIR *d;
	if (l + 3 >= sizeof muster)
		return NULL;
	memcpy(muster, pfad, l);
	if (l > 0 && pfad[l - 1] != '/' && pfad[l - 1] != 0x5C /* Backslash */)
		muster[l++] = '/';
	muster[l++] = '*';
	muster[l] = '\0';
	d = (DIR *)calloc(1, sizeof *d);
	if (d == NULL)
		return NULL;
	d->griff = _findfirst(muster, &d->daten);
	if (d->griff == -1) {
		free(d);
		return NULL;
	}
	d->erstes = 1;
	return d;
}

static __inline struct dirent *readdir(DIR *d)
{
	if (!d->erstes && _findnext(d->griff, &d->daten) != 0)
		return NULL;
	d->erstes = 0;
	strncpy(d->eintrag.d_name, d->daten.name, sizeof d->eintrag.d_name - 1);
	d->eintrag.d_name[sizeof d->eintrag.d_name - 1] = '\0';
	return &d->eintrag;
}

static __inline int closedir(DIR *d)
{
	_findclose(d->griff);
	free(d);
	return 0;
}
#endif
