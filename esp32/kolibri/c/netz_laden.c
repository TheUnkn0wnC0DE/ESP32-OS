/* ESP32-OS (W9): lädt eine Datei per HTTP vollständig herunter und
 * speichert sie – geschrieben gegen die KolibriOS-Netzwerk-API (libc.obj
 * <sys/socket.h>, network.obj <clayer/network.h>), wie ein KolibriOS-
 * Downloader. Ausgabe: Kopf, Inhaltslänge, Content-Length, Prüfsumme.
 * GPL-2.0 wie ESP32-OS. */
#include <clayer/network.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char* HOST = "kolibrios.org";
static const char* PFAD = "/en/";
static const char* ZIEL = "/rd/1/laden.html";

int main(void)
{
    struct addrinfo* ai = NULL;
    printf("netz_laden: http://%s%s -> %s\n", HOST, PFAD, ZIEL);
    if (getaddrinfo(HOST, "80", 0, &ai) != 0 || !ai) {
        printf("netz_laden: %s nicht gefunden\n", HOST);
        exit(1);
    }
    int s = socket(AF_INET4, SOCK_STREAM, IPPROTO_TCP);
    if (s == -1 || connect(s, ai->ai_addr, ai->ai_addrlen) != 0) {
        printf("netz_laden: Verbindung fehlgeschlagen, errno %d\n", errno);
        freeaddrinfo(ai);
        exit(1);
    }
    freeaddrinfo(ai);
    char anfrage[256];
    sprintf(anfrage, "GET %s HTTP/1.0\r\nHost: %s\r\nUser-Agent: ESP32-OS netz_laden\r\nConnection: close\r\n\r\n", PFAD, HOST);
    if (send(s, anfrage, strlen(anfrage), MSG_NOFLAG) == -1) {
        printf("netz_laden: Senden fehlgeschlagen, errno %d\n", errno);
        close(s);
        exit(1);
    }
    FILE* f = fopen(ZIEL, "wb");
    if (!f) {
        printf("netz_laden: %s nicht beschreibbar\n", ZIEL);
        close(s);
        exit(1);
    }
    /* Kopf bis zur Leerzeile sammeln, danach alles in die Datei */
    static char kopf[4096];
    int kopf_n = 0, im_inhalt = 0, empfaenge = 0;
    long inhalt = 0, laenge = -1;
    unsigned long summe = 0;
    char puffer[1024];
    int n;
    while ((n = recv(s, puffer, sizeof puffer, MSG_NOFLAG)) > 0) {
        empfaenge++;
        int i = 0;
        while (!im_inhalt && i < n) {
            if (kopf_n < (int)sizeof kopf - 1) kopf[kopf_n++] = puffer[i];
            i++;
            if (kopf_n >= 4 && memcmp(kopf + kopf_n - 4, "\r\n\r\n", 4) == 0) {
                kopf[kopf_n] = 0;
                im_inhalt = 1;
                char* cl = strstr(kopf, "Content-Length:");
                if (cl) laenge = atol(cl + 15);
            }
        }
        if (im_inhalt && i < n) {
            fwrite(puffer + i, 1, n - i, f);
            for (int k = i; k < n; k++) summe = summe * 31 + (unsigned char)puffer[k];
            inhalt += n - i;
        }
    }
    fclose(f);
    close(s);
    char* zeile_ende = strstr(kopf, "\r\n");
    if (zeile_ende) *zeile_ende = 0;
    printf("netz_laden: %s | %d Empfaenge, Inhalt %ld Byte, Content-Length %ld, Summe %08lx -> %s\n",
           kopf_n ? kopf : "(kein Kopf)", empfaenge, inhalt, laenge, summe & 0xFFFFFFFFul,
           (laenge >= 0 && inhalt == laenge) ? "VOLLSTAENDIG" : "UNVOLLSTAENDIG");
    exit(inhalt > 0 && (laenge < 0 || inhalt == laenge) ? 0 : 1);
}
