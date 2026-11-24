/* ESP32-OS (C1): geändert (wie cmd_ver.c). Das Original fragt die
 * Treiberliste mit Inline-Assembler ("int $0x40", Funktion 68.31) ab; hier
 * dieselben Aufrufe über kos_syscall. ESP32-OS hat keine ladbaren Treiber:
 * Funktion 68.31 gibt es im Kern nicht (unbekannte 68.x lassen ebx stehen)
 * – dann meldet lsmod das, statt eine leere Liste falsch zu durchlaufen. */
struct service_str{
		char  name[16];
		struct service_str* fd;
		struct service_str* bk;
		uint32_t base; // base addreass service for kernel_free
		uint32_t entry; //unsigned drvEntry(int action, char *cmdline)
		uint32_t srv_proc; // static int __stdcall service_proc(ioctl_t *my_ctl)
		//void* srv_proc_kern;
	};

int cmd_lsmod(char param[]) {
    struct service_str* fd;
	struct service_str* bk;
	struct service_str srv_str;
	ksys_regs_t r = { 0 };
	r.eax = 68; r.ebx = 31; r.ecx = 1;
	kos_syscall(&r);
	if (r.ebx == 31 || r.ebx == 0 || r.eax == (uint32_t)-1) {
		printf("No loadable drivers (ESP32-OS)\n\r");
		return TRUE;
	}
	fd = (struct service_str*)(uintptr_t)r.ebx;
	bk = (struct service_str*)(uintptr_t)r.ecx;
	srv_str.fd = fd;
	printf("%-*s Hendler      Service_proc\n\r", 16, "  Name");
	while (1) {
		ksys_regs_t q = { 0 };
		q.eax = 68; q.ebx = 31; q.ecx = 2; q.edx = (uint32_t)(uintptr_t)fd; q.edi = (uint32_t)(uintptr_t)&srv_str;
		kos_syscall(&q);
		printf("%-*s 0x%X   0x%X \n", 16, srv_str.name, fd, srv_str.srv_proc);
		if (fd == bk) break;
		fd = srv_str.fd;
	};

    return TRUE;
}
