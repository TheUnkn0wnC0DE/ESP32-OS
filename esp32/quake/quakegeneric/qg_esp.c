//
// Konsole (nur ESP32): Die Quake-Spieldaten liegen in einer Flash-Partition.
// Quake öffnet die PAK für Demos mit fopen – dafür eine kleine virtuelle
// Datei unter /qpak/pak0.pak (nur lesen), die aus der Partition liest.
// Dazu der Stapel-Höchststand des Quake-Threads für den Autopiloten.
// Copyright(C) 2026 – GPL-2.0-or-later wie der übrige Quake-Code hier.
//

#ifdef ESP_PLATFORM

#include <errno.h>
#include <fcntl.h>
#include <string.h>
#include <sys/stat.h>

#include "esp_partition.h"
#include "esp_vfs.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define QG_GRIFFE 4

static const esp_partition_t *qg_part;
static size_t qg_groesse;
static off_t qg_pos[QG_GRIFFE];
static int qg_offen[QG_GRIFFE];

static int qv_open(const char *pfad, int flags, int modus)
{
    (void) pfad;
    (void) modus;
    if ((flags & O_ACCMODE) != O_RDONLY)
    {
        errno = EROFS;
        return -1;
    }
    for (int i = 0; i < QG_GRIFFE; i++)
    {
        if (!qg_offen[i])
        {
            qg_offen[i] = 1;
            qg_pos[i] = 0;
            return i;
        }
    }
    errno = ENFILE;
    return -1;
}

static ssize_t qv_read(int fd, void *ziel, size_t n)
{
    if (fd < 0 || fd >= QG_GRIFFE || !qg_offen[fd])
    {
        errno = EBADF;
        return -1;
    }
    if ((size_t) qg_pos[fd] >= qg_groesse)
    {
        return 0;
    }
    if (n > qg_groesse - (size_t) qg_pos[fd])
    {
        n = qg_groesse - (size_t) qg_pos[fd];
    }
    if (esp_partition_read(qg_part, qg_pos[fd], ziel, n) != ESP_OK)
    {
        errno = EIO;
        return -1;
    }
    qg_pos[fd] += n;
    return n;
}

static off_t qv_lseek(int fd, off_t ofs, int woher)
{
    off_t p;
    if (fd < 0 || fd >= QG_GRIFFE || !qg_offen[fd])
    {
        errno = EBADF;
        return -1;
    }
    switch (woher)
    {
        case SEEK_SET: p = ofs; break;
        case SEEK_CUR: p = qg_pos[fd] + ofs; break;
        case SEEK_END: p = (off_t) qg_groesse + ofs; break;
        default: errno = EINVAL; return -1;
    }
    if (p < 0)
    {
        errno = EINVAL;
        return -1;
    }
    qg_pos[fd] = p;
    return p;
}

static int qv_close(int fd)
{
    if (fd >= 0 && fd < QG_GRIFFE)
    {
        qg_offen[fd] = 0;
    }
    return 0;
}

static int qv_fstat(int fd, struct stat *st)
{
    (void) fd;
    memset(st, 0, sizeof(*st));
    st->st_size = qg_groesse;
    st->st_mode = S_IFREG | 0444;
    return 0;
}

// Partition (esp_partition_t*) und Größe der PAK; danach ist sie unter
// /qpak/pak0.pak lesbar.
int QG_PakVfs(const void *partition, unsigned int groesse)
{
    static esp_vfs_t v;
    qg_part = partition;
    qg_groesse = groesse;
    memset(&v, 0, sizeof(v));
    v.flags = ESP_VFS_FLAG_DEFAULT;
    v.open = qv_open;
    v.read = qv_read;
    v.lseek = qv_lseek;
    v.close = qv_close;
    v.fstat = qv_fstat;
    return esp_vfs_register("/qpak", &v, NULL) == ESP_OK;
}

// --- Stapel des Quake-Threads (Messlauf) -----------------------------------------

static TaskHandle_t qg_aufgabe;

// Merkt sich den aufrufenden Thread (den Quake-Thread) für QG_StapelFrei.
void QG_StapelMerken(void)
{
    qg_aufgabe = xTaskGetCurrentTaskHandle();
}

// Kleinster bisher freier Rest seines Stapels in Bytes (FreeRTOS füllt neue
// Stapel mit 0xA5 und zählt von unten); aus jedem Thread aufrufbar.
int QG_StapelFrei(void)
{
    return qg_aufgabe != NULL ? (int) uxTaskGetStackHighWaterMark(qg_aufgabe) : -1;
}

#endif
