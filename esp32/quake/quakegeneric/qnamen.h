// Konsole: Doom und Quake liegen im selben Programm; gleichnamige globale
// Funktionen und Variablen bekommen in Quake einen Vorsatz (vor jeder
// Quake-Quelldatei eingebunden, siehe build.rs und CMakeLists.txt).
#ifndef QNAMEN_H
#define QNAMEN_H

// Konsole: eigene Tonausgabe statt DirectSound (snd_*.c).
#define QG_KONSOLE 1

#define M_Init q_M_Init
#define R_DrawSprite q_R_DrawSprite
#define R_Init q_R_Init
#define R_InitTextures q_R_InitTextures
#define R_SetupFrame q_R_SetupFrame
#define V_Init q_V_Init
#define WritePCXfile q_WritePCXfile
#define Z_CheckHeap q_Z_CheckHeap
#define Z_ClearZone q_Z_ClearZone
#define Z_Free q_Z_Free
#define Z_Malloc q_Z_Malloc
#define deathmatch q_deathmatch
#define mainzone q_mainzone
#define nomonsters q_nomonsters
#define onground q_onground
#define startepisode q_startepisode
#define timelimit q_timelimit
#define gammatable q_gammatable
#define precache q_precache
#define S_StopSound q_S_StopSound
#define S_StartSound q_S_StartSound
#define S_Shutdown q_S_Shutdown
#define S_Init q_S_Init

#endif
