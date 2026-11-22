//
// Anbindung von doomgeneric an die Spielekonsole (Pudding-Panik-Projekt).
// Copyright(C) 2026 – GPL-2.0-or-later wie der übrige Doom-Code hier.
//
// * WAD aus dem Speicher: Die Firmware kopiert DOOM1.WAD aus der Flash-
//   Partition in den PSRAM; Doom liest die Lumps direkt von dort (`mapped`).
// * Soundeffekte: DMX-Samples (8 Bit, meist 11025 Hz) gehen unverändert an
//   den Mischer in Rust (läuft auf dem Ton-Kern).
// * Musik: MUS-Daten gehen an den Musik-Spieler in Rust.
// * Maus/Ereignisse: Rust meldet Drehen und Laufen als Maus-Ereignis.
//

#include <stdlib.h>
#include <string.h>

#include "d_event.h"
#include "doomtype.h"
#include "i_sound.h"
#include "w_file.h"
#include "w_wad.h"
#include "z_zone.h"

// --- in Rust umgesetzt ------------------------------------------------------

extern int DG_WadDaten(const char *pfad, const unsigned char **daten, unsigned int *laenge);
extern void DG_TonStart(int kanal, const unsigned char *daten, unsigned int laenge, int rate, int vol, int sep);
extern void DG_TonStopp(int kanal);
extern int DG_TonLaeuft(int kanal);
extern void DG_TonParameter(int kanal, int vol, int sep);
extern void DG_MusikStart(const unsigned char *daten, unsigned int laenge, int schleife);
extern void DG_MusikStopp(void);
extern int DG_MusikLaeuft(void);
extern void DG_MusikLautstaerke(int vol);
extern void DG_MusikPause(int pause);

// Von i_sound.c erwartet (sonst in i_sdlsound.c), hier ohne Bedeutung.
int use_libsamplerate = 0;
float libsamplerate_scale = 0.65f;

// --- WAD aus dem Speicher -----------------------------------------------------

extern wad_file_class_t mem_wad_file;

static wad_file_t *W_Mem_OpenFile(char *path)
{
    const unsigned char *daten;
    unsigned int laenge;
    wad_file_t *result;

    if (!DG_WadDaten(path, &daten, &laenge))
    {
        return NULL;
    }
    result = Z_Malloc(sizeof(wad_file_t), PU_STATIC, 0);
    result->file_class = &mem_wad_file;
    result->mapped = (byte *) daten;
    result->length = laenge;
    return result;
}

static void W_Mem_CloseFile(wad_file_t *wad)
{
    Z_Free(wad);
}

static size_t W_Mem_Read(wad_file_t *wad, unsigned int offset, void *buffer, size_t buffer_len)
{
    if (offset >= wad->length)
    {
        return 0;
    }
    if (buffer_len > wad->length - offset)
    {
        buffer_len = wad->length - offset;
    }
    memcpy(buffer, wad->mapped + offset, buffer_len);
    return buffer_len;
}

wad_file_class_t mem_wad_file =
{
    W_Mem_OpenFile,
    W_Mem_CloseFile,
    W_Mem_Read,
};

boolean DG_MemDateiExistiert(char *pfad)
{
    const unsigned char *daten;
    unsigned int laenge;
    return DG_WadDaten(pfad, &daten, &laenge) != 0;
}

// --- Maus und Steuerknüppel ---------------------------------------------------

// Drehen (dx, Maus-Einheiten), Laufen (dy, + = vorwärts).
void DG_Maus(int dx, int dy)
{
    event_t ev;
    ev.type = ev_mouse;
    ev.data1 = 0;
    ev.data2 = dx;
    ev.data3 = dy;
    ev.data4 = 0;
    D_PostEvent(&ev);
}

// --- Soundeffekte -------------------------------------------------------------

static snddevice_t sound_devices[] =
{
    SNDDEVICE_SB,
    SNDDEVICE_PAS,
    SNDDEVICE_GUS,
    SNDDEVICE_WAVEBLASTER,
    SNDDEVICE_SOUNDCANVAS,
    SNDDEVICE_AWE32,
};

static boolean use_prefix;

static boolean K_InitSound(boolean use_sfx_prefix)
{
    use_prefix = use_sfx_prefix;
    return true;
}

static void K_ShutdownSound(void)
{
}

static int K_GetSfxLumpNum(sfxinfo_t *sfx)
{
    char name[16];

    if (sfx->link != NULL)
    {
        sfx = sfx->link;
    }
    if (use_prefix)
    {
        name[0] = 'd';
        name[1] = 's';
        strncpy(name + 2, sfx->name, 8);
    }
    else
    {
        strncpy(name, sfx->name, 8);
    }
    name[10] = '\0';
    return W_CheckNumForName(name);
}

static void K_UpdateSound(void)
{
}

static void K_UpdateSoundParams(int channel, int vol, int sep)
{
    DG_TonParameter(channel, vol, sep);
}

static int K_StartSound(sfxinfo_t *sfx, int channel, int vol, int sep)
{
    int lump = sfx->lumpnum;
    const byte *data;
    unsigned int lumplen, length;
    int rate;

    if (lump < 0)
    {
        return -1;
    }
    data = W_CacheLumpNum(lump, PU_STATIC);
    lumplen = W_LumpLength(lump);
    // DMX-Kopf: 3, 0, Abtastrate (16 Bit), Länge (32 Bit); DMX überspringt
    // die ersten und letzten 16 Samples.
    if (lumplen < 8 || data[0] != 0x03 || data[1] != 0x00)
    {
        return -1;
    }
    rate = (data[3] << 8) | data[2];
    length = ((unsigned int) data[7] << 24) | (data[6] << 16) | (data[5] << 8) | data[4];
    if (length > lumplen - 8 || length <= 48)
    {
        return -1;
    }
    DG_TonStart(channel, data + 8 + 16, length - 32, rate, vol, sep);
    return channel;
}

static void K_StopSound(int channel)
{
    DG_TonStopp(channel);
}

static boolean K_SoundIsPlaying(int channel)
{
    return DG_TonLaeuft(channel) != 0;
}

static void K_CacheSounds(sfxinfo_t *sounds, int num_sounds)
{
}

sound_module_t DG_sound_module =
{
    sound_devices,
    arrlen(sound_devices),
    K_InitSound,
    K_ShutdownSound,
    K_GetSfxLumpNum,
    K_UpdateSound,
    K_UpdateSoundParams,
    K_StartSound,
    K_StopSound,
    K_SoundIsPlaying,
    K_CacheSounds,
};

// --- Musik --------------------------------------------------------------------

static snddevice_t music_devices[] =
{
    SNDDEVICE_PAS,
    SNDDEVICE_GUS,
    SNDDEVICE_WAVEBLASTER,
    SNDDEVICE_SOUNDCANVAS,
    SNDDEVICE_GENMIDI,
    SNDDEVICE_AWE32,
    SNDDEVICE_SB,
    SNDDEVICE_ADLIB,
};

typedef struct
{
    const unsigned char *data;
    int len;
} song_t;

static boolean K_InitMusic(void)
{
    return true;
}

static void K_ShutdownMusic(void)
{
    DG_MusikStopp();
}

static void K_SetMusicVolume(int volume)
{
    DG_MusikLautstaerke(volume);
}

static void K_PauseMusic(void)
{
    DG_MusikPause(1);
}

static void K_ResumeMusic(void)
{
    DG_MusikPause(0);
}

static void *K_RegisterSong(void *data, int len)
{
    song_t *s = malloc(sizeof(song_t));
    if (s != NULL)
    {
        s->data = data;
        s->len = len;
    }
    return s;
}

static void K_UnRegisterSong(void *handle)
{
    free(handle);
}

static void K_PlaySong(void *handle, boolean looping)
{
    song_t *s = handle;
    if (s != NULL)
    {
        DG_MusikStart(s->data, (unsigned int) s->len, looping);
    }
}

static void K_StopSong(void)
{
    DG_MusikStopp();
}

static boolean K_MusicIsPlaying(void)
{
    return DG_MusikLaeuft() != 0;
}

static void K_PollMusic(void)
{
}

music_module_t DG_music_module =
{
    music_devices,
    arrlen(music_devices),
    K_InitMusic,
    K_ShutdownMusic,
    K_SetMusicVolume,
    K_PauseMusic,
    K_ResumeMusic,
    K_RegisterSong,
    K_UnRegisterSong,
    K_PlaySong,
    K_StopSong,
    K_MusicIsPlaying,
    K_PollMusic,
};

// --- Zustand für die Touch-Steuerung -------------------------------------------

#include "doomkeys.h"
#include "doomstat.h"
#include "m_controls.h"
#include "p_saveg.h"

// Spiegel der Menü-Strukturen aus m_menu.c (dort nicht im Header).
typedef struct
{
    short status;
    char name[10];
    void (*routine)(int choice);
    char alphaKey;
} k_menuitem_t;

typedef struct k_menu_s
{
    short numitems;
    struct k_menu_s *prevMenu;
    k_menuitem_t *menuitems;
    void (*routine)(void);
    short x;
    short y;
    short lastOn;
} k_menu_t;

extern k_menu_t *currentMenu;
extern short itemOn;
extern boolean menuactive;
extern int messageToPrint;
extern boolean messageNeedsInput;
extern int saveStringEnter;
extern int saveSlot;
extern char savegamestrings[10][SAVESTRINGSIZE];
extern int mouseSensitivity;

#define K_LINEHEIGHT 16

// 0 = im Level, 1 = Menü, 2 = Meldung (beliebige Taste), 3 = Ja/Nein-Frage,
// 4 = Spielstand-Name wird eingegeben, 5 = Titel/Demo/Zwischenbild/Abspann.
int DG_Modus(void)
{
    if (messageToPrint)
    {
        return messageNeedsInput ? 3 : 2;
    }
    if (saveStringEnter)
    {
        return 4;
    }
    if (menuactive)
    {
        return 1;
    }
    if (gamestate != GS_LEVEL || demoplayback)
    {
        return 5;
    }
    return 0;
}

// Tipp ins Menü bei (x, y) in Doom-Koordinaten (320×200): wählt den Eintrag.
// Rückgabe: 1 = bestätigen (Enter), 2 = Schieber kleiner, 3 = Schieber
// größer, 0 = nur ausgewählt bzw. nichts getroffen.
int DG_MenueTipp(int x, int y)
{
    int i;
    k_menu_t *m = currentMenu;

    if (!menuactive || m == NULL || y < m->y)
    {
        return 0;
    }
    i = (y - m->y) / K_LINEHEIGHT;
    if (i >= m->numitems)
    {
        return 0;
    }
    if (m->menuitems[i].status == -1)
    {
        // Zeile unter einem Schieber: die Skala selbst.
        if (i > 0 && m->menuitems[i - 1].status == 2)
        {
            itemOn = i - 1;
            return x < m->x + 64 ? 2 : 3;
        }
        return 0;
    }
    itemOn = i;
    return m->menuitems[i].status == 2 ? 0 : 1;
}

// Spielstand wird benannt und der Name ist noch leer?
int DG_SpeicherNameLeer(void)
{
    return saveStringEnter && savegamestrings[saveSlot][0] == '\0';
}

// Episode und Karte (für den Namen eines Spielstands).
int DG_Level(void)
{
    return gameepisode * 10 + gamemap;
}

// Nach dem Laden der Konfiguration: Tasten und Laufen für den Touchscreen.
void DG_Einstellen(void)
{
    key_nextweapon = KEY_INS;
    key_prevweapon = KEY_DEL;
    // "joyb_speed = 31"-Trick: immer rennen.
    joybspeed = 29;
}
