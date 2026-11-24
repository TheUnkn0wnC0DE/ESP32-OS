/* ESP32-OS, C1: <conio.h> der KolibriOS-libc (libc.obj) – Schnittstelle von
 * console.obj (programs/develop/libraries/console_coff/console_en.txt).
 * Die Funktionszeiger zeigen auf die native Konsole (kolibri::konsole, Rust);
 * definiert in c/libc_zusatz.c. Gleiche Namen, Typen und Aufrufkonvention
 * wie im Original (programs/develop/ktcc/libc.obj/include/conio.h). */
#ifndef _CONIO_H_
#define _CONIO_H_

#include <stddef.h>

#if !defined(__i386__) && !defined(_M_IX86) && !defined(__stdcall)
#define __stdcall
#endif
#if !defined(__i386__) && !defined(_M_IX86) && !defined(__cdecl)
#define __cdecl
#endif

typedef unsigned int dword;  /* 32-bit unsigned integer */
typedef unsigned short word; /* 16-bit unsigned integer */

#define CON_WINDOW_CLOSED 0x200
#define CON_COLOR_BLUE    0x01
#define CON_COLOR_GREEN   0x02
#define CON_COLOR_RED     0x04
#define CON_COLOR_BRIGHT  0x08
/* background color */
#define CON_BGR_BLUE   0x10
#define CON_BGR_GREEN  0x20
#define CON_BGR_RED    0x40
#define CON_BGR_BRIGHT 0x80
/* output controls */
#define CON_IGNORE_SPECIALS 0x100

int con_init(void);
int con_init_opt(dword wnd_width, dword wnd_height, dword scr_width, dword scr_height, const char* title);
extern void(__stdcall* con_exit)(int bCloseWindow);
extern void(__stdcall* con_set_title)(const char* title);
extern void(__stdcall* con_write_asciiz)(const char* str);
extern void(__stdcall* con_write_string)(const char* str, dword length);
extern int(__cdecl* con_printf)(const char* format, ...);
extern dword(__stdcall* con_get_flags)(void);
extern dword(__stdcall* con_set_flags)(dword new_flags);
extern int(__stdcall* con_get_font_height)(void);
extern int(__stdcall* con_get_cursor_height)(void);
extern int(__stdcall* con_set_cursor_height)(int new_height);
extern int(__stdcall* con_getch)(void);
extern word(__stdcall* con_getch2)(void);
extern int(__stdcall* con_kbhit)(void);
extern char*(__stdcall* con_gets)(char* str, int n);
typedef int(__stdcall* con_gets2_callback)(int keycode, char** pstr, int* pn, int* ppos);
extern char*(__stdcall* con_gets2)(con_gets2_callback callback, char* str, int n);
extern void(__stdcall* con_cls)(void);
extern void(__stdcall* con_get_cursor_pos)(int* px, int* py);
extern void(__stdcall* con_set_cursor_pos)(int x, int y);
extern int(__stdcall* con_get_input)(char* buf, int buflen);
extern int __con_is_load;

#endif
