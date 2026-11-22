/* ESP32-OS: <strings.h> (POSIX) für die MSVC-Laufzeit der PC-Tests. Am
 * Board (newlib) wird dieser Ordner nicht benutzt. */
#ifndef ESP32OS_COMPAT_STRINGS_H
#define ESP32OS_COMPAT_STRINGS_H
#include <string.h>
#define strcasecmp _stricmp
#define strncasecmp _strnicmp
#endif
