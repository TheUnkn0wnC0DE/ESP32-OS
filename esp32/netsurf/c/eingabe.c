/* ESP32-OS, B2: Eingabe der Adressleiste → Adresse oder Suche.
 *
 * NetSurfs search_web_omni würde ein einzelnes Wort wie „esp32“ als
 * https://esp32 laden. Hier entscheidet eine klare Regel:
 * - mit Schema (…://, about:, file:, data:, resource:, javascript:,
 *   mailto:) → so wie eingegeben;
 * - ein einzelnes Wort mit Punkt (wikipedia.org, a.b/c) oder „localhost“
 *   (auch mit :Port/Pfad) → https://…;
 * - sonst (Wörter, Leerzeichen, Fragen) → Suche beim eingestellten Anbieter
 *   (SearchEngines, DuckDuckGo-HTML zuerst) über search_web_omni.
 * ns_eingabe_art() ist die reine Regel (PC-Test), ns_eingabe_zu_url()
 * erzeugt die Adresse.
 */
#include <ctype.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "utils/errors.h"
#include "utils/nsurl.h"
#include "desktop/searchweb.h"

/* 0 = Suche, 1 = Adresse wie eingegeben, 2 = Adresse mit https:// davor.
 * `aus` bekommt den bereinigten Text (ohne Leerraum am Rand). */
int ns_eingabe_art(const char *text, char *aus, size_t max)
{
	static const char *schemata[] = { "about:", "file:", "data:", "resource:", "javascript:", "mailto:" };
	size_t a = 0, l = strlen(text), i;
	bool leer = false;
	const char *host_ende;
	while (a < l && isspace((unsigned char)text[a]))
		a++;
	while (l > a && isspace((unsigned char)text[l - 1]))
		l--;
	if (l - a >= max)
		l = a + max - 1;
	memcpy(aus, text + a, l - a);
	aus[l - a] = '\0';
	if (aus[0] == '\0')
		return 0;
	if (strstr(aus, "://") != NULL)
		return 1;
	for (i = 0; i < sizeof schemata / sizeof schemata[0]; i++)
		if (strncasecmp(aus, schemata[i], strlen(schemata[i])) == 0)
			return 1;
	for (i = 0; aus[i]; i++)
		if (isspace((unsigned char)aus[i]))
			leer = true;
	if (leer)
		return 0;
	/* Rechnername bis zu Port, Pfad, Abfrage */
	host_ende = aus + strcspn(aus, ":/?#");
	if ((size_t)(host_ende - aus) == 9 && strncasecmp(aus, "localhost", 9) == 0)
		return 2;
	{
		const char *punkt = memchr(aus, '.', (size_t)(host_ende - aus));
		/* Punkt im Namen, nicht am Rand, danach mindestens ein Buchstabe
		 * oder eine Ziffer (keine „1.5“-Zahl ohne Buchstaben davor) */
		if (punkt != NULL && punkt != aus && punkt + 1 < host_ende) {
			bool buchstabe = false;
			const char *p;
			for (p = aus; p < host_ende; p++)
				if (isalpha((unsigned char)*p))
					buchstabe = true;
			bool ziffern_punkte = true;
			for (p = aus; p < host_ende; p++)
				if (!isdigit((unsigned char)*p) && *p != '.')
					ziffern_punkte = false;
			/* IPv4-Adresse (1.2.3.4) oder Name mit Buchstaben */
			if (buchstabe || (ziffern_punkte && host_ende - aus >= 7))
				return 2;
		}
	}
	return 0;
}

nserror ns_eingabe_zu_url(const char *text, struct nsurl **url)
{
	char t[2048], mit[2100];
	switch (ns_eingabe_art(text, t, sizeof t)) {
	case 1:
		return nsurl_create(t, url);
	case 2:
		snprintf(mit, sizeof mit, "https://%s", t);
		return nsurl_create(mit, url);
	default:
		if (t[0] == '\0')
			return NSERROR_BAD_URL;
		return search_web_omni(t, SEARCH_WEB_OMNI_SEARCHONLY, url);
	}
}
