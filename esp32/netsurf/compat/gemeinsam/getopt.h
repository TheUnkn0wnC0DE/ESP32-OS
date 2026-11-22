/* ESP32-OS: <getopt.h> (GNU getopt_long) für NetSurf: das Framebuffer-
 * Frontend (gui.c) und das Werkzeug tools/convert_font.c. Weder die
 * MSVC-Laufzeit noch newlib haben getopt_long. Umsetzung: c/getopt.c.
 *
 * `optind` ist global: Programme laufen im gemeinsamen Adressraum; es liegt
 * im Abschnitt der statischen Daten und wird beim Neustart zurückgespielt
 * (c/programm_daten.c). */
#ifndef ESP32OS_GETOPT_H
#define ESP32OS_GETOPT_H

/* Eigene Namen: am Board hat newlib getopt/optind selbst (ausserhalb des
 * Abschnitts der statischen Daten – ein zweiter Start fände optind am
 * Ende der ersten Argumente). Unsere liegen im Abschnitt und werden beim
 * Neustart zurückgespielt. */
#define optarg ns_optarg
#define optind ns_optind
#define getopt_long ns_getopt_long

#define no_argument 0
#define required_argument 1
#define optional_argument 2

struct option {
	const char *name;
	int has_arg;
	int *flag;
	int val;
};

extern char *optarg;
extern int optind;

/* Kurzoptionen wie POSIX; lange Optionen ("--name", "--name=wert") über die
 * Tabelle, die mit einem Eintrag name == NULL endet. */
int getopt_long(int argc, char *const argv[], const char *kurz, const struct option *lang, int *index);

#endif
