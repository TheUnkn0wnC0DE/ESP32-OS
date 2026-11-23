# ESP32-OS, B1: NetSurf 3.11 mit QuickJS-ng und der ESP32-OS-Anbindung fürs
# Board (Programm „browser“, Rust-Einstieg netsurf::programm).
#
# Die Quellgruppen (Dateien, Pfade, Defines je Bibliothek) schreibt der
# PC-Bau (esp32/netsurf/build.rs) nach <NetSurf-Quellen>/erzeugt/
# netsurf_quellen.cmake – so übersetzt das Board genau dieselben Dateien wie
# die PC-Tests. Vor dem ersten Board-Bau also einmal `cargo test -p netsurf`.

# Erster Durchlauf von ESP-IDF (nur Abhängigkeiten, Skript-Modus: dort ist
# set_source_files_properties nicht erlaubt)
if(CMAKE_BUILD_EARLY_EXPANSION)
    idf_component_register(REQUIRES kolibri_c heap esp_psram lwip esp_http_client esp-tls mbedtls esp_timer)
    return()
endif()

if(DEFINED ENV{NETSURF_QUELLEN})
    set(NS_Q "$ENV{NETSURF_QUELLEN}")
else()
    set(NS_Q "C:/ns/3.11")
endif()
set(NS_LISTE "${NS_Q}/erzeugt/netsurf_quellen.cmake")
if(NOT EXISTS "${NS_LISTE}")
    message(FATAL_ERROR "NetSurf: ${NS_LISTE} fehlt – zuerst am PC 'cargo test -p netsurf' (schreibt die Liste)")
endif()
include("${NS_LISTE}")

# Sicherung der statischen Daten (Neustart): ohne ns_programm.h und
# ausserhalb der umrahmten Bereiche (netsurf.lf)
set(NS_DATEN "${CMAKE_CURRENT_LIST_DIR}/../../../netsurf/c/programm_daten.c")

idf_component_register(SRCS ${NS_QUELLEN} "${NS_DATEN}"
                       LDFRAGMENTS "netsurf.lf"
                       REQUIRES kolibri_c heap esp_psram lwip esp_http_client esp-tls mbedtls esp_timer)

# -msmall-data-limit=0: keine .sdata/.sbss (lägen sonst ausserhalb der
# umrahmten Bereiche); -fno-common: alles in .bss
# Die Quellenliste nennt absolute Pfade des PC-Baus; __FILE__ (Asserts)
# ohne Benutzerverzeichnis ins Abbild
file(TO_CMAKE_PATH "$ENV{USERPROFILE}" NS_HEIM)
if(NS_HEIM)
    target_compile_options(${COMPONENT_LIB} PRIVATE "-ffile-prefix-map=${NS_HEIM}=~")
endif()
target_compile_options(${COMPONENT_LIB} PRIVATE
    -O2 -w -fno-common -msmall-data-limit=0
    -Wno-int-conversion -Wno-incompatible-pointer-types -Wno-implicit-function-declaration)
