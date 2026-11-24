/* ESP32-OS, C1: KolibriOS-Shell (programs/system/shell, GPL-2.0,
 * unverändert in c/shell/). Wie der KolibriOS-Bau (Tupfile.lua):
 * Sprachwahl über LANG_ENG, AUTOBUILD statt erzeugter lang.h. set_cwd/getcwd
 * der Shell-Systemschicht heissen hier shell_* (c/shell_system.c), damit
 * sie nicht mit getcwd der C-Laufzeit kollidieren. */
#define AUTOBUILD 1
#define LANG_ENG 1
#define set_cwd shell_set_cwd
#define getcwd shell_getcwd
/* D1 (c/shell_zusatz.c): Umleitungen/Pipes und weitere Befehle */
int esp32os_umleitung(void);
int cmd_cat(char arg[]);
int cmd_grep(char arg[]);
int cmd_head(char arg[]);
int cmd_sort(char arg[]);
int cmd_tail(char arg[]);
int cmd_wc(char arg[]);
#include "shell/shell.c"
#include "shell_zusatz.c"
