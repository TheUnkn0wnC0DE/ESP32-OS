/* ESP32-OS-Test D12/D5: absichtlicher Hardware-Fehler (CPU-Ausnahme) in
 * einem C-Programm. Seit D5 endet nur dieses Programm, das Board läuft
 * weiter; der Desktop meldet den Absturz.
 *
 * Parameter: (keiner) = Schreiben auf Adresse 0 (Store access fault),
 * „lesen“ = Lesen von Adresse 4 (Load access fault), „befehl“ = unerlaubter
 * Befehl (Illegal instruction). */
#include <string.h>
#include <sys/ksys.h>

int main(int argc, char** argv)
{
    const char* art = argc > 1 ? argv[1] : "";
    if (strcmp(art, "lesen") == 0) {
        _ksys_debug_puts("absturz_test: lese von Adresse 4\n");
        volatile int* p = (volatile int*)4;
        return *p;
    }
    if (strcmp(art, "befehl") == 0) {
        _ksys_debug_puts("absturz_test: unerlaubter Befehl\n");
#if defined(__riscv)
        __asm__ volatile("unimp");
#else
        __builtin_trap();
#endif
        return 1;
    }
    _ksys_debug_puts("absturz_test: schreibe auf Adresse 0\n");
    volatile int* p = (volatile int*)0;
    *p = 42;
    return 0;
}
