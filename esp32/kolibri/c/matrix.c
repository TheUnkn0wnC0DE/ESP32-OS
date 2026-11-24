/* ESP32-OS, C1: „Matrix“ – fallende Zeichen in der Konsole (console.obj).
 * Eigenes Programm (GPL-2.0) statt kmatrix: kmatrix steht unter GPL-3.0-or-
 * later und darf nicht mit NetSurf (GPL-2.0-only) in eine Firmware (siehe
 * PLAN, Abweichungen). Nutzt die Konsole wie kmatrix: Farben über
 * Esc-Sequenzen, Schreibmarke setzen, Taste beendet (kbhit/getch), Fenster
 * schliessen beendet ebenfalls.
 *
 * Parameter: Zahl der Schritte (0 = bis zur Taste), Startwert des Zufalls. */
#include <conio.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ksys.h>

#define BREITE 80
#define HOEHE 25
#define SPALTEN (BREITE / 2)

static unsigned zufall_stand = 1;

static unsigned zufall(void)
{
    zufall_stand = zufall_stand * 1103515245u + 12345u;
    return (zufall_stand >> 16) & 0x7FFF;
}

struct spur {
    int kopf;  /* Zeile des hellen Kopfs, < 0: wartet */
    int laenge;
};

int main(int argc, char** argv)
{
    struct spur s[SPALTEN];
    int schritte = argc > 1 ? atoi(argv[1]) : 0;
    zufall_stand = argc > 2 ? (unsigned)atoi(argv[2]) : (unsigned)_ksys_get_tick_count() | 1u;
    if (con_init_opt(BREITE, HOEHE, BREITE, HOEHE, "Matrix") != 0) {
        return 1;
    }
    (*con_set_cursor_height)(0);
    printf("\033[0;40m\033[2J");
    for (int i = 0; i < SPALTEN; i++) {
        s[i].kopf = -(int)(zufall() % HOEHE);
        s[i].laenge = 4 + (int)(zufall() % 14);
    }
    for (int n = 0; schritte == 0 || n < schritte; n++) {
        if ((*con_kbhit)()) {
            (*con_getch)();
            break;
        }
        for (int i = 0; i < SPALTEN; i++) {
            struct spur* p = &s[i];
            int x = i * 2;
            if (p->kopf >= 0 && p->kopf < HOEHE) {
                /* alter Kopf wird grün, neuer Kopf hell */
                if (p->kopf > 0) {
                    (*con_set_cursor_pos)(x, p->kopf - 1);
                    printf("\033[0;32m%c", 33 + (int)(zufall() % 94));
                }
                (*con_set_cursor_pos)(x, p->kopf);
                printf("\033[1;37m%c", 33 + (int)(zufall() % 94));
            }
            int ende = p->kopf - p->laenge;
            if (ende >= 0 && ende < HOEHE) {
                (*con_set_cursor_pos)(x, ende);
                printf(" ");
            }
            p->kopf++;
            if (p->kopf - p->laenge > HOEHE) {
                p->kopf = -(int)(zufall() % 10);
                p->laenge = 4 + (int)(zufall() % 14);
            }
        }
        _ksys_delay(5);
    }
    printf("\033[0m");
    /* feste Schrittzahl (Vorführung, Test): Fenster bleibt mit „[Finished]“
     * stehen, bis es geschlossen wird; nach einer Taste gleich zu */
    (*con_exit)(schritte == 0 ? 1 : 0);
    return 0;
}
