/* ESP32-OS: <unistd.h> (POSIX) für die MSVC-Laufzeit der PC-Tests. */
#ifndef ESP32OS_COMPAT_UNISTD_H
#define ESP32OS_COMPAT_UNISTD_H
#include <io.h>
#include <process.h>
#include <direct.h>
#include <stdio.h>
#include <stdint.h>
#include <sys/types.h>
#ifndef _SSIZE_T_DEFINED
#define _SSIZE_T_DEFINED
typedef intptr_t ssize_t;
#endif
#ifndef R_OK
#define R_OK 4
#define W_OK 2
#define X_OK 0
#define F_OK 0
#endif
#endif
