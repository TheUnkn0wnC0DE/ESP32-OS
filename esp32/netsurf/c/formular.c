/* ESP32-OS, B7: Formular-Ereignisse für Skripte.
 *
 * NetSurf 3.11 schickt Formulare ab und ändert Textfelder, ohne Skripten
 * Bescheid zu geben, und folgt Links auch dann, wenn ein Skript den Klick
 * abbricht. Die Kopien von box_textarea.c und interaction.c (build.rs)
 * rufen hier:
 * - ns_feld_eingabe_melden: nach einem Tastendruck, der den Text eines
 *   Feldes geändert hat, „input“ ans Feld (sprudelt, nicht abbrechbar);
 * - ns_formular_absenden_erlaubt: vor dem Abschicken (Eingabetaste,
 *   Absendeknopf) „submit“ ans Formular – ruft ein Skript
 *   preventDefault(), wird nicht abgeschickt;
 * - ns_bedienung_gewechselt: Kästchen, Optionsfeld oder Auswahlliste per
 *   Tippen geändert – „input“ und „change“ (sprudeln, nicht abbrechbar);
 * - ns_feld_vorgabe_sichern: libdom nimmt beim ersten Setzen von value den
 *   Wert als Vorgabe (defaultValue), wenn das Feld kein value-Attribut
 *   hatte – vorher die leere Vorgabe festhalten (Tippen, Skript);
 * - ns_ereignis_frei: ein Ereignis zustellen; false nur, wenn es zugestellt
 *   und abgebrochen wurde (Fehler zählen als frei, wie ohne Skript).
 */
#include <stdbool.h>
#include <string.h>

#include <dom/dom.h>

#include "utils/corestrings.h"
#include "html/form_internal.h"

bool ns_ereignis_frei(dom_string *typ, dom_node *ziel, bool sprudelt, bool abbrechbar);
bool ns_formular_absenden_erlaubt(struct form *form);
void ns_feld_eingabe_melden(struct form_control *g, const char *vorher);
void ns_feld_vorgabe_sichern(dom_node *n);
void ns_bedienung_gewechselt(dom_node *n);

bool ns_ereignis_frei(dom_string *typ, dom_node *ziel, bool sprudelt, bool abbrechbar)
{
	dom_event *e = NULL;
	bool erfolg = true;
	if (ziel == NULL || dom_event_create(&e) != DOM_NO_ERR)
		return true;
	if (dom_event_init(e, typ, sprudelt, abbrechbar) == DOM_NO_ERR) {
		dom_node_ref(ziel);
		if (dom_event_target_dispatch_event(ziel, e, &erfolg) != DOM_NO_ERR)
			erfolg = true;
		dom_node_unref(ziel);
	}
	dom_event_unref(e);
	return erfolg;
}

bool ns_formular_absenden_erlaubt(struct form *form)
{
	return form == NULL || ns_ereignis_frei(corestring_dom_submit, form->node, true, true);
}

void ns_feld_eingabe_melden(struct form_control *g, const char *vorher)
{
	const char *jetzt = g->value != NULL ? g->value : "";
	if (g->node == NULL || strcmp(jetzt, vorher != NULL ? vorher : "") == 0)
		return;
	ns_ereignis_frei(corestring_dom_input, g->node, true, false);
}

void ns_feld_vorgabe_sichern(dom_node *n)
{
	dom_string *s = NULL;
	if (n == NULL)
		return;
	if (dom_html_input_element_get_default_value((dom_html_input_element *)n, &s) == DOM_NO_ERR && s != NULL) {
		dom_string_unref(s);
		return;
	}
	dom_html_input_element_set_default_value((dom_html_input_element *)n, NULL);
}

void ns_bedienung_gewechselt(dom_node *n)
{
	ns_ereignis_frei(corestring_dom_input, n, true, false);
	ns_ereignis_frei(corestring_dom_change, n, true, false);
}
