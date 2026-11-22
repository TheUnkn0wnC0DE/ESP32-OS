/* ESP32-OS: wird jeder NetSurf-Datei der PC-Tests vorangestellt (/FI).
 * Was glibc schon in Standard-Headern deklariert und manche Upstream-
 * Dateien darum ohne eigenes #include nutzen: strcasecmp & Co. (<string.h>),
 * PATH_MAX (<limits.h>), ssize_t. */
#ifndef ESP32OS_COMPAT_H
#define ESP32OS_COMPAT_H
#include <limits.h>
#include <stdint.h>
#include "strings.h"
#ifndef PATH_MAX
#define PATH_MAX 260
#endif
#ifndef _SSIZE_T_DEFINED
#define _SSIZE_T_DEFINED
typedef intptr_t ssize_t;
#endif
/* S_ISDIR/S_ISREG fehlen in der MSVC-<sys/stat.h> */
#include <sys/types.h>
#include <sys/stat.h>
#ifndef S_ISDIR
#define S_ISDIR(m) (((m) & _S_IFMT) == _S_IFDIR)
#endif
#ifndef S_ISREG
#define S_ISREG(m) (((m) & _S_IFMT) == _S_IFREG)
#endif
#endif
