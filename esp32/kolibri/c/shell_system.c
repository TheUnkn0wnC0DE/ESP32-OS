/* ESP32-OS, C1: Systemschicht der KolibriOS-Shell (programs/system/shell/
 * system/kolibri.c) nativ – dieselben Funktionen, die Systemaufrufe gehen
 * statt über "int $0x40" an kos_syscall() im Kern. Nur die Funktionen, die
 * die Shell benutzt; Nummern und Register wie im Original. */
#include <stdint.h>
#include <string.h>
#include <sys/ksys.h>
#include "shell/system/kolibri.h"

static uint32_t aufruf(uint32_t eax, uint32_t ebx, uint32_t ecx, uint32_t edx, uint32_t esi)
{
    ksys_regs_t r = { 0 };
    r.eax = eax;
    r.ebx = ebx;
    r.ecx = ecx;
    r.edx = edx;
    r.esi = esi;
    kos_syscall(&r);
    return r.eax;
}

#define Z(p) ((uint32_t)(uintptr_t)(p))

void kol_exit(void)
{
    aufruf((uint32_t)-1, 0, 0, 0, 0);
}

void kol_sleep(unsigned d)
{
    aufruf(5, d, 0, 0, 0);
}

unsigned kol_key_control(void)
{
    return aufruf(66, 3, 0, 0, 0);
}

int kol_file_70(kol_struct70* k)
{
    return (int)aufruf(70, Z(k), 0, 0, 0);
}

unsigned kol_system_cpufreq(void)
{
    return aufruf(18, 5, 0, 0, 0);
}

unsigned kol_system_mem(void)
{
    return aufruf(18, 17, 0, 0, 0);
}

unsigned kol_system_memfree(void)
{
    return aufruf(18, 16, 0, 0, 0);
}

unsigned kol_system_time_get(void)
{
    return aufruf(3, 0, 0, 0, 0);
}

unsigned kol_system_date_get(void)
{
    return aufruf(29, 0, 0, 0, 0);
}

unsigned kol_system_end(unsigned param)
{
    return aufruf(18, 9, param, 0, 0);
}

unsigned kol_time_tick(void)
{
    return aufruf(26, 9, 0, 0, 0);
}

unsigned kol_process_info(unsigned slot, char buf1k[])
{
    return aufruf(9, Z(buf1k), slot, 0, 0);
}

int kol_process_kill_pid(unsigned process)
{
    return (int)aufruf(18, 18, process, 0, 0);
}

int kol_kill_process(unsigned process)
{
    return (int)aufruf(18, 2, process, 0, 0);
}

void kol_get_kernel_ver(struct kernel_version* kv)
{
    aufruf(18, 13, Z(kv), 0, 0);
}

int kol_buffer_open(char name[], int mode, int size, char** buf)
{
    ksys_regs_t r = { 0 };
    r.eax = 68;
    r.ebx = 22;
    r.ecx = Z(name);
    r.edx = (uint32_t)size;
    r.esi = (uint32_t)mode;
    kos_syscall(&r);
    *buf = (char*)(uintptr_t)r.eax;
    return (int)r.edx;
}

void kol_buffer_close(char name[])
{
    aufruf(68, 23, Z(name), 0, 0);
}

/* set_cwd/getcwd der Shell (Funktion 30.1/30.2); in shell_haupt.c auf
 * diese Namen umgelenkt, damit sie nicht mit getcwd der C-Laufzeit kollidieren */
void shell_set_cwd(const char* cwd)
{
    aufruf(30, 1, Z(cwd), 0, 0);
}

int shell_getcwd(char* buf, unsigned size)
{
    return (int)aufruf(30, 2, Z(buf), size, 0);
}
