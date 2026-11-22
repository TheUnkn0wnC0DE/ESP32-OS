/* ESP32-OS-Test D12: absichtlicher Zugriffsfehler (CPU-Ausnahme) in einem
 * C-Programm. Das Board startet neu und meldet im Desktop den Absturz. */
#include <sys/ksys.h>

int main()
{
    _ksys_debug_puts("absturz_test: schreibe auf Adresse 0\n");
    volatile int* p = (volatile int*)0;
    *p = 42;
    return 0;
}
