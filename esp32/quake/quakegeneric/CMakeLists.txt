# Quake (quakegeneric/WinQuake, GPL-2.0) als ESP-IDF-Komponente der
# Konsolen-Firmware. Die Rust-Seite (Crate `quake`) liefert die QG_*-Funktionen.
file(GLOB QUAKE_SRCS "*.c")
idf_component_register(SRCS ${QUAKE_SRCS}
                       INCLUDE_DIRS "."
                       REQUIRES vfs esp_partition
                       LDFRAGMENTS "linker.lf")
target_compile_options(${COMPONENT_LIB} PRIVATE
    -O3 -ffast-math -w -Wno-error -fno-strict-aliasing -fsingle-precision-constant
    -include ${CMAKE_CURRENT_SOURCE_DIR}/qnamen.h)
