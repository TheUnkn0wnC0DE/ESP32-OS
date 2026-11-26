/* ESP32-OS, Browser (Q3): welche CSS-Funktionen libcss nicht versteht.
 *
 * Die geprüfte Kopie von libcss/src/parse/language.c (build.rs,
 * css_sprache_kopie) meldet jede verworfene Deklaration:
 * - Art 0: unbekannte Eigenschaft (z. B. „gap“); Präfix-Eigenschaften
 *   zusammengefasst („-webkit-*“). Eigene Eigenschaften („--farbe“)
 *   kommen nicht bis hierher (der Lexer von libcss 0.9 liest „--name“
 *   nicht als Namen) – ihr Gebrauch zeigt sich als „var()“;
 * - Art 1: bekannte Eigenschaft, Wert mit einer Funktion, die libcss nicht
 *   kennt (Schlüssel „var()“, „calc()“, „clamp()“ …);
 * - Art 2: bekannte Eigenschaft, Wert sonst ungültig („wert:display“).
 * Gezählt wird in einer festen Tabelle (keine Belegung im Heap); die
 * Statik liegt im Abschnitt des Programms, jeder Browserstart beginnt bei
 * null. Ausgabe: ns_css_luecken_text (Board: Zeile „ns_css“, am PC Test).
 */
#include <stddef.h>
#include <stdio.h>
#include <string.h>

#define LUECKEN 160
#define LUECKE_NAME 40

struct luecke {
	char name[LUECKE_NAME];
	unsigned n;
};

static struct luecke luecken[LUECKEN];
static unsigned luecken_belegt;
static unsigned luecken_voll; /* Meldungen ohne freien Platz in der Tabelle */
static unsigned luecken_stand; /* Änderungszähler */

static void schluessel(int art, const char *name, size_t len, char *aus)
{
	size_t i, o = 0, max = LUECKE_NAME - 3;
	if (art == 2) {
		memcpy(aus, "wert:", 5);
		o = 5;
	}
	if (art == 0 && len >= 1 && name[0] == '-') {
		/* -webkit-box-flex → -webkit-* */
		const char *zweiter = memchr(name + 1, '-', len - 1);
		if (zweiter != NULL)
			len = (size_t)(zweiter - name) + 1;
	}
	for (i = 0; i < len && o < max; i++) {
		char z = name[i];
		if (z >= 'A' && z <= 'Z')
			z = (char)(z - 'A' + 'a');
		aus[o++] = z;
	}
	if (art == 0 && len >= 1 && name[0] == '-' && name[len - 1] == '-' && o < max)
		aus[o++] = '*';
	if (art == 1 && o + 2 <= LUECKE_NAME - 1) {
		aus[o++] = '(';
		aus[o++] = ')';
	}
	aus[o] = 0;
}

void ns_css_luecke(int art, const char *name, size_t len)
{
	char k[LUECKE_NAME];
	unsigned i;
	if (name == NULL || len == 0)
		return;
	schluessel(art, name, len, k);
	luecken_stand++;
	for (i = 0; i < luecken_belegt; i++) {
		if (strcmp(luecken[i].name, k) == 0) {
			luecken[i].n++;
			return;
		}
	}
	if (luecken_belegt == LUECKEN) {
		luecken_voll++;
		return;
	}
	strcpy(luecken[luecken_belegt].name, k);
	luecken[luecken_belegt].n = 1;
	luecken_belegt++;
}

/* Änderungszähler (Board: nur bei Änderung neu ausgeben) */
unsigned ns_css_luecken_stand(void)
{
	return luecken_stand;
}

/* Die häufigsten `anzahl` Lücken als „name n, name n, …“ nach `aus`
 * (abgeschnitten auf `max`); Rückgabe: Anzahl verschiedener Lücken. */
unsigned ns_css_luecken_text(char *aus, size_t max, unsigned anzahl)
{
	unsigned gezeigt = 0, i;
	size_t o = 0;
	unsigned char genommen[LUECKEN];
	if (max == 0)
		return luecken_belegt;
	aus[0] = 0;
	memset(genommen, 0, sizeof genommen);
	while (gezeigt < anzahl && gezeigt < luecken_belegt) {
		unsigned beste = LUECKEN;
		int w;
		for (i = 0; i < luecken_belegt; i++) {
			if (!genommen[i] && (beste == LUECKEN || luecken[i].n > luecken[beste].n))
				beste = i;
		}
		genommen[beste] = 1;
		w = snprintf(aus + o, max - o, "%s%s %u", gezeigt ? ", " : "", luecken[beste].name, luecken[beste].n);
		if (w < 0 || (size_t)w >= max - o)
			break;
		o += (size_t)w;
		gezeigt++;
	}
	if (luecken_voll > 0 && o < max)
		snprintf(aus + o, max - o, " (+%u ohne Platz)", luecken_voll);
	return luecken_belegt;
}
