/* ESP32-OS: KolibriOS-libc-Aufsatz für C-Programme (wird jedem Programm mit
 * -include vorangestellt).
 *  - Erweiterungen der KolibriOS-libc (libc.obj): debug_printf.
 *  - DLLAPI/__stdcall der clayer-Header (box_lib, libimg, proc_lib, libini
 *    sind nativ eingelinkt, keine DLLs).
 *  - Datei-Ein/Ausgabe über die KolibriOS-Dateifunktionen (Funktion 70,
 *    KolibriOS-Pfade, aktueller Ordner) statt über das Host-Dateisystem.
 *  - malloc & Co. aus dem Heap des Programms (Funktion 68), damit beim
 *    Beenden alles frei wird.
 *  - exit() beendet nur das Programm (Funktion -1), nicht das System.
 *  - Sockets (libc.obj sys/socket.h, Funktion 75) und network.obj
 *    (getaddrinfo, inet_ntoa, …) – W9; Namen mit kos_-Präfix, damit sie
 *    nicht mit lwIP/der Host-libc zusammenstoßen. */
#ifndef KOLIBRI_LIBC_H
#define KOLIBRI_LIBC_H
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef DLLAPI
#define DLLAPI extern
#endif
#if !defined(__i386__) && !defined(_M_IX86) && !defined(__stdcall)
#define __stdcall
#endif

void debug_printf(const char* fmt, ...);

FILE* kos_fopen(const char* pfad, const char* modus);
int kos_fclose(FILE* f);
size_t kos_fread(void* p, size_t g, size_t n, FILE* f);
size_t kos_fwrite(const void* p, size_t g, size_t n, FILE* f);
int kos_fseek(FILE* f, long o, int woher);
long kos_ftell(FILE* f);
void kos_rewind(FILE* f);
int kos_feof(FILE* f);
int kos_ferror(FILE* f);
void kos_clearerr(FILE* f);
int kos_fgetc(FILE* f);
char* kos_fgets(char* s, int n, FILE* f);
int kos_fputc(int c, FILE* f);
int kos_fputs(const char* s, FILE* f);
int kos_fprintf(FILE* f, const char* fmt, ...);
int kos_vfprintf(FILE* f, const char* fmt, va_list ap);
int kos_fflush(FILE* f);
/* stdout/stderr → Debug-Board (Funktion 63.1), siehe libc_zusatz.c */
int kos_printf(const char* fmt, ...);
int kos_vprintf(const char* fmt, va_list ap);
int kos_puts(const char* s);
/* libc.obj: itoa(Zahl, Puffer) dezimal, zwei Argumente (Shell, C1) */
char* kos_itoa(int n, char* s);
int kos_putchar(int c);
int kos_remove(const char* pfad);
int kos_rename(const char* alt, const char* neu);
struct sockaddr;
struct addrinfo;
void kos_exit(int status);
int kos_socket(int domain, int type, int protocol);
int kos_close(int socket);
int kos_bind(int socket, const struct sockaddr* adresse, int laenge);
int kos_listen(int socket, int rueckstau);
int kos_connect(int socket, const struct sockaddr* adresse, int laenge);
int kos_accept(int socket, const struct sockaddr* adresse, int laenge);
int kos_send(int socket, const void* daten, size_t laenge, int flags);
int kos_recv(int socket, void* puffer, size_t laenge, int flags);
int kos_socketpair(int* s1, int* s2);
/* setsockopt/getsockopt: Prototypen aus <sys/socket.h> (optstruct) */
/* network.obj (nativ in Rust, kolibri::netzwerk; __stdcall wie im Original) */
int __stdcall kos_inet_addr(const char* name);
char* __stdcall kos_inet_ntoa(int ip);
int __stdcall kos_getaddrinfo(const char* name, const char* dienst, const struct addrinfo* hinweise, struct addrinfo** erg);
void __stdcall kos_freeaddrinfo(struct addrinfo* ai);
void* kos_malloc(size_t n);
void* kos_calloc(size_t n, size_t g);
void* kos_realloc(void* p, size_t n);
void kos_free(void* p);

#ifndef KOLIBRI_LIBC_INTERN
#undef getc
#undef putc
#undef putchar
#define printf kos_printf
#define vprintf kos_vprintf
#define puts kos_puts
#define itoa kos_itoa
#define putchar kos_putchar
#define fopen kos_fopen
#define fclose kos_fclose
#define fread kos_fread
#define fwrite kos_fwrite
#define fseek kos_fseek
#define ftell kos_ftell
#define rewind kos_rewind
#define feof kos_feof
#define ferror kos_ferror
#define clearerr kos_clearerr
#define fgetc kos_fgetc
#define getc kos_fgetc
#define fgets kos_fgets
#define fputc kos_fputc
#define putc kos_fputc
#define fputs kos_fputs
#define fprintf kos_fprintf
#define vfprintf kos_vfprintf
#define fflush kos_fflush
#define remove kos_remove
#define rename kos_rename
#define malloc kos_malloc
#define calloc kos_calloc
#define realloc kos_realloc
#define free kos_free
#define exit kos_exit
#define socket kos_socket
#define close kos_close
#define bind kos_bind
#define listen kos_listen
#define connect kos_connect
#define accept kos_accept
#define send kos_send
#define recv kos_recv
#define setsockopt kos_setsockopt
#define getsockopt kos_getsockopt
#define socketpair kos_socketpair
#define inet_addr kos_inet_addr
#define inet_ntoa kos_inet_ntoa
#define getaddrinfo kos_getaddrinfo
#define freeaddrinfo kos_freeaddrinfo
#endif
#endif
