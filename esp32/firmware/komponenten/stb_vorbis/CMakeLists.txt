# D2: Ogg-Vorbis-Dekoder stb_vorbis v1.25 (Sean Barrett, MIT oder Public
# Domain; Quelle unverändert in esp32/desktop/c/stb_vorbis.c, am PC baut ihn
# desktop/build.rs). Nur die Push-Schnittstelle, ohne stdio; NDEBUG: eine
# kaputte Datei darf kein assert/abort auslösen.
idf_component_register(SRCS "${CMAKE_CURRENT_LIST_DIR}/../../../desktop/c/stb_vorbis.c")
target_compile_definitions(${COMPONENT_LIB} PRIVATE STB_VORBIS_NO_STDIO STB_VORBIS_NO_PULLDATA_API NDEBUG)
target_compile_options(${COMPONENT_LIB} PRIVATE -O2 -w)
