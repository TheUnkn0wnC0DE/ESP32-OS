/* ESP32-OS, B1: Prüfungen der NetSurf-Parserbibliotheken (PC-Tests):
 * HTML über libhubbub in ein libdom-Dokument, CSS über libcss. */
#include <stdint.h>
#include <string.h>
#include <dom/dom.h>
#include <dom/bindings/hubbub/parser.h>
#include <libcss/libcss.h>

/* HTML parsen; liefert die Anzahl der Elemente mit Namen `tag` ("*" =
 * alle), -1 bei Fehler. */
int ns_probe_html(const uint8_t *daten, size_t laenge, const char *tag)
{
	dom_hubbub_parser_params p;
	memset(&p, 0, sizeof p);
	p.enc = NULL;
	p.fix_enc = true;
	p.enable_script = false;
	dom_hubbub_parser *parser = NULL;
	dom_document *doc = NULL;
	if (dom_hubbub_parser_create(&p, &parser, &doc) != DOM_HUBBUB_OK)
		return -1;
	dom_hubbub_error e = dom_hubbub_parser_parse_chunk(parser, daten, laenge);
	if (e == DOM_HUBBUB_OK)
		e = dom_hubbub_parser_completed(parser);
	dom_hubbub_parser_destroy(parser);
	if (e != DOM_HUBBUB_OK) {
		dom_node_unref(doc);
		return -1;
	}
	dom_string *name = NULL;
	if (dom_string_create((const uint8_t *)tag, strlen(tag), &name) != DOM_NO_ERR) {
		dom_node_unref(doc);
		return -1;
	}
	dom_nodelist *liste = NULL;
	int n = -1;
	if (dom_document_get_elements_by_tag_name(doc, name, &liste) == DOM_NO_ERR && liste) {
		uint32_t l = 0;
		if (dom_nodelist_get_length(liste, &l) == DOM_NO_ERR)
			n = (int)l;
		dom_nodelist_unref(liste);
	}
	dom_string_unref(name);
	dom_node_unref(doc);
	return n;
}

static css_error aufloesen(void *pw, const char *basis, lwc_string *rel, lwc_string **abs)
{
	(void)pw;
	(void)basis;
	*abs = lwc_string_ref(rel);
	return CSS_OK;
}

/* CSS parsen; liefert die Grösse des Stylesheets in Byte (> 0) oder einen
 * negativen libcss-Fehler. */
long ns_probe_css(const uint8_t *daten, size_t laenge)
{
	css_stylesheet_params p;
	memset(&p, 0, sizeof p);
	p.params_version = CSS_STYLESHEET_PARAMS_VERSION_1;
	p.level = CSS_LEVEL_DEFAULT;
	p.charset = "UTF-8";
	p.url = "file:///probe.css";
	p.title = "probe";
	p.resolve = aufloesen;
	css_stylesheet *s = NULL;
	css_error e = css_stylesheet_create(&p, &s);
	if (e != CSS_OK)
		return -(long)e - 1;
	e = css_stylesheet_append_data(s, daten, laenge);
	if (e != CSS_OK && e != CSS_NEEDDATA) {
		css_stylesheet_destroy(s);
		return -(long)e - 1;
	}
	e = css_stylesheet_data_done(s);
	if (e != CSS_OK) {
		css_stylesheet_destroy(s);
		return -(long)e - 1;
	}
	size_t g = 0;
	css_stylesheet_size(s, &g);
	css_stylesheet_destroy(s);
	return (long)g;
}
