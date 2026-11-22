# Doom (doomgeneric, GPL-2.0) als ESP-IDF-Komponente der Konsolen-Firmware.
# Die Rust-Seite (Crate `doom`) liefert die DG_*-Funktionen.
file(GLOB DOOM_SRCS "*.c")
idf_component_register(SRCS ${DOOM_SRCS}
                       INCLUDE_DIRS "."
                       REQUIRES esp_partition fatfs
                       LDFRAGMENTS "linker.lf")
target_compile_definitions(${COMPONENT_LIB} PRIVATE
    CMAP256 DOOMGENERIC_RESX=320 DOOMGENERIC_RESY=200 NORMALUNIX)
target_compile_options(${COMPONENT_LIB} PRIVATE
    -O2 -w -Wno-error -fno-strict-aliasing)
