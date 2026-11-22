//
// Anbindung von quakegeneric an die Spielekonsole (Pudding-Panik-Projekt).
// Copyright(C) 2026 – GPL-2.0-or-later wie der übrige Quake-Code hier.
//
// * Ton: der WinQuake-Mischer schreibt in einen Ringpuffer der Rust-Seite,
//   den der Ton-Kern abspielt (SNDDMA_*).
// * Befehle (Konsole) und Zustand (Menü/Spiel) für die Touch-Steuerung.
//

#include "quakedef.h"

// --- in Rust umgesetzt ------------------------------------------------------

extern short *QG_TonPuffer(int *frames);
extern int QG_TonPosition(void);

// --- Ton --------------------------------------------------------------------

static dma_t qg_dma;

qboolean SNDDMA_Init(void)
{
    int frames;
    short *puffer = QG_TonPuffer(&frames);

    if (puffer == NULL)
    {
        return false;
    }
    memset((void *) &qg_dma, 0, sizeof(qg_dma));
    shm = &qg_dma;
    shm->splitbuffer = 0;
    shm->samplebits = 16;
    shm->speed = 22050;
    shm->channels = 2;
    shm->samples = frames * 2;
    shm->samplepos = 0;
    shm->submission_chunk = 1;
    shm->buffer = (unsigned char *) puffer;
    return true;
}

int SNDDMA_GetDMAPos(void)
{
    shm->samplepos = (QG_TonPosition() * shm->channels) % shm->samples;
    return shm->samplepos;
}

void SNDDMA_Shutdown(void)
{
}

void SNDDMA_Submit(void)
{
}

// --- Befehle und Zustand -------------------------------------------------------

void QG_Befehl(const char *text)
{
    Cbuf_AddText((char *) text);
}

// 0 = Spiel, 1 = Konsole, 2 = Nachricht tippen, 3 = Menü (key_dest);
// +16, wenn gerade eine Demo läuft (Titel/Vorführung).
int QG_Modus(void)
{
    return (int) key_dest + (cls.demoplayback ? 16 : 0);
}

// --- Messwerte (Autopilot) ------------------------------------------------------

extern int hunk_size, hunk_low_used, hunk_high_used;

// Freier Platz zwischen unterem und oberem Hunk-Ende in Bytes (dort liegt
// auch der verdrängbare Cache).
int QG_HunkFrei(void)
{
    return hunk_size - hunk_low_used - hunk_high_used;
}

// Name der laufenden Karte ("" ohne laufenden Server).
const char *QG_Karte(void)
{
    return sv.active ? sv.name : "";
}
