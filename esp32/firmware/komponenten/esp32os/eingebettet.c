/* ESP32-OS: eingebettete Dateien als schreibgeschütztes Dateisystem (VFS).
 *
 * Für den Browser (B1): NetSurf liest seine Ressourcen (CSS, Messages,
 * Willkommensseiten, Symbole) über open/read/stat. Statt sie beim ersten
 * Start in den Flash zu schreiben (≈200 KB – am Board brachte dieses lange
 * Schreiben zusammen mit Programmen, deren Stapel im PSRAM liegen, das
 * System zum Absturz), liegen sie im Firmware-Abbild und werden von hier
 * aus direkt gelesen. Sie passen damit immer zur Firmware.
 *
 * Die Dateitabelle kommt vom Aufrufer (statisch im Abbild, z. B. vom
 * netsurf-Crate erzeugt) und muss für die Laufzeit gültig bleiben.
 */
#include <errno.h>
#include <fcntl.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "esp_vfs.h"
#include "freertos/FreeRTOS.h"

struct eingebettet {
	const char *name; /* relativ zur Basis, z. B. "de/Messages" */
	const uint8_t *daten;
	size_t laenge;
};

#define OFFEN_MAX 16

static const struct eingebettet *tabelle;
static size_t anzahl;
static struct {
	const struct eingebettet *d;
	size_t pos;
} offen[OFFEN_MAX];
static portMUX_TYPE sperre = portMUX_INITIALIZER_UNLOCKED;

static const char *ohne_strich(const char *p)
{
	while (*p == '/')
		p++;
	return p;
}

static const struct eingebettet *finden(const char *pfad)
{
	size_t i;
	pfad = ohne_strich(pfad);
	for (i = 0; i < anzahl; i++)
		if (strcmp(tabelle[i].name, pfad) == 0)
			return &tabelle[i];
	return NULL;
}

/* Ordner: die Basis selbst oder ein Präfix eines Dateinamens bis '/' */
static bool ist_ordner(const char *pfad)
{
	size_t i, n;
	pfad = ohne_strich(pfad);
	n = strlen(pfad);
	while (n > 0 && pfad[n - 1] == '/')
		n--;
	if (n == 0)
		return true;
	for (i = 0; i < anzahl; i++)
		if (strncmp(tabelle[i].name, pfad, n) == 0 && tabelle[i].name[n] == '/')
			return true;
	return false;
}

static void datei_stat(const struct eingebettet *d, struct stat *st)
{
	memset(st, 0, sizeof *st);
	if (d != NULL) {
		st->st_mode = S_IFREG | 0444;
		st->st_size = (off_t)d->laenge;
	} else {
		st->st_mode = S_IFDIR | 0555;
	}
}

static int e_open(const char *pfad, int flags, int mode)
{
	const struct eingebettet *d;
	int fd;
	(void)mode;
	if ((flags & O_ACCMODE) != O_RDONLY) {
		errno = EROFS;
		return -1;
	}
	d = finden(pfad);
	if (d == NULL) {
		errno = ist_ordner(pfad) ? EISDIR : ENOENT;
		return -1;
	}
	taskENTER_CRITICAL(&sperre);
	for (fd = 0; fd < OFFEN_MAX; fd++) {
		if (offen[fd].d == NULL) {
			offen[fd].d = d;
			offen[fd].pos = 0;
			break;
		}
	}
	taskEXIT_CRITICAL(&sperre);
	if (fd == OFFEN_MAX) {
		errno = ENFILE;
		return -1;
	}
	return fd;
}

static bool gueltig(int fd)
{
	return fd >= 0 && fd < OFFEN_MAX && offen[fd].d != NULL;
}

static ssize_t e_read(int fd, void *ziel, size_t n)
{
	size_t rest;
	if (!gueltig(fd)) {
		errno = EBADF;
		return -1;
	}
	rest = offen[fd].d->laenge - offen[fd].pos;
	if (n > rest)
		n = rest;
	memcpy(ziel, offen[fd].d->daten + offen[fd].pos, n);
	offen[fd].pos += n;
	return (ssize_t)n;
}

static ssize_t e_write(int fd, const void *daten, size_t n)
{
	(void)fd;
	(void)daten;
	(void)n;
	errno = EROFS;
	return -1;
}

static off_t e_lseek(int fd, off_t weit, int woher)
{
	off_t neu;
	if (!gueltig(fd)) {
		errno = EBADF;
		return -1;
	}
	switch (woher) {
	case SEEK_SET: neu = weit; break;
	case SEEK_CUR: neu = (off_t)offen[fd].pos + weit; break;
	case SEEK_END: neu = (off_t)offen[fd].d->laenge + weit; break;
	default:
		errno = EINVAL;
		return -1;
	}
	if (neu < 0) {
		errno = EINVAL;
		return -1;
	}
	offen[fd].pos = (size_t)neu > offen[fd].d->laenge ? offen[fd].d->laenge : (size_t)neu;
	return (off_t)offen[fd].pos;
}

static int e_close(int fd)
{
	if (!gueltig(fd)) {
		errno = EBADF;
		return -1;
	}
	taskENTER_CRITICAL(&sperre);
	offen[fd].d = NULL;
	taskEXIT_CRITICAL(&sperre);
	return 0;
}

static int e_fstat(int fd, struct stat *st)
{
	if (!gueltig(fd)) {
		errno = EBADF;
		return -1;
	}
	datei_stat(offen[fd].d, st);
	return 0;
}

static int e_stat(const char *pfad, struct stat *st)
{
	const struct eingebettet *d = finden(pfad);
	if (d == NULL && !ist_ordner(pfad)) {
		errno = ENOENT;
		return -1;
	}
	datei_stat(d, st);
	return 0;
}

static int e_access(const char *pfad, int art)
{
	if (finden(pfad) == NULL && !ist_ordner(pfad)) {
		errno = ENOENT;
		return -1;
	}
	if (art & W_OK) {
		errno = EROFS;
		return -1;
	}
	return 0;
}

/* Meldet die Dateien unter `basis` an (einmal je Basis; die Tabelle bleibt
 * gültig). 0 = angemeldet, sonst Fehlercode von esp_vfs_register. */
int esp32os_eingebettet_anmelden(const char *basis, const struct eingebettet *dateien, size_t n)
{
	esp_vfs_t vfs = {
		.flags = ESP_VFS_FLAG_DEFAULT,
		.write = e_write,
		.lseek = e_lseek,
		.read = e_read,
		.open = e_open,
		.close = e_close,
		.fstat = e_fstat,
		.stat = e_stat,
		.access = e_access,
	};
	tabelle = dateien;
	anzahl = n;
	return (int)esp_vfs_register(basis, &vfs, NULL);
}
