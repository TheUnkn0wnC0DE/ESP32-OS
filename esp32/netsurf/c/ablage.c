/* ESP32-OS (B8): Zwischenablage des Browsers = Zwischenablage des Systems
 * (Funktion 54 über netsurf/src/lib.rs). Kopierter Text aus Webseiten ist
 * damit auch in anderen Programmen da, und Einfügen in Formularfelder holt
 * den letzten Text des Systems. Ersetzt framebuffer/clipboard.c (die
 * gui.c-Kopie trägt diese Tabelle ein). */
#include <stdlib.h>
#include <string.h>

#include "netsurf/clipboard.h"

/* netsurf/src/lib.rs */
void ns_ablage_setzen(const char *text, size_t laenge);
size_t ns_ablage_laenge(void);
size_t ns_ablage_kopieren(char *ziel, size_t max);

static void ablage_holen(char **buffer, size_t *length)
{
	size_t n = ns_ablage_laenge();
	*buffer = NULL;
	*length = 0;
	if (n == 0)
		return;
	*buffer = malloc(n);
	if (*buffer == NULL)
		return;
	*length = ns_ablage_kopieren(*buffer, n);
	if (*length == 0) {
		free(*buffer);
		*buffer = NULL;
	}
}

static void ablage_setzen(const char *buffer, size_t length, nsclipboard_styles styles[], int n_styles)
{
	(void)styles;
	(void)n_styles;
	if (buffer != NULL)
		ns_ablage_setzen(buffer, length);
}

struct gui_clipboard_table ns_ablage_tabelle = {
	.get = ablage_holen,
	.set = ablage_setzen,
};
