/* ESP32-OS, B1: <sys/utsname.h> fürs Board (newlib hat keins). NetSurf
 * baut daraus die Kennung des Browsers (utils/useragent.c):
 * „NetSurf/3.11 (ESP32-OS; riscv32)“. */
#ifndef ESP32OS_SYS_UTSNAME_H
#define ESP32OS_SYS_UTSNAME_H
#include <string.h>

struct utsname {
	char sysname[65];
	char nodename[65];
	char release[65];
	char version[65];
	char machine[65];
};

static inline int uname(struct utsname *u)
{
	strcpy(u->sysname, "ESP32-OS");
	strcpy(u->nodename, "esp32-p4");
	strcpy(u->release, "1");
	strcpy(u->version, "1");
	strcpy(u->machine, "riscv32");
	return 0;
}

#endif
