/* ESP32-OS: <sys/param.h> (BSD) für die MSVC-Laufzeit der PC-Tests. */
#ifndef ESP32OS_COMPAT_SYS_PARAM_H
#define ESP32OS_COMPAT_SYS_PARAM_H
#include <stdlib.h>
#ifndef MIN
#define MIN(a, b) (((a) < (b)) ? (a) : (b))
#endif
#ifndef MAX
#define MAX(a, b) (((a) > (b)) ? (a) : (b))
#endif
#ifndef PATH_MAX
#define PATH_MAX 260
#endif
#ifndef MAXPATHLEN
#define MAXPATHLEN PATH_MAX
#endif
#endif
