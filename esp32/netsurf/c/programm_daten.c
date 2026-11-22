/* ESP32-OS, B1: Neustart von NetSurf als Programm. Programme teilen den
 * Adressraum; statische Daten (z. B. die Operationstabelle `guit`, die
 * Zeichenketten-Tabelle von libwapcaplet, Merker wie `fb_complete`) würden
 * ein Programmende überleben und beim nächsten Start auf freigegebenen
 * Speicher zeigen. Darum liegen alle beschreibbaren statischen Daten der
 * NetSurf-Bibliotheken in einem Abschnitt (compat/gemeinsam/ns_programm.h), der beim
 * ersten Start gesichert und bei jedem weiteren Start zurückgespielt wird.
 *
 * Diese Datei wird OHNE ns_programm.h übersetzt: die Sicherung selbst liegt
 * ausserhalb des Abschnitts und im Speicher des Systems (nicht im Heap des
 * Programms, der beim Ende verschwindet).
 *
 * PC (clang-cl/MSVC): Der Linker ordnet gleichnamige Abschnitte nach dem
 * Teil hinter '$': .nsd$a < .nsd$m (Daten) < .nsd$z, ebenso .nsb.
 * Board (GCC): Linker-Fragment mit SURROUND(netsurf_daten/netsurf_bss) –
 * die Grenzen heissen dort _netsurf_daten_start/_end usw. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#if defined(_MSC_VER)
#pragma section(".nsd$a", read, write)
#pragma section(".nsd$z", read, write)
#pragma section(".nsb$a", read, write)
#pragma section(".nsb$z", read, write)
__declspec(allocate(".nsd$a")) static volatile char daten_a = 1;
__declspec(allocate(".nsd$z")) static volatile char daten_z = 1;
__declspec(allocate(".nsb$a")) static volatile char bss_a = 1;
__declspec(allocate(".nsb$z")) static volatile char bss_z = 1;
#define DATEN_ANFANG ((uint8_t *)&daten_a + 1)
#define DATEN_ENDE ((uint8_t *)&daten_z)
#define BSS_ANFANG ((uint8_t *)&bss_a + 1)
#define BSS_ENDE ((uint8_t *)&bss_z)
#define SICHERUNG_HOLEN(n) malloc(n)
#else
extern uint8_t _netsurf_daten_start[], _netsurf_daten_end[];
extern uint8_t _netsurf_bss_start[], _netsurf_bss_end[];
#define DATEN_ANFANG _netsurf_daten_start
#define DATEN_ENDE _netsurf_daten_end
#define BSS_ANFANG _netsurf_bss_start
#define BSS_ENDE _netsurf_bss_end
#include "esp_heap_caps.h"
#define SICHERUNG_HOLEN(n) heap_caps_malloc((n), MALLOC_CAP_SPIRAM)
#endif

static uint8_t *sicherung;
static size_t daten_n, bss_n;

/* Beim Start des Programms: erstes Mal sichern, sonst zurückspielen.
 * Auch BSS wird gesichert statt genullt: Konstruktoren vor main (z. B.
 * die Flächen-Anmeldung von libnsfb, NSFB_SURFACE_DEF) haben dort schon
 * eingetragen. Liefert die Grösse (Daten + BSS) oder 0 bei Fehler. */
size_t ns_programm_daten_start(void)
{
	if (sicherung == NULL) {
		daten_n = (size_t)(DATEN_ENDE - DATEN_ANFANG);
		bss_n = (size_t)(BSS_ENDE - BSS_ANFANG);
		/* leerer Rahmen = Linker-Fragment greift nicht: NetSurf liefe ohne
		 * Neustart-Schutz, darum gar nicht */
		if (daten_n == 0 || bss_n == 0)
			return 0;
		sicherung = SICHERUNG_HOLEN(daten_n + bss_n + 1);
		if (sicherung == NULL)
			return 0;
		memcpy(sicherung, DATEN_ANFANG, daten_n);
		memcpy(sicherung + daten_n, BSS_ANFANG, bss_n);
	} else {
		memcpy(DATEN_ANFANG, sicherung, daten_n);
		memcpy(BSS_ANFANG, sicherung + daten_n, bss_n);
	}
	return daten_n + bss_n;
}
