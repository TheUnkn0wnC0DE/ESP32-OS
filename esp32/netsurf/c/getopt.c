/* ESP32-OS: getopt_long für NetSurf (siehe compat/gemeinsam/getopt.h). */
#include <stddef.h>
#include <string.h>
#include "getopt.h"

char *optarg;
int optind = 1;

int getopt_long(int argc, char *const argv[], const char *kurz, const struct option *lang, int *index)
{
	const char *a, *p;
	optarg = NULL;
	if (optind >= argc)
		return -1;
	a = argv[optind];
	if (a[0] != '-' || a[1] == '\0')
		return -1;
	if (strcmp(a, "--") == 0) {
		optind++;
		return -1;
	}
	if (a[1] == '-') {
		const struct option *o;
		size_t l = strcspn(a + 2, "=");
		optind++;
		for (o = lang; o != NULL && o->name != NULL; o++) {
			if (strlen(o->name) != l || strncmp(o->name, a + 2, l) != 0)
				continue;
			if (index != NULL)
				*index = (int)(o - lang);
			if (o->has_arg != no_argument) {
				if (a[2 + l] == '=')
					optarg = (char *)a + 3 + l;
				else if (o->has_arg == required_argument) {
					if (optind >= argc)
						return '?';
					optarg = argv[optind++];
				}
			}
			if (o->flag != NULL) {
				*o->flag = o->val;
				return 0;
			}
			return o->val;
		}
		return '?';
	}
	p = strchr(kurz, a[1]);
	optind++;
	if (p == NULL || a[1] == ':')
		return '?';
	if (p[1] == ':') {
		if (a[2] != '\0')
			optarg = (char *)a + 2;
		else if (optind < argc)
			optarg = argv[optind++];
		else
			return '?';
	} else if (a[2] != '\0') {
		/* zusammengefasste Kurzoptionen ("-qv") gibt es bei NetSurf nicht */
		return '?';
	}
	return a[1];
}
