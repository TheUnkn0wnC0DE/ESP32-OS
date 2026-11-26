/* ESP32-OS, Browser (Q3): CSS-Text vorbereiten, bevor libcss ihn liest.
 *
 * libcss 0.9 (NetSurf 3.11) kennt keine eigenen Eigenschaften (`--name`),
 * kein `var()`, kein `calc()`, keine Leerzeichen-Schreibweise von `rgb()` und
 * keine logischen Eigenschaften. Am Board waren das die häufigsten
 * verworfenen Deklarationen der 30 Testseiten (c/css_luecken.c: var() 8677,
 * calc() 1055, rgb() 468, margin-block-start/-inline-start, inline-size …).
 * Diese Datei schreibt den Text so um, dass libcss ihn versteht:
 *
 * 1. Eigene Eigenschaften sammeln (je Browserstart, alle Stylesheets und
 *    style-Attribute): Vorrang 3 hat eine Definition in `:root`/`html`/
 *    `body`/`*` auf oberster Ebene, 2 dieselben in `@media` (bei 3 und 2
 *    gilt die spätere, wie die Kaskade), 1 andere Regeln (die erste gilt:
 *    Grundwerte stehen vor Abwandlungen wie :hover oder .aktiv), 0 alles mit
 *    „dark“ im Selektor oder in der @media-Bedingung (dunkles Design; die
 *    erste gilt, jede andere Definition schlägt sie – Board 26.11.:
 *    DuckDuckGo wurde sonst schwarz). Nur eine Näherung – Werte je Element
 *    gibt es nicht.
 * 2. `var(--name[, ersatz])` durch den Wert ersetzen (verschachtelt bis
 *    Tiefe 8), sonst durch den Ersatz; unbekannt: unverändert (libcss
 *    verwirft die Deklaration wie vorher).
 * 3. `calc(…)` ausrechnen, wenn alle Glieder dieselbe Einheit haben bzw.
 *    Faktoren ohne Einheit sind (`calc(2 * 8px)` → `16px`); gemischte
 *    Einheiten (`100% - 10px`) bleiben.
 * 4. `rgb(a b c / d)` → `rgba(a,b,c,d)`, `rgb(a b c)` → `rgb(a,b,c)`
 *    (ebenso hsl).
 * 5. Logische Eigenschaften (links nach rechts) auf die physischen:
 *    margin-inline-start → margin-left, inline-size → width …
 *
 * Stylesheets kommen in Teilstücken: verarbeitet wird bis zur letzten
 * vollständigen Regel (`}` ausserhalb von Kommentar und Zeichenkette), der
 * Rest wartet auf das nächste Stück (Puffer je Stylesheet, `schluessel`).
 */
#include <ctype.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

/* ------------------------------------------------------ Textpuffer */

struct puffer {
	char *d;
	size_t n, platz;
	bool fehler;
};

static void p_platz(struct puffer *p, size_t mehr)
{
	size_t neu;
	char *d;
	if (p->fehler || p->n + mehr + 1 <= p->platz)
		return;
	neu = p->platz ? p->platz : 256;
	while (neu < p->n + mehr + 1)
		neu *= 2;
	d = realloc(p->d, neu);
	if (d == NULL) {
		p->fehler = true;
		return;
	}
	p->d = d;
	p->platz = neu;
}

static void p_text(struct puffer *p, const char *s, size_t n)
{
	p_platz(p, n);
	if (p->fehler)
		return;
	memcpy(p->d + p->n, s, n);
	p->n += n;
	p->d[p->n] = 0;
}

static void p_zeichen(struct puffer *p, char c)
{
	p_text(p, &c, 1);
}

/* ------------------------------------------------- Eigenschaften */

#define VAR_PLAETZE 4096 /* Hash-Tabelle, höchstens 3/4 belegt */
#define VAR_MAX_BYTES (384u * 1024u)
#define VAR_NAME_MAX 96
#define VAR_WERT_MAX 512

struct variable {
	char *name; /* "--x" */
	char *wert;
	uint8_t vorrang;
};

static struct variable *vars; /* VAR_PLAETZE Einträge, bei Bedarf */
static size_t vars_anzahl, vars_bytes;
static unsigned vars_ersetzt, vars_unbekannt;

static uint32_t hash(const char *s, size_t n)
{
	uint32_t h = 2166136261u;
	size_t i;
	for (i = 0; i < n; i++)
		h = (h ^ (uint8_t)s[i]) * 16777619u;
	return h;
}

static struct variable *var_suchen(const char *name, size_t n, bool anlegen)
{
	uint32_t i;
	if (vars == NULL) {
		if (!anlegen)
			return NULL;
		vars = calloc(VAR_PLAETZE, sizeof *vars);
		if (vars == NULL)
			return NULL;
	}
	i = hash(name, n) & (VAR_PLAETZE - 1);
	for (;;) {
		struct variable *v = &vars[i];
		if (v->name == NULL)
			return anlegen && vars_anzahl < VAR_PLAETZE * 3 / 4 ? v : NULL;
		if (strlen(v->name) == n && memcmp(v->name, name, n) == 0)
			return v;
		i = (i + 1) & (VAR_PLAETZE - 1);
	}
}

static char *kopie(const char *s, size_t n)
{
	char *d = malloc(n + 1);
	if (d != NULL) {
		memcpy(d, s, n);
		d[n] = 0;
	}
	return d;
}

static void var_setzen(const char *name, size_t nn, const char *wert, size_t wn, uint8_t vorrang)
{
	struct variable *v;
	char *w;
	while (wn > 0 && isspace((unsigned char)wert[wn - 1]))
		wn--;
	while (wn > 0 && isspace((unsigned char)*wert)) {
		wert++;
		wn--;
	}
	/* „!important“ gehört nicht zum Wert */
	if (wn >= 10 && strncmp(wert + wn - 10, "!important", 10) == 0) {
		wn -= 10;
		while (wn > 0 && isspace((unsigned char)wert[wn - 1]))
			wn--;
	}
	if (nn < 3 || nn > VAR_NAME_MAX || wn > VAR_WERT_MAX)
		return;
	v = var_suchen(name, nn, true);
	if (v == NULL)
		return;
	if (v->name != NULL) {
		if (vorrang < v->vorrang)
			return;
		/* andere Regeln und dunkles Design: die erste Definition gilt */
		if (vorrang == v->vorrang && vorrang <= 1)
			return;
		if (strlen(v->wert) == wn && memcmp(v->wert, wert, wn) == 0) {
			v->vorrang = vorrang;
			return;
		}
		if (vars_bytes + wn + 1 > VAR_MAX_BYTES)
			return;
		w = kopie(wert, wn);
		if (w == NULL)
			return;
		vars_bytes = vars_bytes - strlen(v->wert) + wn;
		free(v->wert);
		v->wert = w;
		v->vorrang = vorrang;
		return;
	}
	if (vars_bytes + nn + wn + 2 > VAR_MAX_BYTES)
		return;
	v->name = kopie(name, nn);
	v->wert = kopie(wert, wn);
	if (v->name == NULL || v->wert == NULL) {
		free(v->name);
		free(v->wert);
		v->name = v->wert = NULL;
		return;
	}
	v->vorrang = vorrang;
	vars_anzahl++;
	vars_bytes += nn + wn + 2;
}

/* Kommentar oder Zeichenkette ab `i` überspringen: neue Stelle, sonst `i` */
static size_t ueberspringen(const char *s, size_t n, size_t i)
{
	if (i + 1 < n && s[i] == '/' && s[i + 1] == '*') {
		size_t j;
		for (j = i + 2; j + 1 < n; j++)
			if (s[j] == '*' && s[j + 1] == '/')
				return j + 2;
		return n;
	}
	if (s[i] == '"' || s[i] == '\'') {
		char q = s[i];
		size_t j;
		for (j = i + 1; j < n; j++) {
			if (s[j] == '\\') {
				j++;
				continue;
			}
			if (s[j] == q || s[j] == '\n')
				return j + 1;
		}
		return n;
	}
	return i;
}

/* Vorspann eines Blocks (Selektor oder @-Regel) einordnen */
enum block { B_REGEL, B_REGEL_WURZEL, B_MEDIA, B_DURCHLAESSIG, B_ANDERE };

static bool wurzel_selektor(const char *s, size_t n)
{
	size_t i = 0, a;
	/* jeder Teil der Liste muss :root, html, body oder * sein */
	while (i < n) {
		while (i < n && (isspace((unsigned char)s[i]) || s[i] == ','))
			i++;
		a = i;
		while (i < n && s[i] != ',')
			i++;
		if (i > a) {
			size_t e = i;
			while (e > a && isspace((unsigned char)s[e - 1]))
				e--;
			if (!((e - a == 5 && strncmp(s + a, ":root", 5) == 0) || (e - a == 4 && strncmp(s + a, "html", 4) == 0) ||
			      (e - a == 4 && strncmp(s + a, "body", 4) == 0) || (e - a == 1 && s[a] == '*') ||
			      (e - a == 5 && strncmp(s + a, ":host", 5) == 0)))
				return false;
		}
	}
	return true;
}

/* Steht „dark“ im Vorspann (Selektor oder @media-Bedingung)? */
static bool dunkel(const char *s, size_t n)
{
	size_t i;
	for (i = 0; i + 4 <= n; i++)
		if (strncasecmp(s + i, "dark", 4) == 0)
			return true;
	return false;
}

static enum block einordnen(const char *s, size_t n)
{
	while (n > 0 && isspace((unsigned char)*s)) {
		s++;
		n--;
	}
	while (n > 0 && isspace((unsigned char)s[n - 1]))
		n--;
	if (n > 0 && s[0] == '@') {
		if (n >= 6 && strncmp(s, "@media", 6) == 0)
			return B_MEDIA;
		if ((n >= 9 && strncmp(s, "@supports", 9) == 0) || (n >= 6 && strncmp(s, "@layer", 6) == 0) ||
		    (n >= 10 && strncmp(s, "@container", 10) == 0) || (n >= 9 && strncmp(s, "@document", 9) == 0))
			return B_DURCHLAESSIG;
		return B_ANDERE;
	}
	return wurzel_selektor(s, n) ? B_REGEL_WURZEL : B_REGEL;
}

#define TIEFE 24

/* Eigene Eigenschaften im Text sammeln; `inline` = style-Attribut (eine
 * Deklarationsliste ohne Selektor, Vorrang 1) */
static void sammeln(const char *s, size_t n, bool inline_stil)
{
	enum block stapel[TIEFE];
	bool dunkel_stapel[TIEFE];
	size_t tiefe = 0, i = 0, vorspann = 0;
	bool in_regel = inline_stil;
	uint8_t vorrang = 1;
	while (i < n) {
		size_t j = ueberspringen(s, n, i);
		if (j != i) {
			i = j;
			continue;
		}
		if (in_regel) {
			/* Deklaration: Leerraum, Name, ':' */
			size_t a, e, k, klammern = 0;
			while (i < n && (isspace((unsigned char)s[i]) || s[i] == ';'))
				i++;
			if (i >= n)
				break;
			if (s[i] == '}') {
				if (tiefe > 0)
					tiefe--;
				in_regel = false;
				vorspann = ++i;
				continue;
			}
			a = i;
			while (i < n && s[i] != ':' && s[i] != ';' && s[i] != '}' && s[i] != '{')
				i++;
			if (i >= n)
				break;
			if (s[i] == '{') {
				/* verschachtelte Regel (CSS Nesting): als normale Regel */
				bool d = dunkel(s + a, i - a) || (tiefe > 0 && dunkel_stapel[tiefe - 1]);
				if (tiefe < TIEFE) {
					dunkel_stapel[tiefe] = d;
					stapel[tiefe++] = B_REGEL;
				}
				vorrang = d ? 0 : 1;
				i++;
				continue;
			}
			if (s[i] != ':')
				continue;
			e = i;
			while (e > a && isspace((unsigned char)s[e - 1]))
				e--;
			k = ++i;
			while (i < n) {
				j = ueberspringen(s, n, i);
				if (j != i) {
					i = j;
					continue;
				}
				if (s[i] == '(')
					klammern++;
				else if (s[i] == ')' && klammern > 0)
					klammern--;
				else if (klammern == 0 && (s[i] == ';' || s[i] == '}'))
					break;
				i++;
			}
			if (e - a > 2 && s[a] == '-' && s[a + 1] == '-')
				var_setzen(s + a, e - a, s + k, i - k, vorrang);
			continue;
		}
		if (s[i] == '{') {
			enum block b = einordnen(s + vorspann, i - vorspann);
			bool media = false, d = dunkel(s + vorspann, i - vorspann);
			size_t t;
			for (t = 0; t < tiefe; t++) {
				if (stapel[t] == B_MEDIA)
					media = true;
				if (dunkel_stapel[t])
					d = true;
			}
			if (tiefe < TIEFE) {
				dunkel_stapel[tiefe] = d;
				stapel[tiefe++] = b;
			}
			if (b == B_REGEL || b == B_REGEL_WURZEL) {
				in_regel = true;
				vorrang = d ? 0 : b == B_REGEL_WURZEL ? (media ? 2 : 3) : 1;
			}
			vorspann = ++i;
			continue;
		}
		if (s[i] == '}') {
			if (tiefe > 0)
				tiefe--;
			vorspann = ++i;
			continue;
		}
		if (s[i] == ';') {
			/* @import, @charset … */
			vorspann = ++i;
			continue;
		}
		i++;
	}
}

/* ------------------------------------------------------ var() */

static bool namenszeichen(char c)
{
	return isalnum((unsigned char)c) || c == '-' || c == '_' || (unsigned char)c >= 0x80;
}

/* Ende der Klammer, die bei `i` (nach „(“) offen ist: Stelle der „)“ */
static size_t klammer_ende(const char *s, size_t n, size_t i)
{
	size_t tiefe = 1;
	while (i < n) {
		size_t j = ueberspringen(s, n, i);
		if (j != i) {
			i = j;
			continue;
		}
		if (s[i] == '(')
			tiefe++;
		else if (s[i] == ')' && --tiefe == 0)
			return i;
		i++;
	}
	return n;
}

static void var_ersetzen(struct puffer *aus, const char *s, size_t n, int tiefe);

/* `var(` bei `i` (Stelle nach der Klammer `a`): ersetzt nach `aus`, liefert
 * die Stelle nach der „)“; false = unverändert lassen */
static bool ein_var(struct puffer *aus, const char *s, size_t n, size_t a, size_t *weiter, int tiefe)
{
	size_t e = klammer_ende(s, n, a), i = a, nb, ne;
	struct variable *v;
	if (e >= n || tiefe > 8)
		return false;
	while (i < e && isspace((unsigned char)s[i]))
		i++;
	nb = i;
	while (i < e && namenszeichen(s[i]))
		i++;
	ne = i;
	if (ne - nb < 3 || s[nb] != '-' || s[nb + 1] != '-')
		return false;
	while (i < e && isspace((unsigned char)s[i]))
		i++;
	v = var_suchen(s + nb, ne - nb, false);
	if (v != NULL && v->name != NULL) {
		var_ersetzen(aus, v->wert, strlen(v->wert), tiefe + 1);
		vars_ersetzt++;
	} else if (i < e && s[i] == ',') {
		var_ersetzen(aus, s + i + 1, e - i - 1, tiefe + 1);
		vars_ersetzt++;
	} else {
		vars_unbekannt++;
		return false;
	}
	*weiter = e + 1;
	return true;
}

static void var_ersetzen(struct puffer *aus, const char *s, size_t n, int tiefe)
{
	size_t i = 0;
	while (i < n) {
		size_t j = ueberspringen(s, n, i);
		if (j != i) {
			if (s[i] != '/')
				p_text(aus, s + i, j - i); /* Zeichenketten bleiben, Kommentare fallen weg */
			i = j;
			continue;
		}
		if (i + 4 <= n && (s[i] == 'v' || s[i] == 'V') && strncasecmp(s + i, "var(", 4) == 0 &&
		    (i == 0 || !namenszeichen(s[i - 1]))) {
			size_t weiter;
			if (ein_var(aus, s, n, i + 4, &weiter, tiefe)) {
				i = weiter;
				continue;
			}
		}
		p_zeichen(aus, s[i]);
		i++;
	}
}

/* ------------------------------------------------------ calc() */

struct wert {
	double z;
	char einheit[8]; /* "" = ohne */
	bool ok;
};

struct leser {
	const char *s;
	size_t i, n;
};

static void l_leer(struct leser *l)
{
	while (l->i < l->n && isspace((unsigned char)l->s[l->i]))
		l->i++;
}

static struct wert ausdruck(struct leser *l);

static struct wert faktor(struct leser *l)
{
	struct wert w = { 0, "", false };
	char *ende;
	char zahl[40];
	size_t a, k;
	l_leer(l);
	if (l->i >= l->n)
		return w;
	if (l->s[l->i] == '(' || (l->n - l->i >= 5 && strncasecmp(l->s + l->i, "calc(", 5) == 0)) {
		l->i += l->s[l->i] == '(' ? 1 : 5;
		w = ausdruck(l);
		l_leer(l);
		if (l->i >= l->n || l->s[l->i] != ')') {
			w.ok = false;
			return w;
		}
		l->i++;
		return w;
	}
	a = l->i;
	if (l->s[l->i] == '-' || l->s[l->i] == '+')
		l->i++;
	while (l->i < l->n && (isdigit((unsigned char)l->s[l->i]) || l->s[l->i] == '.'))
		l->i++;
	if (l->i - a == 0 || l->i - a >= sizeof zahl)
		return w;
	memcpy(zahl, l->s + a, l->i - a);
	zahl[l->i - a] = 0;
	w.z = strtod(zahl, &ende);
	if (*ende != 0)
		return w;
	k = 0;
	while (l->i < l->n && (isalpha((unsigned char)l->s[l->i]) || l->s[l->i] == '%') && k < sizeof w.einheit - 1)
		w.einheit[k++] = (char)tolower((unsigned char)l->s[l->i++]);
	w.einheit[k] = 0;
	if (l->i < l->n && (isalpha((unsigned char)l->s[l->i]) || l->s[l->i] == '%'))
		return w; /* Einheit zu lang */
	w.ok = true;
	return w;
}

static struct wert term(struct leser *l)
{
	struct wert a = faktor(l);
	while (a.ok) {
		char op;
		struct wert b;
		l_leer(l);
		if (l->i >= l->n || (l->s[l->i] != '*' && l->s[l->i] != '/'))
			break;
		op = l->s[l->i++];
		b = faktor(l);
		if (!b.ok) {
			a.ok = false;
			break;
		}
		if (op == '*') {
			if (a.einheit[0] && b.einheit[0]) {
				a.ok = false;
				break;
			}
			if (!a.einheit[0])
				memcpy(a.einheit, b.einheit, sizeof a.einheit);
			a.z *= b.z;
		} else {
			if (b.einheit[0] || b.z == 0) {
				a.ok = false;
				break;
			}
			a.z /= b.z;
		}
	}
	return a;
}

static struct wert ausdruck(struct leser *l)
{
	struct wert a = term(l);
	while (a.ok) {
		char op;
		struct wert b;
		l_leer(l);
		if (l->i >= l->n || (l->s[l->i] != '+' && l->s[l->i] != '-'))
			break;
		op = l->s[l->i++];
		b = term(l);
		if (!b.ok) {
			a.ok = false;
			break;
		}
		/* gleiche Einheit; 0 ohne Einheit passt zu jeder */
		if (strcmp(a.einheit, b.einheit) != 0) {
			if (!b.einheit[0] && b.z == 0)
				memcpy(b.einheit, a.einheit, sizeof b.einheit);
			else if (!a.einheit[0] && a.z == 0)
				memcpy(a.einheit, b.einheit, sizeof a.einheit);
			else {
				a.ok = false;
				break;
			}
		}
		a.z = op == '+' ? a.z + b.z : a.z - b.z;
	}
	return a;
}

/* calc-Inhalt s[0..n) ausrechnen; true und Text nach `aus` */
static bool calc_rechnen(const char *s, size_t n, struct puffer *aus)
{
	struct leser l = { s, 0, n };
	struct wert w = ausdruck(&l);
	char t[48];
	l_leer(&l);
	if (!w.ok || l.i != n)
		return false;
	snprintf(t, sizeof t, "%.4f", w.z);
	/* Nullen am Ende weg */
	{
		size_t k = strlen(t);
		while (k > 0 && t[k - 1] == '0')
			t[--k] = 0;
		if (k > 0 && t[k - 1] == '.')
			t[--k] = 0;
		if (strcmp(t, "-0") == 0)
			strcpy(t, "0");
	}
	p_text(aus, t, strlen(t));
	p_text(aus, w.einheit, strlen(w.einheit));
	return true;
}

/* ------------------------------------- rgb()/hsl() und Eigenschaften */

/* Leerzeichen-Schreibweise „rgb(a b c / d)“ → „rgba(a,b,c,d)“; Inhalt
 * s[0..n) ohne Kommas, sonst false */
static bool farbe_umschreiben(const char *name, size_t nn, const char *s, size_t n, struct puffer *aus)
{
	const char *teil[4];
	size_t laenge[4], k = 0, i = 0;
	bool alpha = false;
	if (memchr(s, ',', n) != NULL)
		return false;
	while (i < n && k < 4) {
		while (i < n && isspace((unsigned char)s[i]))
			i++;
		if (i < n && s[i] == '/') {
			if (k != 3)
				return false;
			alpha = true;
			i++;
			continue;
		}
		if (i >= n)
			break;
		teil[k] = s + i;
		while (i < n && !isspace((unsigned char)s[i]) && s[i] != '/')
			i++;
		laenge[k] = (size_t)(s + i - teil[k]);
		k++;
	}
	while (i < n && isspace((unsigned char)s[i]))
		i++;
	if (i != n || k < 3 || (k == 4) != alpha)
		return false;
	p_text(aus, name, nn);
	if (k == 4 && name[nn - 1] != 'a')
		p_zeichen(aus, 'a');
	p_zeichen(aus, '(');
	for (i = 0; i < k; i++) {
		if (i)
			p_zeichen(aus, ',');
		/* Alpha in Prozent → Zahl (libcss erwartet 0..1) */
		if (i == 3 && laenge[3] > 1 && teil[3][laenge[3] - 1] == '%') {
			char t[24], z[24];
			size_t m = laenge[3] - 1 < sizeof z - 1 ? laenge[3] - 1 : sizeof z - 1;
			memcpy(z, teil[3], m);
			z[m] = 0;
			snprintf(t, sizeof t, "%.3g", strtod(z, NULL) / 100.0);
			p_text(aus, t, strlen(t));
		} else {
			p_text(aus, teil[i], laenge[i]);
		}
	}
	p_zeichen(aus, ')');
	return true;
}

static const char *const LOGISCH[][2] = {
	{ "margin-inline-start", "margin-left" },   { "margin-inline-end", "margin-right" },
	{ "margin-block-start", "margin-top" },     { "margin-block-end", "margin-bottom" },
	{ "padding-inline-start", "padding-left" }, { "padding-inline-end", "padding-right" },
	{ "padding-block-start", "padding-top" },   { "padding-block-end", "padding-bottom" },
	{ "inset-inline-start", "left" },           { "inset-inline-end", "right" },
	{ "inset-block-start", "top" },             { "inset-block-end", "bottom" },
	{ "inline-size", "width" },                 { "block-size", "height" },
	{ "min-inline-size", "min-width" },         { "max-inline-size", "max-width" },
	{ "min-block-size", "min-height" },         { "max-block-size", "max-height" },
	{ "border-inline-start", "border-left" },   { "border-inline-end", "border-right" },
	{ "border-block-start", "border-top" },     { "border-block-end", "border-bottom" },
};

static unsigned calc_gerechnet, farben_umgeschrieben, logisch_umbenannt;

/* Beginnt s[i..n) mit `w` (ohne Gross/Klein), ohne Namenszeichen davor? */
static bool beginnt(const char *s, size_t i, size_t n, const char *w)
{
	size_t m = strlen(w);
	return n - i >= m && strncasecmp(s + i, w, m) == 0 && (i == 0 || !namenszeichen(s[i - 1]));
}

/* Zweiter Durchgang: calc(), rgb()/hsl(), logische Eigenschaften */
static void umschreiben(struct puffer *aus, const char *s, size_t n)
{
	size_t i = 0;
	bool name_erwartet = true; /* nach '{' oder ';' */
	while (i < n) {
		size_t j = ueberspringen(s, n, i);
		if (j != i) {
			p_text(aus, s + i, j - i);
			i = j;
			continue;
		}
		if (name_erwartet && isalpha((unsigned char)s[i])) {
			size_t a = i, k;
			while (i < n && namenszeichen(s[i]))
				i++;
			for (k = 0; k < sizeof LOGISCH / sizeof LOGISCH[0]; k++) {
				size_t m = strlen(LOGISCH[k][0]);
				if (i - a == m && strncasecmp(s + a, LOGISCH[k][0], m) == 0) {
					size_t b = i;
					while (b < n && isspace((unsigned char)s[b]))
						b++;
					if (b < n && s[b] == ':') {
						p_text(aus, LOGISCH[k][1], strlen(LOGISCH[k][1]));
						logisch_umbenannt++;
						break;
					}
				}
			}
			if (k == sizeof LOGISCH / sizeof LOGISCH[0])
				p_text(aus, s + a, i - a);
			name_erwartet = false;
			continue;
		}
		if (s[i] == '{' || s[i] == ';')
			name_erwartet = true;
		else if (!isspace((unsigned char)s[i]))
			name_erwartet = false;
		if (beginnt(s, i, n, "calc(")) {
			size_t e = klammer_ende(s, n, i + 5);
			if (e < n && calc_rechnen(s + i + 5, e - i - 5, aus)) {
				calc_gerechnet++;
				i = e + 1;
				continue;
			}
		}
		if (beginnt(s, i, n, "rgb(") || beginnt(s, i, n, "rgba(") || beginnt(s, i, n, "hsl(") || beginnt(s, i, n, "hsla(")) {
			size_t nn = s[i + 3] == '(' ? 3 : 4;
			if (i + nn < n && s[i + nn] == '(') {
				size_t e = klammer_ende(s, n, i + nn + 1);
				if (e < n && farbe_umschreiben(s + i, nn, s + i + nn + 1, e - i - nn - 1, aus)) {
					farben_umgeschrieben++;
					i = e + 1;
					continue;
				}
			}
		}
		p_zeichen(aus, s[i]);
		i++;
	}
}

/* Ganzer Text: sammeln, var() ersetzen, umschreiben. Ergebnis mit malloc
 * (Aufrufer gibt frei) oder NULL (kein Speicher: Text unverändert nutzen). */
static char *vorbereiten(const char *s, size_t n, bool inline_stil, size_t *aus_n)
{
	struct puffer a = { 0 }, b = { 0 };
	sammeln(s, n, inline_stil);
	var_ersetzen(&a, s, n, 0);
	if (a.fehler) {
		free(a.d);
		return NULL;
	}
	umschreiben(&b, a.d ? a.d : "", a.n);
	free(a.d);
	if (b.fehler) {
		free(b.d);
		return NULL;
	}
	*aus_n = b.n;
	return b.d ? b.d : kopie("", 0);
}

/* ------------------------------------------ Stücke je Stylesheet */

#define PLAETZE 24

static struct rest {
	const void *schluessel;
	struct puffer p;
} reste[PLAETZE];

static struct rest *rest_von(const void *k, bool anlegen)
{
	size_t i;
	for (i = 0; i < PLAETZE; i++)
		if (reste[i].schluessel == k)
			return &reste[i];
	if (!anlegen)
		return NULL;
	for (i = 0; i < PLAETZE; i++)
		if (reste[i].schluessel == NULL) {
			reste[i].schluessel = k;
			return &reste[i];
		}
	return NULL;
}

/* Ende der letzten vollständigen Regel in s[0..n): Stelle nach der „}“,
 * die auf die oberste Ebene zurückführt (sonst begänne das nächste Stück
 * mitten in einem @media-Block); gibt es keine und ist der Puffer schon
 * gross (64 KB), die letzte „}“ überhaupt. */
static size_t letzte_regel(const char *s, size_t n)
{
	size_t i = 0, ende0 = 0, ende = 0, tiefe = 0;
	while (i < n) {
		size_t j = ueberspringen(s, n, i);
		if (j != i) {
			/* unvollständiger Kommentar/Zeichenkette am Ende: nicht weiter */
			if (j >= n && !((s[i] == '/' && n >= 2 && s[n - 2] == '*' && s[n - 1] == '/') ||
			                ((s[i] == '"' || s[i] == '\'') && s[n - 1] == s[i] && n - 1 > i)))
				break;
			i = j;
			continue;
		}
		if (s[i] == '{') {
			tiefe++;
		} else if (s[i] == '}') {
			if (tiefe > 0)
				tiefe--;
			ende = i + 1;
			if (tiefe == 0)
				ende0 = i + 1;
		}
		i++;
	}
	return ende0 > 0 ? ende0 : (n > 64u * 1024u ? ende : 0);
}

/* Ein Stück Stylesheet `k`: liefert vorbereiteten Text (malloc, `*aus_n`)
 * bis zur letzten vollständigen Regel; der Rest wartet. NULL und *aus_n = 0:
 * nichts weiterzugeben; NULL und *aus_n = n: Stück unverändert weitergeben
 * (kein Platz/Speicher, Text mit Nullbytes). */
char *ns_css_stueck(const void *k, const char *s, size_t n, size_t *aus_n)
{
	struct rest *r;
	size_t ende;
	char *t;
	*aus_n = n;
	if (memchr(s, 0, n) != NULL)
		return NULL; /* UTF-16 o. ä.: unverändert */
	r = rest_von(k, true);
	if (r == NULL)
		return NULL;
	p_text(&r->p, s, n);
	if (r->p.fehler) {
		/* kein Speicher: alles Bisherige unverändert weiter wäre falsch
		 * (Teile schon vorbereitet) – Rest verwerfen, Stück direkt */
		free(r->p.d);
		memset(r, 0, sizeof *r);
		return NULL;
	}
	ende = letzte_regel(r->p.d, r->p.n);
	if (ende == 0) {
		*aus_n = 0;
		return NULL;
	}
	t = vorbereiten(r->p.d, ende, false, aus_n);
	if (t == NULL) {
		/* kein Speicher: diesen Teil unverändert weitergeben */
		t = kopie(r->p.d, ende);
		*aus_n = t ? ende : 0;
	}
	memmove(r->p.d, r->p.d + ende, r->p.n - ende);
	r->p.n -= ende;
	r->p.d[r->p.n] = 0;
	return t;
}

/* Letztes Stück von `k` (Daten fertig): vorbereiteter Rest oder NULL
 * (*aus_n = 0: nichts übrig); gibt den Platz frei. */
char *ns_css_ende(const void *k, size_t *aus_n)
{
	struct rest *r = rest_von(k, false);
	char *t = NULL;
	*aus_n = 0;
	if (r == NULL)
		return NULL;
	if (r->p.n > 0) {
		t = vorbereiten(r->p.d, r->p.n, false, aus_n);
		if (t == NULL) {
			t = kopie(r->p.d, r->p.n);
			*aus_n = t ? r->p.n : 0;
		}
	}
	free(r->p.d);
	memset(r, 0, sizeof *r);
	return t;
}

/* Stylesheet `k` verworfen (Abbruch): Platz freigeben */
void ns_css_vergessen(const void *k)
{
	struct rest *r = rest_von(k, false);
	if (r != NULL) {
		free(r->p.d);
		memset(r, 0, sizeof *r);
	}
}

/* style-Attribut: ganzer Text auf einmal (NULL = unverändert nutzen) */
char *ns_css_inline(const char *s, size_t n, size_t *aus_n)
{
	if (memchr(s, 0, n) != NULL)
		return NULL;
	return vorbereiten(s, n, true, aus_n);
}

/* Zähler fürs Protokoll (Board: Zeile „ns_css“) und die Tests */
void ns_css_vorbereitung_stand(unsigned *variablen, unsigned *ersetzt, unsigned *unbekannt, unsigned *calc,
			       unsigned *farben, unsigned *logisch)
{
	*variablen = (unsigned)vars_anzahl;
	*ersetzt = vars_ersetzt;
	*unbekannt = vars_unbekannt;
	*calc = calc_gerechnet;
	*farben = farben_umgeschrieben;
	*logisch = logisch_umbenannt;
}
