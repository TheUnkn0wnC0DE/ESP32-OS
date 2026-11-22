/* ESP32-OS, B1: Alle beschreibbaren statischen Daten der NetSurf-
 * Bibliotheken in einen eigenen Abschnitt (Neustart, c/programm_daten.c).
 * PC (clang-cl/MSVC): .nsd$m/.nsb$m zwischen den Marken $a und $z.
 * Board (GCC): ein Linker-Fragment ordnet die Bibliothek zu, hier nichts. */
#ifndef NS_ABSCHNITT_H
#define NS_ABSCHNITT_H
#if defined(_MSC_VER)
#pragma data_seg(".nsd$m")
#pragma bss_seg(".nsb$m")
/* clang-cl legte konstante Daten sonst als schreibgeschützten zweiten
 * Abschnitt gleichen Namens ab (.nsd zweimal, Marken im falschen Teil) */
#pragma const_seg(".nsr$m")
/* LLVM legt nie beschriebene statische Variablen als Konstanten ab; die
 * erste Variable eines Abschnitts bestimmt dessen Rechte. Damit .nsd$m und
 * .nsb$m in jeder Übersetzungseinheit beschreibbar angelegt werden, kommt
 * zuerst je eine beschreibbare Marke (2 Byte je Datei). */
__attribute__((used)) static volatile char ns_abschnitt_daten = 1;
__attribute__((used)) static volatile char ns_abschnitt_bss;
#endif
#endif
