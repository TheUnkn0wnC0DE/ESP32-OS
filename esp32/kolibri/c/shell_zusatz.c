/* ESP32-OS, D1: Umleitungen, Pipes und weitere Befehle für die
 * KolibriOS-Shell. Eingebunden am Ende von c/shell_haupt.c (gleiche
 * Übersetzungseinheit wie die Shell). Die Shell selbst bleibt bis auf einen
 * Aufruf in command_execute und die Einträge der neuen Befehle in der
 * Befehlstabelle unverändert.
 *
 *   befehl > datei      Ausgabe in die Datei (neu)
 *   befehl >> datei     Ausgabe an die Datei anhängen
 *   befehl < datei      Datei als Eingabe (cat, grep, wc, head, tail, sort)
 *   a | b | c           Ausgabe von a ist Eingabe von b …
 *
 * Aufgefangen wird die Ausgabe in der nativen Konsole des eigenen Fadens
 * (kolibri::konsole, kos_konsole_aufnahme_*): andere Konsolenprogramme
 * schreiben ungestört weiter. Gestartete Programme (keine eingebauten
 * Befehle) haben eine eigene Konsole; ihre Ausgabe lässt sich nicht
 * umleiten (Abweichung in PLAN D1). Zeilenenden in Dateien und Pipes: \n
 * (die Shell schreibt \n\r; \r wird entfernt). */

void __stdcall kos_konsole_aufnahme_start(void);
int __stdcall kos_konsole_aufnahme_ende(char* ziel, int max);
int __stdcall kos_konsole_aufnahme_laenge(void);

/* Höchstens so viel Text je Datei bzw. Pipe (Heap der Shell 1 MB; der
 * Kern gibt unter 1/16 freiem Heap NULL zurück – Review 24.11.) */
#define ZS_MAX (256 * 1024)

/* Eingabe der laufenden Pipe-Stufe (NULL: keine) */
static char* zs_ein = NULL;
static int zs_ein_n = 0;

/* Absoluter Pfad zu `name` (relativ zum aktuellen Ordner der Shell). */
static void zs_pfad(const char* name, char* aus)
{
    if (name[0] == '/') {
        strcpy(aus, name);
        return;
    }
    strcpy(aus, cur_dir);
    if (aus[0] && aus[strlen(aus) - 1] != '/')
        strcat(aus, "/");
    strcat(aus, name);
}

/* Datei ganz lesen (malloc, nullterminiert); NULL, wenn sie fehlt. */
static char* zs_lesen(const char* name, int* n)
{
    char pfad[FILENAME_MAX * 2];
    kol_struct70 k70;
    kol_struct_BDVK bdvk;
    char* b;
    zs_pfad(name, pfad);
    k70.p00 = 5;
    k70.p04 = 0;
    k70.p12 = 0;
    k70.p16 = (unsigned)&bdvk;
    k70.p20 = 0;
    k70.p21 = pfad;
    if (kol_file_70(&k70) != 0) {
        printf("  %s: not found\n\r", name);
        return NULL;
    }
    if (bdvk.p32 > ZS_MAX) {
        printf("  %s: too large (over %d KB)\n\r", name, ZS_MAX / 1024);
        return NULL;
    }
    *n = (int)bdvk.p32;
    b = (char*)malloc(*n + 1);
    if (!b) {
        printf("  out of memory\n\r");
        return NULL;
    }
    k70.p00 = 0;
    k70.p04 = 0;
    k70.p12 = *n;
    k70.p16 = (unsigned)b;
    k70.p20 = 0;
    k70.p21 = pfad;
    if (*n > 0) {
        int r = kol_file_70(&k70);
        if (r != 0 && r != 6) { /* 6 = Dateiende */
            printf("  %s: read error %d\n\r", name, r);
            free(b);
            return NULL;
        }
    }
    b[*n] = 0;
    return b;
}

/* Datei schreiben (neu) oder anhängen; 0 = ok. */
static int zs_schreiben(const char* name, const char* d, int n, int anhaengen)
{
    char pfad[FILENAME_MAX * 2];
    kol_struct70 k70;
    kol_struct_BDVK bdvk;
    unsigned long long pos = 0;
    zs_pfad(name, pfad);
    if (anhaengen) {
        k70.p00 = 5;
        k70.p04 = 0;
        k70.p12 = 0;
        k70.p16 = (unsigned)&bdvk;
        k70.p20 = 0;
        k70.p21 = pfad;
        if (kol_file_70(&k70) == 0)
            pos = bdvk.p32;
        else
            anhaengen = 0;
    }
    k70.p00 = anhaengen ? 3 : 2;
    k70.p04 = pos;
    k70.p12 = n;
    k70.p16 = (unsigned)d;
    k70.p20 = 0;
    k70.p21 = pfad;
    return kol_file_70(&k70);
}

/* \r entfernen (Ausgabe der Shell: \n\r). Gibt die neue Länge zurück. */
static int zs_ohne_cr(char* s, int n)
{
    int i, j = 0;
    for (i = 0; i < n; i++)
        if (s[i] != '\r')
            s[j++] = s[i];
    s[j] = 0;
    return j;
}

/* Text auf die Konsole: \n als \n\r (Zeilenanfang). */
static void zs_ausgeben(const char* s, int n)
{
    int i, a = 0;
    for (i = 0; i < n; i++) {
        if (s[i] == '\n') {
            if (i > a)
                con_write_string(s + a, i - a);
            con_write_asciiz("\n\r");
            a = i + 1;
        }
    }
    if (n > a)
        con_write_string(s + a, n - a);
}

/* Nächstes Wort aus *p (Anführungszeichen fassen Leerzeichen ein). */
static int zs_wort(char** p, char* aus, int max)
{
    char* s = *p;
    int n = 0, q = 0;
    while (*s == ' ' || *s == '\t')
        s++;
    if (!*s) {
        *p = s;
        return 0;
    }
    while (*s && (q || (*s != ' ' && *s != '\t'))) {
        if (*s == '"') {
            q = !q;
            s++;
            continue;
        }
        if (n < max - 1)
            aus[n++] = *s;
        s++;
    }
    aus[n] = 0;
    *p = s;
    return 1;
}

/* Eingabe eines Befehls: Dateien aus `rest`, sonst die Pipe. */
static char* zs_eingabe(char* rest, int* n)
{
    char name[FILENAME_MAX];
    char* alles = NULL;
    int gesamt = 0, gefunden = 0;
    while (zs_wort(&rest, name, sizeof name)) {
        int m;
        char* d = zs_lesen(name, &m);
        gefunden = 1;
        if (!d)
            continue;
        if (!alles) {
            alles = d;
            gesamt = m;
            continue;
        }
        if (gesamt + m > ZS_MAX) {
            printf("  input too large (over %d KB)\n\r", ZS_MAX / 1024);
            free(d);
            break;
        }
        {
            char* neu = (char*)realloc(alles, gesamt + m + 1);
            if (!neu) {
                printf("  out of memory\n\r");
                free(d);
                break;
            }
            alles = neu;
        }
        memcpy(alles + gesamt, d, m);
        gesamt += m;
        free(d);
    }
    if (!gefunden && zs_ein) {
        alles = (char*)malloc(zs_ein_n + 1);
        if (alles) {
            memcpy(alles, zs_ein, zs_ein_n);
            gesamt = zs_ein_n;
        }
    }
    if (!alles)
        alles = (char*)malloc(1);
    if (!alles) {
        printf("  out of memory\n\r");
        *n = 0;
        return NULL;
    }
    alles[gesamt] = 0;
    *n = zs_ohne_cr(alles, gesamt);
    return alles;
}

/* Text in Zeilen zerlegen (Zeiger in `t`, \n wird zu 0). */
static char** zs_zeilen(char* t, int n, int* anzahl)
{
    int i, k = 0, cap = 64;
    char** z = (char**)malloc(cap * sizeof(char*));
    char* a = t;
    *anzahl = 0;
    if (!z) {
        printf("  out of memory\n\r");
        return NULL;
    }
    for (i = 0; i <= n; i++) {
        if (i == n || t[i] == '\n') {
            if (i == n && a == t + n)
                break;
            t[i] = 0;
            if (k == cap) {
                char** neu = (char**)realloc(z, cap * 2 * sizeof(char*));
                if (!neu) {
                    printf("  out of memory\n\r");
                    free(z);
                    return NULL;
                }
                z = neu;
                cap *= 2;
            }
            z[k++] = a;
            a = t + i + 1;
        }
    }
    *anzahl = k;
    return z;
}

/* Optionen am Anfang (-x); gibt den Rest zurück. */
static char* zs_optionen(char* a, char* opt, int max, int* zahl)
{
    char w[64];
    char* p = a;
    int n = 0;
    opt[0] = 0;
    for (;;) {
        char* vor = p;
        if (!zs_wort(&p, w, sizeof w) || w[0] != '-' || w[1] == 0) {
            p = vor;
            break;
        }
        if (w[1] == 'n' && zahl) {
            if (w[2])
                *zahl = atoi(w + 2);
            else if (zs_wort(&p, w, sizeof w))
                *zahl = atoi(w);
            continue;
        }
        if (w[1] >= '0' && w[1] <= '9' && zahl) {
            *zahl = atoi(w + 1);
            continue;
        }
        if (n + (int)strlen(w + 1) < max - 1) {
            strcat(opt, w + 1);
            n += strlen(w + 1);
        }
    }
    return p;
}

int cmd_cat(char arg[])
{
    int n;
    char* t = zs_eingabe(arg, &n);
    if (!t)
        return TRUE;
    zs_ausgeben(t, n);
    free(t);
    return TRUE;
}

/* Eine Zeile mit Zeilenende (ohne den 2-KB-Puffer von printf). */
static void zs_zeile(const char* z)
{
    con_write_string(z, strlen(z));
    con_write_asciiz("\n\r");
}

static char zs_klein(char c)
{
    return (c >= 'A' && c <= 'Z') ? c + 32 : c;
}

static int zs_enthaelt(const char* z, const char* m, int ohne_gross)
{
    int i, j;
    if (!*m)
        return 1;
    for (i = 0; z[i]; i++) {
        for (j = 0; m[j] && z[i + j]; j++) {
            char a = z[i + j], b = m[j];
            if (ohne_gross) {
                a = zs_klein(a);
                b = zs_klein(b);
            }
            if (a != b)
                break;
        }
        if (!m[j])
            return 1;
    }
    return 0;
}

int cmd_grep(char arg[])
{
    char opt[16], muster[256];
    char* p = zs_optionen(arg, opt, sizeof opt, NULL);
    int n, k, i, treffer = 0;
    int ohne = strchr(opt, 'i') != NULL, nicht = strchr(opt, 'v') != NULL, zaehlen = strchr(opt, 'c') != NULL;
    char *t, **z;
    if (!zs_wort(&p, muster, sizeof muster)) {
        printf("  grep [-i] [-v] [-c] <text> [file ...]\n\r");
        return TRUE;
    }
    t = zs_eingabe(p, &n);
    if (!t)
        return TRUE;
    z = zs_zeilen(t, n, &k);
    if (!z) {
        free(t);
        return TRUE;
    }
    for (i = 0; i < k; i++) {
        if (zs_enthaelt(z[i], muster, ohne) != nicht) {
            treffer++;
            if (!zaehlen)
                zs_zeile(z[i]);
        }
    }
    if (zaehlen)
        printf("%d\n\r", treffer);
    free(z);
    free(t);
    return TRUE;
}

int cmd_wc(char arg[])
{
    char opt[16];
    char* p = zs_optionen(arg, opt, sizeof opt, NULL);
    int n, i, zeilen = 0, woerter = 0, im_wort = 0;
    char* t = zs_eingabe(p, &n);
    if (!t)
        return TRUE;
    for (i = 0; i < n; i++) {
        char c = t[i];
        if (c == '\n')
            zeilen++;
        if (c == ' ' || c == '\t' || c == '\n') {
            im_wort = 0;
        } else if (!im_wort) {
            im_wort = 1;
            woerter++;
        }
    }
    if (n > 0 && t[n - 1] != '\n')
        zeilen++;
    if (!opt[0])
        printf("%d %d %d\n\r", zeilen, woerter, n);
    else {
        int erst = 1;
        for (i = 0; opt[i]; i++) {
            int v = opt[i] == 'l' ? zeilen : opt[i] == 'w' ? woerter : n;
            printf(erst ? "%d" : " %d", v);
            erst = 0;
        }
        printf("\n\r");
    }
    free(t);
    return TRUE;
}

static int zs_kopf_schwanz(char arg[], int schwanz)
{
    char opt[16];
    int zahl = 10, n, k, i;
    char* p = zs_optionen(arg, opt, sizeof opt, &zahl);
    char* t = zs_eingabe(p, &n);
    char** z = t ? zs_zeilen(t, n, &k) : NULL;
    int von = schwanz ? (k > zahl ? k - zahl : 0) : 0;
    int bis = schwanz ? k : (zahl < k ? zahl : k);
    if (!z) {
        free(t);
        return TRUE;
    }
    for (i = von; i < bis; i++)
        zs_zeile(z[i]);
    free(z);
    free(t);
    return TRUE;
}

int cmd_head(char arg[])
{
    return zs_kopf_schwanz(arg, 0);
}

int cmd_tail(char arg[])
{
    return zs_kopf_schwanz(arg, 1);
}

static int zs_vergleich(const void* a, const void* b)
{
    return strcmp(*(char* const*)a, *(char* const*)b);
}

int cmd_sort(char arg[])
{
    char opt[16];
    char* p = zs_optionen(arg, opt, sizeof opt, NULL);
    int n, k, i;
    char* t = zs_eingabe(p, &n);
    char** z = t ? zs_zeilen(t, n, &k) : NULL;
    if (!z) {
        free(t);
        return TRUE;
    }
    qsort(z, k, sizeof(char*), zs_vergleich);
    if (strchr(opt, 'r'))
        for (i = k - 1; i >= 0; i--)
            zs_zeile(z[i]);
    else
        for (i = 0; i < k; i++)
            zs_zeile(z[i]);
    free(z);
    free(t);
    return TRUE;
}

/* Stellen von |, >, >>, < ausserhalb von Anführungszeichen? */
static int zs_hat_operator(const char* s)
{
    int q = 0;
    for (; *s; s++) {
        if (*s == '"')
            q = !q;
        else if (!q && (*s == '|' || *s == '>' || *s == '<'))
            return 1;
    }
    return 0;
}

/* Dateiname nach > / < (bis zum nächsten Operator), ohne Leerzeichen. */
static char* zs_ziel(char* s, char* aus, int max)
{
    int n = 0;
    while (*s == ' ' || *s == '\t')
        s++;
    while (*s && *s != ' ' && *s != '\t' && *s != '|' && *s != '<' && *s != '>') {
        if (n < max - 1)
            aus[n++] = *s;
        s++;
    }
    aus[n] = 0;
    return s;
}

/* Aufruf am Anfang von command_execute: 1 = Zeile mit Umleitung/Pipe
 * hier ausgeführt, 0 = normal weiter. */
int esp32os_umleitung(void)
{
    static int aktiv = 0;
    char zeile[FILENAME_MAX * 2];
    char* stufen[16];
    int anzahl = 0, i, q = 0;
    char aus_datei[FILENAME_MAX] = "", ein_datei[FILENAME_MAX] = "";
    int anhaengen = 0, hat_aus = 0, hat_ein = 0, stufe_aus = 0, stufe_ein = 0;
    char *s, *eingabe = NULL;
    int ein_n = 0;

    /* command_execute kopiert die Zeile in cmd[256]/args[256] (Original-
     * Shell): längere Zeilen ablehnen statt den Stapel zu überschreiben */
    if (strlen(CMD) > 250) {
        printf("  line too long (max 250 characters)\n\r");
        return 1;
    }
    if (aktiv || !zs_hat_operator(CMD))
        return 0;
    strcpy(zeile, CMD);
    /* zerlegen: Stufen an |, Umleitungen herausnehmen */
    stufen[anzahl++] = zeile;
    for (s = zeile; *s; s++) {
        if (*s == '"') {
            q = !q;
            continue;
        }
        if (q)
            continue;
        if (*s == '|') {
            *s = 0;
            if (anzahl == 16) {
                printf("  too many pipes\n\r");
                return 1;
            }
            stufen[anzahl++] = s + 1;
        } else if (*s == '>') {
            *s = 0;
            hat_aus = 1;
            stufe_aus = anzahl - 1;
            if (s[1] == '>') {
                anhaengen = 1;
                s++;
            }
            s = zs_ziel(s + 1, aus_datei, sizeof aus_datei) - 1;
        } else if (*s == '<') {
            *s = 0;
            hat_ein = 1;
            stufe_ein = anzahl - 1;
            s = zs_ziel(s + 1, ein_datei, sizeof ein_datei) - 1;
        }
    }
    if ((hat_aus && !aus_datei[0]) || (hat_ein && !ein_datei[0])) {
        printf("  missing file name after > or <\n\r");
        return 1;
    }
    if ((hat_aus && stufe_aus != anzahl - 1) || (hat_ein && stufe_ein != 0)) {
        printf("  > only after the last command, < only after the first\n\r");
        return 1;
    }
    if (ein_datei[0]) {
        eingabe = zs_lesen(ein_datei, &ein_n);
        if (!eingabe)
            return 1;
        ein_n = zs_ohne_cr(eingabe, ein_n);
    }
    aktiv = 1;
    for (i = 0; i < anzahl; i++) {
        int fangen = i < anzahl - 1 || aus_datei[0];
        char* aus = NULL;
        int aus_n = 0;
        strcpy(CMD, stufen[i]);
        trim(CMD);
        zs_ein = eingabe;
        zs_ein_n = ein_n;
        if (fangen)
            kos_konsole_aufnahme_start();
        if (CMD[0])
            command_execute();
        if (fangen) {
            aus_n = kos_konsole_aufnahme_laenge();
            aus = (char*)malloc(aus_n + 1);
            aus_n = kos_konsole_aufnahme_ende(aus, aus ? aus_n : 0);
            if (aus)
                aus_n = zs_ohne_cr(aus, aus_n);
            else
                printf("  out of memory\n\r");
        }
        free(eingabe);
        eingabe = aus;
        ein_n = aus_n;
    }
    zs_ein = NULL;
    zs_ein_n = 0;
    aktiv = 0;
    if (aus_datei[0] && zs_schreiben(aus_datei, eingabe ? eingabe : "", ein_n, anhaengen) != 0)
        printf("  cannot write %s\n\r", aus_datei);
    free(eingabe);
    return 1;
}
