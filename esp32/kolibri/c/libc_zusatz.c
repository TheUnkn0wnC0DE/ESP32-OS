/* ESP32-OS: KolibriOS-libc-Aufsatz (siehe include/kolibri_libc.h).
 * Dateien über Funktion 70, Speicher über Funktion 68. Ströme, die nicht
 * von kos_fopen stammen, gehen an die Host-libc – ausser stdout/stderr:
 * Die gehen auf das Debug-Board (Funktion 63.1, Systemlog). Grund (A6): Der
 * Taskmanager kann ein Programm mitten im Lauf anhalten. Hielte es dabei die
 * Sperre von stdout der Host-libc (printf), stünde danach jede Ausgabe der
 * Firmware (ESP_LOG) still. Das Debug-Board läuft im Kern-Abschnitt; dort
 * wird nie angehalten. */
#define KOLIBRI_LIBC_INTERN
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ksys.h>
#include "kolibri_libc.h"

/* libc.obj: printf auf das Debug-Board (Funktion 63.1). */
void debug_printf(const char* fmt, ...)
{
    char buf[512];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    _ksys_debug_puts(buf);
}

/* stdout/stderr eines Programms (siehe oben). */
static int konsole(FILE* f)
{
    return f == stdout || f == stderr;
}

static void konsole_schreiben(const char* p, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        _ksys_debug_putc(p[i]);
    }
}

int kos_vprintf(const char* fmt, va_list ap)
{
    char buf[2048];
    int n = vsnprintf(buf, sizeof buf, fmt, ap);
    if (n > 0) {
        konsole_schreiben(buf, (size_t)(n < (int)sizeof buf ? n : (int)sizeof buf - 1));
    }
    return n;
}

int kos_printf(const char* fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int n = kos_vprintf(fmt, ap);
    va_end(ap);
    return n;
}

int kos_puts(const char* s)
{
    konsole_schreiben(s, strlen(s));
    _ksys_debug_putc('\n');
    return 0;
}

int kos_putchar(int c)
{
    _ksys_debug_putc((char)c);
    return (unsigned char)c;
}

/* ------------------------------------------------------------ Speicher */

void* kos_malloc(size_t n)
{
    ksys_regs_t r = { 68, 12, (uint32_t)(n ? n : 1) };
    kos_syscall(&r);
    return (void*)(uintptr_t)r.eax;
}

void* kos_calloc(size_t n, size_t g)
{
    if (g && n > (size_t)-1 / g) {
        return NULL; /* Überlauf */
    }
    size_t t = n * g;
    void* p = kos_malloc(t);
    if (p) {
        memset(p, 0, t);
    }
    return p;
}

void kos_free(void* p)
{
    if (!p) {
        return;
    }
    ksys_regs_t r = { 68, 13, (uint32_t)(uintptr_t)p };
    kos_syscall(&r);
    if (r.eax == 0) {
        free(p); /* stammt aus der Host-libc (z. B. strdup) */
    }
}

void* kos_realloc(void* p, size_t n)
{
    if (!p) {
        return kos_malloc(n);
    }
    ksys_regs_t r = { 68, 20, (uint32_t)n, (uint32_t)(uintptr_t)p };
    kos_syscall(&r);
    if (r.eax == 0 && r.ebx == 1) {
        return realloc(p, n); /* stammt aus der Host-libc */
    }
    return (void*)(uintptr_t)r.eax;
}

/* ------------------------------------------------------------ Dateien */

#define KOS_MAGIC 0x4B4F5346u

typedef struct {
    uint32_t magic;
    char pfad[1024];
    uint32_t pos;
    uint32_t groesse;
    int lesen, schreiben, fehler, ende;
    int zurueck; /* ungetc-Ersatz: -1 = leer */
} kos_datei;

#pragma pack(push, 1)
typedef struct {
    uint32_t sub, off, hi, n;
    uint32_t ptr;
    uint8_t null;
    uint32_t pfad;
} f70_t;
#pragma pack(pop)

static uint32_t f70(uint32_t sub, uint32_t off, uint32_t n, void* p, const char* pfad, uint32_t* ebx)
{
    f70_t b = { sub, off, 0, n, (uint32_t)(uintptr_t)p, 0, (uint32_t)(uintptr_t)pfad };
    ksys_regs_t r = { 70, (uint32_t)(uintptr_t)&b };
    kos_syscall(&r);
    if (ebx) {
        *ebx = r.ebx;
    }
    return r.eax;
}

static kos_datei* eigen(FILE* f)
{
    kos_datei* d = (kos_datei*)f;
    return (d && d->magic == KOS_MAGIC) ? d : NULL;
}

FILE* kos_fopen(const char* pfad, const char* modus)
{
    if (!pfad || !modus) {
        return NULL;
    }
    kos_datei* d = (kos_datei*)kos_calloc(1, sizeof(kos_datei));
    if (!d) {
        return NULL;
    }
    d->magic = KOS_MAGIC;
    d->zurueck = -1;
    strncpy(d->pfad, pfad, sizeof d->pfad - 1);
    int plus = strchr(modus, '+') != NULL;
    uint8_t info[40];
    int da = f70(5, 0, 0, info, d->pfad, NULL) == 0 && !(info[0] & 0x10);
    if (da) {
        memcpy(&d->groesse, info + 32, 4);
    }
    switch (modus[0]) {
    case 'r':
        if (!da) {
            kos_free(d);
            return NULL;
        }
        d->lesen = 1;
        d->schreiben = plus;
        break;
    case 'w':
        if (f70(2, 0, 0, d, d->pfad, NULL) != 0) {
            kos_free(d);
            return NULL;
        }
        d->groesse = 0;
        d->schreiben = 1;
        d->lesen = plus;
        break;
    case 'a':
        if (!da && f70(2, 0, 0, d, d->pfad, NULL) != 0) {
            kos_free(d);
            return NULL;
        }
        d->pos = d->groesse;
        d->schreiben = 1;
        d->lesen = plus;
        break;
    default:
        kos_free(d);
        return NULL;
    }
    return (FILE*)d;
}

int kos_fclose(FILE* f)
{
    kos_datei* d = eigen(f);
    if (!d) {
        return fclose(f);
    }
    d->magic = 0;
    kos_free(d);
    return 0;
}

size_t kos_fread(void* p, size_t g, size_t n, FILE* f)
{
    kos_datei* d = eigen(f);
    if (!d) {
        return fread(p, g, n, f);
    }
    if (!d->lesen || g == 0) {
        d->fehler = !d->lesen;
        return 0;
    }
    uint32_t gelesen = 0, t = (uint32_t)(g * n);
    uint8_t* q = (uint8_t*)p;
    if (d->zurueck >= 0 && t) {
        *q++ = (uint8_t)d->zurueck;
        d->zurueck = -1;
        d->pos++;
        t--;
        gelesen = 1;
    }
    uint32_t k = 0;
    uint32_t c = f70(0, d->pos, t, q, d->pfad, &k);
    d->pos += k;
    gelesen += k;
    if (c == 6 || k < t) {
        d->ende = 1;
    } else if (c != 0) {
        d->fehler = 1;
    }
    return gelesen / g;
}

size_t kos_fwrite(const void* p, size_t g, size_t n, FILE* f)
{
    kos_datei* d = eigen(f);
    if (!d && konsole(f)) {
        konsole_schreiben(p, g * n);
        return n;
    }
    if (!d) {
        return fwrite(p, g, n, f);
    }
    if (!d->schreiben || g == 0) {
        d->fehler = !d->schreiben;
        return 0;
    }
    uint32_t k = 0, t = (uint32_t)(g * n);
    if (f70(3, d->pos, t, (void*)p, d->pfad, &k) != 0) {
        d->fehler = 1;
    }
    d->pos += k;
    if (d->pos > d->groesse) {
        d->groesse = d->pos;
    }
    return k / g;
}

int kos_fseek(FILE* f, long o, int woher)
{
    kos_datei* d = eigen(f);
    if (!d) {
        return fseek(f, o, woher);
    }
    long basis = woher == SEEK_SET ? 0 : woher == SEEK_CUR ? (long)d->pos : (long)d->groesse;
    if (basis + o < 0) {
        return -1;
    }
    d->pos = (uint32_t)(basis + o);
    d->ende = 0;
    d->zurueck = -1;
    return 0;
}

long kos_ftell(FILE* f)
{
    kos_datei* d = eigen(f);
    return d ? (long)d->pos : ftell(f);
}

void kos_rewind(FILE* f)
{
    kos_datei* d = eigen(f);
    if (!d) {
        rewind(f);
        return;
    }
    kos_fseek(f, 0, SEEK_SET);
    d->fehler = 0;
}

int kos_feof(FILE* f)
{
    kos_datei* d = eigen(f);
    return d ? d->ende : feof(f);
}

int kos_ferror(FILE* f)
{
    kos_datei* d = eigen(f);
    return d ? d->fehler : ferror(f);
}

void kos_clearerr(FILE* f)
{
    kos_datei* d = eigen(f);
    if (!d) {
        clearerr(f);
        return;
    }
    d->fehler = d->ende = 0;
}

int kos_fgetc(FILE* f)
{
    if (!eigen(f)) {
        return fgetc(f);
    }
    unsigned char c;
    return kos_fread(&c, 1, 1, f) == 1 ? c : EOF;
}

char* kos_fgets(char* s, int n, FILE* f)
{
    if (!eigen(f)) {
        return fgets(s, n, f);
    }
    int i = 0;
    while (i < n - 1) {
        int c = kos_fgetc(f);
        if (c == EOF) {
            break;
        }
        s[i++] = (char)c;
        if (c == '\n') {
            break;
        }
    }
    if (i == 0) {
        return NULL;
    }
    s[i] = 0;
    return s;
}

int kos_fputc(int c, FILE* f)
{
    if (!eigen(f) && konsole(f)) {
        return kos_putchar(c);
    }
    if (!eigen(f)) {
        return fputc(c, f);
    }
    unsigned char b = (unsigned char)c;
    return kos_fwrite(&b, 1, 1, f) == 1 ? b : EOF;
}

int kos_fputs(const char* s, FILE* f)
{
    if (!eigen(f) && konsole(f)) {
        konsole_schreiben(s, strlen(s));
        return 0;
    }
    if (!eigen(f)) {
        return fputs(s, f);
    }
    size_t n = strlen(s);
    return kos_fwrite(s, 1, n, f) == n ? 0 : EOF;
}

int kos_vfprintf(FILE* f, const char* fmt, va_list ap)
{
    if (!eigen(f) && konsole(f)) {
        return kos_vprintf(fmt, ap);
    }
    if (!eigen(f)) {
        return vfprintf(f, fmt, ap);
    }
    char buf[2048];
    int n = vsnprintf(buf, sizeof buf, fmt, ap);
    if (n > 0) {
        kos_fwrite(buf, 1, (size_t)(n < (int)sizeof buf ? n : (int)sizeof buf - 1), f);
    }
    return n;
}

int kos_fprintf(FILE* f, const char* fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int n = kos_vfprintf(f, fmt, ap);
    va_end(ap);
    return n;
}

int kos_fflush(FILE* f)
{
    return (eigen(f) || konsole(f)) ? 0 : fflush(f);
}

int kos_remove(const char* pfad)
{
    return f70(8, 0, 0, NULL, pfad, NULL) == 0 ? 0 : -1;
}

/* C-rename nimmt den vollen neuen Pfad; Funktion 70.10 einen Namen relativ
 * zum Ordner. Gleicher Ordner → umbenennen, sonst kopieren und löschen. */
int kos_rename(const char* alt, const char* neu)
{
    const char* sa = strrchr(alt, '/');
    const char* sn = strrchr(neu, '/');
    size_t la = sa ? (size_t)(sa - alt) : 0, ln = sn ? (size_t)(sn - neu) : 0;
    if (!sn || (la == ln && strncmp(alt, neu, la) == 0)) {
        return f70(10, 0, 0, (void*)(sn ? sn + 1 : neu), alt, NULL) == 0 ? 0 : -1;
    }
    uint8_t info[40];
    if (f70(5, 0, 0, info, alt, NULL) != 0) {
        return -1;
    }
    uint32_t n;
    memcpy(&n, info + 32, 4);
    void* p = kos_malloc(n ? n : 1);
    uint32_t k = 0;
    int ok = p && f70(0, 0, n, p, alt, &k) <= 6 && k == n && f70(2, 0, n, p, neu, NULL) == 0;
    kos_free(p);
    return ok && kos_remove(alt) == 0 ? 0 : -1;
}

/* ------------------------------------------------------------ Programmende */

/* exit() aus einem Programm beendet nur dieses Programm (Funktion -1). */
void kos_exit(int status)
{
    (void)status;
    ksys_regs_t r = { (uint32_t)-1 };
    kos_syscall(&r);
    for (;;) {
    }
}

/* ------------------------------------------------------------ Sockets (W9)
 * Wie libc.obj source/sys/socket.c (KolibriOS, GPL-2.0): Funktion 75,
 * Fehlercode des Kerns in ebx → errno. */

static void kos_socket_fehler(uint32_t code)
{
    switch (code) {
    case 0: errno = 0; break;
    case 1: errno = ENOBUFS; break;
    case 2: errno = EINPROGRESS; break;
    case 4: errno = EOPNOTSUPP; break;
    case 6: errno = EWOULDBLOCK; break;
    case 9: errno = ENOTCONN; break;
    case 10: errno = EALREADY; break;
    case 11: errno = EINVAL; break;
    case 12: errno = EMSGSIZE; break;
    case 18: errno = ENOMEM; break;
    case 20: errno = EADDRINUSE; break;
    case 61: errno = ECONNREFUSED; break;
    case 52: errno = ECONNRESET; break;
    case 56: errno = EISCONN; break;
    case 60: errno = ETIMEDOUT; break;
    case 54: errno = ECONNABORTED; break;
    default: errno = EINVAL; break;
    }
}

static int kos_netz(uint32_t sub, uint32_t ecx, uint32_t edx, uint32_t esi, uint32_t edi)
{
    ksys_regs_t r = { 75, sub, ecx, edx, esi, edi };
    kos_syscall(&r);
    kos_socket_fehler(r.ebx);
    return (int)r.eax;
}

int kos_socket(int domain, int type, int protocol) { return kos_netz(0, domain, type, protocol, 0); }
int kos_close(int socket) { return kos_netz(1, socket, 0, 0, 0); }
int kos_bind(int socket, const struct sockaddr* a, int n) { return kos_netz(2, socket, (uint32_t)(uintptr_t)a, n, 0); }
int kos_listen(int socket, int rueckstau) { return kos_netz(3, socket, rueckstau, 0, 0); }
int kos_connect(int socket, const struct sockaddr* a, int n) { return kos_netz(4, socket, (uint32_t)(uintptr_t)a, n, 0); }
int kos_accept(int socket, const struct sockaddr* a, int n) { return kos_netz(5, socket, (uint32_t)(uintptr_t)a, n, 0); }
int kos_send(int socket, const void* d, size_t n, int flags) { return kos_netz(6, socket, (uint32_t)(uintptr_t)d, (uint32_t)n, flags); }
int kos_recv(int socket, void* p, size_t n, int flags) { return kos_netz(7, socket, (uint32_t)(uintptr_t)p, (uint32_t)n, flags); }
int kos_setsockopt(int socket, const void* opt) { return kos_netz(8, socket, (uint32_t)(uintptr_t)opt, 0, 0); }
int kos_getsockopt(int socket, void* opt) { return kos_netz(9, socket, (uint32_t)(uintptr_t)opt, 0, 0); }

int kos_socketpair(int* s1, int* s2)
{
    ksys_regs_t r = { 75, 10 };
    kos_syscall(&r);
    if ((int)r.eax == -1) {
        kos_socket_fehler(r.ebx);
        return -1;
    }
    *s1 = (int)r.eax;
    *s2 = (int)r.ebx;
    errno = 0;
    return 0;
}
