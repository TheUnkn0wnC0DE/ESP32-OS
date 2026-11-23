# ESP32-OS: KolibriOS-C-Programme (unverändert, GPL-2.0) für das Board.
# Am PC baut sie kolibri/build.rs; Systemaufrufe gehen an kos_syscall (Rust).
set(Q "${CMAKE_CURRENT_LIST_DIR}/../c")
file(GLOB PROGS "${Q}/gen/*.c")
# libcryptal (Brad Conte, gemeinfrei, unverändert) für TinyHashView
set(CRYPTAL "${Q}/cryptal/md5.c" "${Q}/cryptal/sha1.c" "${Q}/cryptal/sha256.c")
idf_component_register(SRCS "${Q}/libc_zusatz.c" ${PROGS} ${CRYPTAL}
                       INCLUDE_DIRS "../include")
target_compile_options(${COMPONENT_LIB} PRIVATE
    -O2 -w -Wno-int-conversion -Wno-incompatible-pointer-types)
set_source_files_properties("${Q}/libc_zusatz.c" PROPERTIES COMPILE_DEFINITIONS "KOLIBRI_LIBC_INTERN")
set_source_files_properties(${PROGS} PROPERTIES COMPILE_OPTIONS "-include;${CMAKE_CURRENT_LIST_DIR}/../include/kolibri_libc.h")
set_source_files_properties(${CRYPTAL} PROPERTIES COMPILE_OPTIONS "-I${Q}/cryptal")
