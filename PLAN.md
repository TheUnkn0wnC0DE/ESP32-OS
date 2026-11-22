# PLAN – ESP32-OS

Repo: dieser Ordner (Repo-Wurzel), Branch `esp32-os`.
Reihenfolge nach Auftrag Abschnitt 8. Jeder Punkt: **fertig, wenn …** /
**Nachweis**. Abhaken nur mit Nachweis-Zeile. Sim = PC-Simulator `esp32/sim`,
Foto = `bildschirmfoto.py` (Board) bzw. PNG aus Sim, Autopilot = eingespeiste
Touch-Ereignisse.

## 0. Vorgezogen (Auftrag 13, Reihenfolge)

Zuerst V1 und V2, dann Block E (Browser, Auftrag 13.9), danach der Rest in
Planreihenfolge. V3/V4 laufen nebenher (klein).

- [x] **V1 Touch oben links** (Auftrag 13.4) – Rohdaten des GT911 auslesen
  (Statusregister, Punktzahl, Koordinaten, Konfiguration), Ursache der
  dauerhaften Berührung bei Panel (82, 1231) finden und beheben (nicht nur
  ausblenden). Nachweis: Rohdaten-Log + Ursache in STATUS; 10 Minuten ohne
  Berührung am Board: keine einzige gemeldete Berührung (Log).
  Nachweis 22.11.: Rohdaten-Diagnose (`ESP32OS_TOUCH_ROH`) –
  `docs/nachweise/v1_gt911_konfig.txt` (GT911 Firmware 1060, 720×1280,
  Konfig v70, Schwellen 90/60, Bildrate 10 ms: unauffällig). Ursache (Code):
  Treiber esp_lcd_touch_gt911 1.2.1 schreibt 0 ins Statusregister 0x814E
  auch ohne „bereit“ – meldet der GT911 dazwischen, geht die Meldung
  verloren; war es das Loslassen, hielt der Treiber den letzten Punkt für
  immer. Behoben: eigene Abfrage in `firmware/src/touch.rs` (nur Gelesenes
  quittieren; ohne Meldung 250 ms bei gesetztem Punkt = losgelassen),
  das Ausblenden nach 60 s ist entfernt (nur noch Log-Hinweis).
  `docs/nachweise/v1_touch_10min_log.txt`: 630 s ohne Berührung, Status
  durchgehend 00, keine gemeldete Berührung, Bildschirm aus nach 120 s,
  Ruhezustand nach 420 s ohne Aufwecken. Einschränkung: In den drei
  Diagnose-Starts trat der Fehler nicht auf; die Ursache ist darum am Code
  belegt, nicht im Mitschnitt. Gegenprobe 22.11. (Auftraggeber am Board):
  2 s Finger auf die Uhr → Bildschirmfoto wurde ausgelöst, ein langer Druck
  wird also weiter erkannt (der GT911 meldet bei liegendem Finger laufend).
- [x] **V2 Einstellungen sicher speichern** (Auftrag 13.7) – Ursache des
  Verlusts in der Nacht genau belegen, beheben; Speichern über temporäre
  Datei + Umbenennen, Sicherungskopie behalten, beim Laden Rückfall auf
  Sicherung bzw. temporäre Datei. Nachweis: Stromausfall während des
  Speicherns simuliert (PC-Test an jeder Stelle des Ablaufs; Board: Reset
  mitten im Speichern), Einstellungen danach vollständig.
  Stand 22.11.: Ursachen belegt (Moduldoku `kolibri/src/libini.rs`):
  (1) Schreiben nicht atomar, (2) zwei Schreiber (Einstellungen + Uhr-
  Sicherung der Firmware) ohne Sperre auf derselben Datei/Zwischendatei,
  (3) unlesbare Datei wurde wie leer behandelt → nur der neue Schlüssel
  blieb. Behoben: Sperre über Lesen–Ändern–Schreiben, Prüfsummen-Zeile,
  `.neu` → Umbenennen, Sicherungskopie `.bak`, Laden aus der ersten gültigen
  Fassung, bei nur beschädigten Fassungen kein Überschreiben. PC-Test
  `kolibri/tests/libini.rs::absturzsicher_speichern`: Abbruch nach jedem
  der 6 Schritte, beschädigte Datei → `.bak`, 4 Schreiber gleichzeitig.
  Board-Test (`ESP32OS_STROMAUSFALL`, `firmware/src/stromtest.rs`): Neustart
  mitten im Speichern an jedem Schritt, Prüfung nach jedem Start.
  Nachweis 22.11.: `docs/nachweise/v2_board_log.txt` – Abbruch nach Schritt
  1–6: jedes Mal „Einstellungen unverändert“ (9 Einträge wie die Referenz),
  Testwert bis Schritt 4 nicht da, ab 5 vollständig („wie erwartet“). Ein
  echter Stromausfall mitten in einem Sektor-Schreibvorgang lässt sich per
  Software nicht nachstellen; dafür sorgen FATFS + Verschleissausgleich von
  ESP-IDF, und die Prüfsumme fängt einen beschädigten Inhalt ab (PC-Test).
- [x] **V3 Aufräumen** (Auftrag 13.6) – SD-Testreste (`z`, `autoboot
  (Kopie).txt`, drei Bilder in `/Screenshots`) und `C:\ns\src` löschen.
  Nachweis: Verzeichnisliste vorher/nachher.
  Nachweis 22.11.: `C:\ns\src` gelöscht (36 MB Probe-Entpackordner; danach in
  `C:\ns`: 3.11, dl, zlib-1.4, zwei Werkzeuge). SD: Test-Build
  `ESP32OS_SD_AUFRAEUMEN` – `docs/nachweise/v3_sd_log.txt`: `z`,
  `autoboot (Kopie).txt` und die Bilder in `/Screenshots` sind weg, sonst
  alles unverändert. Abweichung: Die Vorher-Liste fehlt im Log (das Board
  startete nach dem Aufspielen unbeobachtet und löschte dort schon); seither
  gibt es `bauen_flashen.ps1 -Anhalten` (Board startet erst beim Mitschnitt).
- [x] **V4 Anleitungen** (Auftrag 13.1, 13.2) in STATUS: Akkuteiler und
  Strommessung (GPIO, Widerstände, Messung); C6-Firmware-Update Schritt für
  Schritt mit Rückweg. Nachweis: Abschnitte in STATUS.
  Nachweis 22.11.: STATUS „Anleitung: Akkuanzeige und Strommessung“ (GPIO21,
  200 kΩ/100 kΩ + 100 nF, Messung in Reihe, drei Zustände) und „Anleitung:
  C6-Firmware aktualisieren“ (Sicherung per `esptool read_flash`, Rückweg
  `write_flash`; Lage der C6-Anschlüsse vorher am Schaltplan prüfen).

## A. Kernschicht

- [x] **K0 Altlasten übernehmen** – fertig, wenn Quake-Fix `4af4f06` und die
  Firmware-Änderungen aus Übergabe 4.1 auf `esp32-os` committet sind und das
  alte Repo (`konsole`) unverändert weiter baut. Nachweis:
  `cargo test --profile sim -q` + Quake-32-Bit-Langlauf grün, Firmware-Build ok.
  ✔ 21.11.: Workspace-Tests alle ok; Quake i686 alle ok, Langlauf 31 072
  Bilder, Warp bei viewsize 100/110/120 ok; `bauen_flashen.ps1 -NurBauen`
  EXIT 0 (factory 1 248 432 B, doomapp 1 633 824 B, quakeapp 1 648 528 B).
  Altes Repo unberührt. Board-Test der 4.1-Änderungen folgt mit D1 (erstes
  Flashen).
- [x] **K1 Prozess-/Speichermodell** – Programme als FreeRTOS-Tasks, eigener
  PSRAM-Heap je Programm, statisch eingelinkte Programmtabelle; Start/Ende/
  Absturz eines Programms reisst das System nicht mit. Nachweis: Sim-Test
  (Programm starten, beenden, Heap wieder frei) + Board-Log.
  ✔ 21.11.: `cargo test --profile sim -p kern` 4 + 7 ok, 30 Wiederholungen
  ohne Fehler (normal, Absturz, beenden, Speicher voll, sysfunc −1, 2. Faden,
  40 Starts ohne Leck). Board (`KERN_SELBSTTEST=1`): „K1-SELBSTTEST OK: 40
  Starts … Verlust ab Runde 2 0 B, Regionen 0, Notbelegungen 0“.
- [x] **K2 sysfuncs-Kern** – Fenster (0), Pixel/Linie/Rechteck/Text/Bild
  (1, 4, 7, 13, 38, 65, 73), Ereignisse (10, 11, 23, 40), Knöpfe (8, 17),
  Maus/Touch (37), Zeit (3, 26, 29), Prozesse (9, 18, 51), Systemfarben/Skin
  (48), Speicher (68), Datei (70/80), Exit (-1) nach `sysfuncs.txt`.
  Nachweis: Testprogramm je Gruppe im Sim, Foto mit Erwartungsbild verglichen.
  ✔ 21.11.: `kern/tests/sysfunc.rs` 4 Gruppen grün (Zeichnen/Text/Bilder mit
  Erwartungsbild `tests/erwartet/k2_zeichnen.png` + Knopf/Taste/Schliessen;
  Maus/Knopf/Maske/Minimieren; Zeit/Warten/BCD; Info 9, Speicher 68, Stil 48,
  Faden 51 mit 2. Fenster `k2_zwei_fenster.png`, 18.2/3/7/21). 10 Läufe 64 Bit
  ohne Fehler, i686 grün. Umgesetzt zusätzlich: 2, 5, 12, 14, 35, 47, 63.1,
  66, 67, 71.1. Datei 70/80 → K6 (siehe Abweichungen).
  Firmware-Build (RISC-V) EXIT 0, factory 1 301 232 B.
- [x] **K3 ksys.h-Fassung** – nativer Header ruft die Rust/C-Funktionen statt
  `int 0x40`; ein Original-KolibriOS-C-Beispiel kompiliert für RISC-V
  unverändert und läuft. Nachweis: Build-Ausgabe + Foto.
  ✔ 21.11.: `kolibri/werkzeuge/ksys_nativ.py` → „118 Systemaufrufe ersetzt“
  (ktcc-ksys.h); `kern::abi::kos_syscall`. `basic_gui.c` unverändert:
  riscv32-esp-elf-gcc 14.2 `-c` exit 0 (4504 B, offen nur kos_syscall +
  libc); i686-Test `kolibri/tests/basic_gui.rs` grün: Foto
  `k3_basic_gui.png`, Tipp auf BUTTON1 → `k3_basic_gui_pop.png`
  („POP pressed....“), Schliessen → Ende Normal. Firmware-Build EXIT 0.
- [x] **K4 Original-Look** – `.skn`-Parser (Rahmen, Knöpfe, Farben),
  Systemschriften (CHAR.MT/CHAR2.MT), Taskleisten-Grafik aus Originaldaten,
  Touch-Skalierung. Nachweis: Foto eines Fensters mit Default-Skin.
  ✔ 21.11.: Default-Skin wie KolibriOS (`data/Tupfile.lua`: DEFAULT.SKN =
  Leency/Shkvorka), `.skn` aus Original-Quellen (`kern/werkzeuge/skn_bauen.py`,
  Layout `skins/skin.inc`), Parser `kern/src/skin.rs` (Kopf links/basis/oper,
  Rahmen 1/3/1, Knöpfe, dtp-Systemfarben, Titel nach
  `draw_window_caption`). Faktor 2: Kopf 48 px, Schliessen 54×36 (= 9,3 mm).
  Tests 15+1 grün, Foto `kern/tests/erwartet/k4_shkvorka.png`; Schriften
  `char.mt`/`charUni.mt` seit K2. Firmware-Build EXIT 0.
- [x] **K5 Dirty-Rects + Compositor** – nur geänderte Bereiche neu, über die
  bestehende PPA-/Zwei-Kern-Pipeline; Vollbildspiele direkt. Nachweis:
  Messwert (Bildzeit leerer Desktop / Fenster verschieben) in STATUS.
  ✔ 21.11.: `kern/tests/schirm.rs` (Teilbild = Vollbild, Pixel 1 px²,
  Verschieben 60 000 statt 230 400 px²); Board (`ESP32OS_DESKTOP=1`):
  leer 0 Bilder gezeichnet (Compositor 0,01 ms), Fenster verschieben 55 FPS,
  Compositor 7,3 ms/Bild bei ~95 000 px, p99 14,5 ms; Foto
  `docs/nachweise/k5_board_desktop.png`. Spiele laufen weiter direkt
  (eigene `Spiel`-Implementierungen, unverändert).
- [x] **K3b basic_gui.c am Board** (Vorgabe Nutzer 21.11.: „sobald es geht“)
  – das unveränderte C-Programm ist in die Firmware eingelinkt, startet im
  Desktop am Board und reagiert auf Touch. Nachweis: Board-Foto
  (`bildschirmfoto.py`) vor/nach Tipp auf BUTTON1 + Log.
  ✔ 21.11.: ESP-IDF-Komponente `kolibri/kolibri_c`; Build
  `ESP32OS_DESKTOP=1 ESP32OS_START=basic_gui ESP32OS_AUTOPILOT=…`; Fotos
  `docs/nachweise/k3b_board_basic_gui.png` → `…_pop.png` („POP pressed....“),
  Log „[debug] mouse ev (40,40)…“ aus dem C-Programm.
- [x] **K6 Dateisystem** – SD (FAT, SDIO) + interne Partition auf
  KolibriOS-Pfade (`/sd0/…`, `/sys/…`) abgebildet, sysfunc 70 vollständig
  (lesen, schreiben, Ordner, löschen, umbenennen, Info). Nachweis: Sim-Test +
  Board-Log mit SD.
  ✔ 21.11.: `kern/src/datei.rs` (70.0–70.12, 80, 30.1/2/4/5, virtuelle
  Ordner, cp866/UTF-16/UTF-8), `kern/tests/datei.rs` grün (alle
  Unterfunktionen inkl. Start mit Parametern). Board (`KERN_DATEITEST=1`):
  „SD-Karte eingehängt: 241724 MB als /sd0/1“, „K6-SELBSTTEST OK: 0 Fehler“
  (je /sys und /sd0/1: Ordner, 20 KB schreiben/lesen gleich, umbenennen,
  Ordner lesen, löschen). Firmware alle drei Images EXIT 0.
- [x] **K7 Systembibliotheken nativ** – box_lib, libimg, proc_lib, libini
  (gleiche Schnittstelle, Umfang nach Bedarf der Programme). Nachweis: Tests je
  Bibliothek + ein Programm, das sie nutzt.
  ✔ 21.11.: Crate `kolibri`: `boxlib.rs` (edit_box, check_box, scrollbar v/h,
  progressbar, option_box, frame), `libimg.rs` + `dekoder.rs` (BMP inkl.
  RLE, PNG inkl. Adam7/16 Bit/tRNS, GIF inkl. Animation, JPEG Baseline),
  `dialog.rs` (Datei-/Farbdialog), `libini.rs`; C-Exporte mit Original-
  Namen, clayer-Header unverändert; libc-Aufsatz (fopen… → Funktion 70,
  malloc → 68). Tests: `dekoder` (17 Bilder gleich Pillow, JPEG Ø<3),
  `libini`, `boxlib_umlaute`, i686 `bibliotheken`: Original-Beispiele
  `boxlib.c` (Eingabe per Touch + Tasten, Checkbox, Schieberegler) und
  `libimg.c` (logo.png aus /sys, Alpha, Skalierung) unverändert, plus
  `test_dialog.c` (Datei „/sys/a.txt“ und Farbe #3F0000 per Touch gewählt);
  Fotos `kolibri/tests/erwartet/k7_*.png`. Firmware alle Images EXIT 0.
- [x] **K-Review** – Review-Subagent: Diff gegen PLAN + Abschnitt 9.
  ✔ 21.11.: 7 Fehler + 1 Risiko gefunden, alle behoben: Heap-Überlauf bei
  riesigen Anfragen (+ calloc), 68.20 mit fremdem Zeiger, Skin/Einhängen im
  Programm-Heap angelegt, Board-Taskende ohne Idle-Abhängigkeit (anhalten →
  Aufräumer löscht), beendete Nebenfäden sofort abgeräumt (Stapel/TCB frei),
  Bereich erst beim nächsten Durchlauf frei, kein Aufräumen mitten im Start,
  Bedingungsvariable im System-Heap. Nachweis: Tests 64/32 Bit (5×) grün inkl.
  neuem `nebenfaeden_werden_einzeln_abgeraeumt`; Board „K1-SELBSTTEST OK: 50
  Starts (davon 10 mit je 20 Nebenfäden) … Verlust ab Runde 2 0 B“.

## B. Desktop, Taskleiste, Touch, Bildschirmtastatur (Auftrag 4)

- [x] **D1 Boot in den Desktop** – Board startet direkt in den
  ESP32-OS-Desktop (Startbild „ESP32-OS“, Hintergrund, Symbole, Taskleiste,
  Info-Dialog „ESP32-OS“). Nachweis: Foto nach Kaltstart.
  ✔ 21.11.: Shell `desktop/src/shell.rs` (@start, @icon, @taskbar, @menu,
  info; Original-Symbole icons32.png), Kern: 18.22, 18.25 (Ebenen), 15/39
  (Hintergrund), Fensterliste. Standardstart = Desktop (Spiel-Startziele und
  Merker „Spiele“ ausgenommen). Board: Fotos nach Kaltstart
  `docs/nachweise/d1_board_desktop.png`, Startmenü per Touch-Autopilot
  `d1_board_menue.png`; Log „ESP32-OS-Desktop startet (@start)“, Leerlauf
  0 Bilder/s neu; „Spiele“ → Neustart in den Launcher (Log), danach wieder
  Desktop. PC: `desktop/tests/shell.rs` (Startbild, Desktop, Menü, Info,
  OK) mit Fotos `desktop/tests/erwartet/d1_*.png`.
- [x] **D2 Logische Auflösung/Skalierung** – Wahl dokumentiert (Ziel ≥ 7–8 mm
  Ziele bei ≈ 294 ppi, quer), PPA-Skalierung. Nachweis: Foto + Masse in
  STATUS.
  ✔ 21.11.: 640×360 logisch, PPA ×2 scharf; Skin-Faktor 2. Masse in STATUS
  („Bedienmasse“), Fotos `docs/nachweise/d1_board_*.png`.
- [x] **D3 Tippen = Klick** – Nachweis: Autopilot-Tipp öffnet Programm, Foto.
  ✔ 21.11.: Gestenerkennung `desktop/src/gesten.rs`; PC `desktop/tests/gesten.rs` (Tippen → Knopf 7 links); Board: Tipp auf Symbol öffnet Info (`docs/nachweise/d7_board_verschoben.png`).
- [x] **D4 Lang drücken = Rechtsklick/Kontextmenü** – Nachweis: Autopilot,
  Foto mit Kontextmenü.
  ✔ 21.11.: Halten ≥ 600 ms → Rechtsklick (PC: Knopf 7 mit al = 2); Kontextmenü der Shell (`shell::kontextmenue`, wiederverwendbar) am Desktop-Symbol, „Öffnen“ startet Info; Fotos `desktop/tests/erwartet/d4_kontextmenue.png`, Board `docs/nachweise/d4_board_kontextmenue.png`.
- [x] **D5 Wischen = Scrollen** – Nachweis: Autopilot in Liste, Foto vorher/
  nachher.
  ✔ 21.11.: Wischen in den ersten 300 ms → 37.7 (120 px → +10 Schritte); Dateidialog-Liste geblättert, Fotos `d5_liste_vorher.png` / `d5_liste_nachher.png`.
- [x] **D6 Zwei-Finger-Zoom** (Ereignis an Programme, Browser/Bildbetrachter)
  – Nachweis: Autopilot-Pinch, Foto.
  ✔ 21.11.: zwei Finger 100 → 200 px → Zoom ×2 beim aktiven Fenster (`sys::zoom_holen`, ESP32-OS-Erweiterung); Anwendung in B5/P2.
- [x] **D7 Titelleiste ziehen = verschieben, Doppeltipp = maximieren** –
  Nachweis: Autopilot, zwei Fotos.
  ✔ 21.11.: PC: Ziehen (+120, +40) exakt, Doppeltipp → Arbeitsfläche 640 breit, nochmal → alte Lage; Fotos `d7_verschoben.png`, `d7_maximiert.png`; Board: Titelleiste per Autopilot gezogen (`docs/nachweise/d7_board_verschoben.png`).
- [x] **D8 Bildschirmtastatur als Systemdienst** – öffnet automatisch bei
  Fokus in box_lib-Editbox (und später Browser), zusätzlich per
  Taskleistenknopf; CH-Deutsch mit ä ö ü, Sonderzeichen-Ebene. Nachweis:
  Autopilot tippt „Grüezi äöü @#“ in Editbox, Foto.
  ✔ 21.11.: `desktop/src/tastatur.rs` (@tastatur, CH-QWERTZ, Shift è é à,
  Ebene 123 mit @ # € [ ] { } \ |), Auto-Einblenden über box_lib-Fokus +
  Feldlage, Taskleistenknopf. PC `desktop/tests/tastatur.rs`: „Grüezi äöü
  @#“ per Touch getippt (`d8_tastatur.png`), Aus/Ein über Taskleiste. Board:
  Original `boxlib.c` per Menü gestartet, Tastatur erscheint von selbst,
  „hallo“ per Autopilot getippt (`docs/nachweise/d8_board_tastatur.png`).
  Browser-Hook folgt mit B6.
- [x] **D9 Kopieren/Einfügen per Kontextmenü** (Zwischenablage, sysfunc 54)
  – Nachweis: Autopilot kopiert Text von Feld A nach B, Foto.
  ✔ 21.11.: Funktion 54 im Kern (Slots, 54.1 Kopie in den Programm-Heap,
  Text-Kopf Grösse/Typ/Kodierung UTF-8), box_lib: langer Druck im Feld →
  Kontextmenü „Ausschneiden / Kopieren / Einfügen / Alles markieren“
  (`kolibri::dialog::kontextmenue`). PC `desktop/tests/ablage.rs`: „Grüezi
  Wält“ von Feld A nach B kopiert; Fotos `d9_kontextmenue.png`,
  `d9_eingefuegt.png`. Firmware-Build EXIT 0.
- [x] **D10 Touchpad-Modus** (zuschaltbar, relativer Zeiger) – Nachweis:
  Autopilot, Foto mit Zeiger an Zielposition.
  ✔ 21.11.: Gesten im Touchpad-Modus (relativ ×1,5, Tippen = Klick am Zeiger,
  Halten = Rechtsklick, Tippen + Aufsetzen = Ziehen, zwei Finger = Scrollen);
  Zeiger im Compositor (Original `arrow.cur`), 37.4/37.5/37.6/37.8
  (`kern/src/zeiger.rs`, .cur 1–32 bpp, ARGB). Schalter vorerst im Startmenü
  (Einstellungen: P3). PC `desktop/tests/touchpad.rs`: Zeiger auf 20×20-Ziel
  geführt, Klick und Rechtsklick dort, Foto `d10_touchpad.png`;
  `kern/tests/sysfunc.rs` eigener Zeiger über dem Fenster.
- [x] **D12 Absturzmeldung** (Vorgabe Nutzer 21.11.) – stürzt ein Programm
  (auch C-Code mit CPU-Ausnahme) ab, startet das Board automatisch wieder in
  den Desktop und zeigt per Touch-Dialog, welches Programm abgestürzt ist
  (Name aus RTC-/NVS-Merker, Neustartgrund Panik). Nachweis: absichtlicher
  Absturz eines C-Testprogramms am Board → Log + Foto des Dialogs.
  ✔ 21.11.: CPU-Ausnahme: `--wrap=esp_panic_handler` (`komponenten/esp32os/
  absturz.c`) legt Taskname (= Programm) + Grund im RTC-Speicher ab; nach
  dem Neustart zeigt der Desktop `@meldung`. Board: C-Test `absturz_test`
  (Schreiben auf Adresse 0) → „Store access fault“, Neustart, Log „Letzter
  Neustart nach Absturz von „absturz_test““, Foto
  `docs/nachweise/d12_board_absturz.png`. Rust-Panik/Speicher voll ohne
  Neustart: `kern::neue_abstuerze` → Meldung (PC `desktop/tests/absturz.rs`,
  `d12_meldung.png`).
- [x] **D11 Keine Taste nötig** – alle Dialoge mit Touch-Knöpfen; grep nach
  Tastenabfragen in eingebundenen Programmen ohne Touch-Ersatz = leer.
  Nachweis: grep-Ausgabe + Liste.
  ✔ 21.11.: `docs/nachweise/d11_tasten.txt` (grep + Bewertung): alle
  Tastenabfragen der eingebundenen Programme haben Touch-Ersatz
  (Bildschirmtastatur, Fensterknöpfe, Kontextmenü). Quakes y-Abfrage ist
  Pflicht in S4 (Quake kommt dort in den Desktop), Ersteinrichtung in P4.
- [x] **D-Review** – Review-Subagent.
  ✔ 21.11.: Review-Subagent (Diff gegen PLAN + Auftrag Abschn. 9), 9 Befunde
  behoben: (1) Zwischenablage Kodierung 0 = UTF-8 wie ksys.h, cp1251 lesbar;
  (2) Touchpad-Halten: früher Loslass-Klick nur bei neu aufsetzendem Finger;
  (3) Titelleiste berühren aktiviert das Fenster; (4) box_lib Ausschneiden
  ohne Markierung tut nichts; (5) Grenzen 54.2 (1 MB), 15.1 (4096², 16 MB),
  15.5 Versatz; (6) neue_abstuerze liest Zähler unter der Sperre;
  (7) Wächter im Desktop (nicht in @taskbar): meldet auch Abstürze von
  @-Diensten und startet @icon/@taskbar/@tastatur neu; (8) Zeiger-Rechtecke
  beim Wechsel/Löschen neu gezeichnet; (9) Mindestmasse ≥ 7 mm (Taskleiste
  48 px, Tasten 42 px, Kontextmenü 44 px, Tabelle in STATUS). Dabei gefunden:
  Panik-Text lag im Programm-Heap → Zugriffsfehler nach Freigabe (~50 % im
  Absturztest), jetzt im System-Heap (0/60). Tests 64 Bit + i686 grün,
  Bilder angesehen, Firmware gebaut.

## C. Dateimanager, Bildbetrachter, Einstellungen (Auftrag 5)

- [x] **P1 Dateimanager** – SD + interne Partition; kopieren, verschieben,
  löschen, umbenennen, Ordner anlegen, alles per Touch (Kontextmenü).
  Nachweis: Autopilot-Ablauf je Funktion, Fotos + Dateiliste im Log.
  ✔ 21.11.: PC `desktop/tests/dateimanager.rs` (Gesten: Ordner öffnen/Hoch,
  Kopieren SD→intern + freier Name, Verschieben SD→intern (100 KB, Inhalt
  geprüft) und innerhalb SD, Umbenennen + Neuer Ordner über die
  Bildschirmtastatur, Löschen Datei/Ordner mit Inhalt mit Rückfrage; Teilbild
  = Vollbild bei jedem Foto), Fotos `tests/erwartet/p1_*.png`. Board:
  Autopilot auf der SD (anlegen, kopieren, umbenennen, verschieben, Ordner
  mit Inhalt löschen), Dateiliste je Schritt `docs/nachweise/p1_board_log.txt`,
  Fotos `p1_board_*.png`. Dabei behoben: FatFs ohne lange Namen (LFN an,
  UTF-8), Endlosschleife bei der Namenssuche (begrenzt), Fensterebene ≥ 1
  nach Wiederherstellen nicht neu gezeichnet (Tastatur unsichtbar beim
  zweiten Einblenden, Fotos vorher/nachher).
- [x] **P1b Sprache Englisch/Deutsch** (Auftrag 12.1) – Oberfläche Englisch
  als Standard, Deutsch umschaltbar (Schalter in P3); Bestehendes nachziehen:
  Desktop, Taskleiste, Startmenü, Info, Meldungen, Dialoge (proc_lib,
  Kontextmenüs), Dateimanager. Bildschirmtastatur zusätzlich US-Layout,
  umschaltbar. Nachweis: Tests/Fotos je Sprache, Autopilot Layoutwechsel,
  Sprache nach Neustart erhalten.
  ✔ 21.11.: `kern::sprache` (`t(en, de)`), Einstellungen in
  `/sys/settings/esp32os.ini` (`desktop::konfig`, beim Start geladen, auch
  vor einer Absturzmeldung). Englisch: alle PC-Bilder neu (`d1_*`, `d4`,
  `d5`, `d8`, `d12`, `p1_*`, `k7_dialog_*`). Deutsch + US aus der ini:
  `desktop/tests/sprache.rs` (Menü/Dateimanager deutsch, „yz“ über die
  US-Belegung getippt, Layout-Taste schaltet und speichert), Fotos `p1b_*`.
  Board: Tastatur CH → Layout-Taste → US, nach Neustart US
  (`docs/nachweise/p1b_board_*.png`). Sprachwahl in der Oberfläche folgt
  mit P3 (gleiche ini, gleicher Lademechanismus).
- [x] **P2 Bildbetrachter** – JPG über Hardware-JPEG, PNG, GIF, BMP; wischen =
  nächstes Bild, Pinch-Zoom. Nachweis: Foto je Format, Autopilot wischen/
  zoomen, JPEG-Dekodierzeit (HW) in STATUS. Dazu (Auftrag 12.4): prüfen, ob
  die HW-JPEG-Einheit progressive JPEGs kann – Ergebnis in STATUS; falls
  nicht, Software-Decoder (spätestens vor B1, siehe B0b).
  ✔ 21.11.: PC `desktop/tests/bildbetrachter.rs` (Gesten: Wischen links/
  rechts, Zwei-Finger-Zoom, Verschieben im Zoom, Fit/„>“-Knöpfe, GIF
  animiert, Farbfelder geprüft), Fotos `p2_*.png`. Board (Autopilot):
  BMP/JPEG/Zoom/PNG/GIF je Foto `docs/nachweise/p2_board_*.png`, Log
  `p2_board_log.txt`: JPEG 640×480 Hardware 55,8 ms (inkl. Farbumrechnung)
  gegen Software 810 ms, Abweichung HW/SW im Mittel 0,36; progressives
  JPEG: Hardware lehnt ab (nur Baseline, Code 0x105) → B0b nötig.
- [x] **P3 Einstellungen** – Helligkeit, Lautstärke, Akku, Datum/Uhrzeit
  (manuell + NTP), Touch (inkl. Touchpad-Modus), Hintergrundbild,
  Speicherplatz; WLAN-Seite folgt in W. Nachweis: Autopilot je Seite, Foto,
  Wert nach Neustart erhalten (NVS).
  ✔ 21.11.: Programm „einstellungen“ (Kacheln: Anzeige, Ton, Akku, Datum &
  Zeit, Touch, Hintergrund, Speicher, Sprache), Werte in
  `/sys/settings/esp32os.ini`. PC `desktop/tests/einstellungen.rs`: jede
  Seite per Gesten bedient, ini geprüft, Hintergrundbild über den
  Dateidialog, Neustart nachgestellt (`konfig::laden`) → alle Werte wieder
  da; Fotos `p3_*.png`. Board: Autopilot + Neustart, Werte im Boot-Log
  erhalten (`docs/nachweise/p3_board_log.txt`, Fotos `p3_board_*.png`),
  Speicherplatz echt (SD 504 MB = Pi-Bootpartition, intern < 1 MB).
  Dabei behoben: Uhr stellen am Board über `settimeofday` (vorher nur RAM-
  Korrektur, nach Neustart weg); Tipps gingen verloren, solange nach einem
  „alles neu zeichnen“ `@icon` rund 0,8 s zeichnete (Symbole jetzt
  zwischengespeichert, Verlauf in Bändern; Uhr stellen zeichnet nicht mehr
  alles neu); Autopilot übernimmt je Bild höchstens einen Eintrag.
- [x] **P4 Ersteinrichtung per Touch** – beim ersten Start WLAN + Uhrzeit
  einrichten. Nachweis: Autopilot nach NVS-Löschen, Fotos.
  WLAN-Schritt folgt mit W3 (Abweichung), bis dahin Sprache, Tastatur, Uhr.
  ✔ 21.11.: Programm „ersteinrichtung“ (Sprache → Tastatur → Datum & Uhrzeit
  → Fertig), von `@start` gestartet, solange `setup=1` fehlt. PC
  `desktop/tests/ersteinrichtung.rs` (frisches /sys, per Gesten Deutsch, US,
  Jahr + 1, Fertig → ini geprüft, danach kein Assistent), Fotos `p4_*.png`.
  Board: Einstellungen gelöscht (`ESP32OS_ERSTEINRICHTUNG=1`) → Assistent
  startet, Autopilot bis „Fertig“, Fotos `docs/nachweise/p4_board_*.png`;
  normale Firmware nach Neustart: „schon erledigt“ (`p4_board_log.txt`).
- [ ] **P5 Standby und Akku** (Auftrag 12.2; 13.1: Strommessung und
  Akkuteiler macht der Auftraggeber beim Einlöten des Akkus, Anleitung in
  STATUS – bis dahin offen) – Bildschirm nach einstellbarer
  Zeit aus, Ruhezustand, Aufwachen per Power-Knopf oder Touch, Akkustand in
  der Taskleiste. Nachweis: Log Abschalten + Aufwachen (Knopf und Touch),
  Foto Taskleiste, Stromverbrauch im Standby gemessen (STATUS).
  Stand 22.11.: Bildschirm aus / Ruhezustand / Aufwecken per Touch /
  Ruhezustand aus dem Startmenü am Board nachgewiesen
  (`docs/nachweise/p5_board_*`), Energie-Seite und Akku in der Taskleiste
  per PC-Test (`desktop/tests/energie.rs`). Offen (wartet auf Nutzer):
  PWR-Knopf (Knopfsuche gebaut, braucht Drücken am Board), Strommessung
  (kein Messgerät), Akkuanzeige am Board (Spannungsteiler).
- [x] **P6 Screenshot** (Auftrag 12.3) – per Taskleiste oder Geste ein
  Bildschirmfoto als PNG auf die SD-Karte. Nachweis: Autopilot, PNG von der
  SD als Nachweis, Dateiliste im Log.
  ✔ 22.11.: PNG-Kodierer nativ (`kolibri::kodierer`: Deflate mit LZ77 +
  festen Huffman-Codes, Rundreise-Test, Pillow liest es, 640×360-Oberfläche
  ≈ 9–25 KB). `@screenshot` über Startmenü oder langen Druck auf die Uhr der
  Taskleiste; Datei `/sd0/1/Screenshots/ESP32-OS_<Datum>_<Zeit>.png`,
  zurückgelesen und dekodiert. PC `desktop/tests/bildschirmfoto.rs` (beide
  Wege, Pixelvergleich, Menü nicht im Bild), Foto `p6_screenshot.png`.
  Board: Screenshot per Autopilot, Dateiliste im Log, im Bildbetrachter
  geöffnet (`docs/nachweise/p6_board_*`). Dabei behoben: statischer Mutex
  im Programm-Heap (Absturz, s. STATUS), klemmender Touch.

## D. WLAN Wi-Fi 6 (Auftrag 6)

- [x] **W0 Geheimnisse** (Auftrag 12.6) – WLAN-Testpasswort nur in einer
  lokalen Datei (in `.gitignore`), keine Passwörter/Schlüssel im Repo oder in
  Commits, HTTPS-Zertifikatsprüfung nie abgeschaltet. Nachweis:
  `.gitignore`-Eintrag, grep über Repo und `git log -p` ohne Treffer (vor
  jedem Commit in D/E und vor O8 wiederholen).
  Nachweis: `docs/nachweise/w0_geheimnisse.txt` – `wlan.local` ignoriert,
  `werkzeuge/geheimnis_pruefen.py` 0 Treffer (878 Dateien, 68 Commits),
  Board-Logs vor dem Lesen per `werkzeuge/log_schwaerzen.py` geprüft; die
  SSID-Ausgabe von esp_hosted (eh_wifi, Info) abgeschaltet.
- [x] **W1 esp_hosted + esp_wifi_remote** auf dem P4, SDIO zum C6. Nachweis:
  Boot-Log „Transport up“.
  Nachweis: `docs/nachweise/w1_board_log.txt` – SDIO 4 Bit 40 MHz, C6
  erkannt, „Transport zum C6 bereit, WLAN gestartet in 2066 ms“, verbunden
  in 4,7 s mit IP (esp_hosted 3.1.0, esp_wifi_remote 1.6.5).
- [ ] **W2 C6-Firmware versionsgleich** + Update vom P4 aus. Nachweis: Log mit
  gleichen Host/Slave-Versionen nach OTA vom P4. Auftrag 13.2: jetzt nicht
  (WLAN läuft); Anleitung mit Rückweg in STATUS (V4). W5 wartet darauf.
- [x] **W3 Netze suchen + verbinden per Touch** (dazu der WLAN-Schritt der
  Ersteinrichtung aus P4) (WPA2/WPA3, 802.11ax
  2,4 GHz). Nachweis: Autopilot in Einstellungen, Foto, Log mit PHY-Modus HE.
  Nachweis: PC `desktop/tests/wlan.rs` (Suchen, WEP abgelehnt, falsches
  Passwort, Bildschirmtastatur, verbunden + gemerkt, Blättern, Trennen,
  Vergessen) und `ersteinrichtung.rs` (WLAN-Schritt); Board-Autopilot:
  `w3_board_liste.png`, `w3_board_passwort.png`, `w3_board_verbunden.png`,
  Log „verbunden … PHY 802.11ax (HE)“ (siehe Abweichung „W3 PHY-Modus“).
- [x] **W3b Uhrzeit per NTP** (aus P3 verschoben, braucht WLAN) – in
  Einstellungen „Datum & Zeit“ automatisch stellen. Nachweis: Log NTP-
  Abgleich, Foto.
  Nachweis: Board-Log „NTP: Uhr gestellt – 22.11.2026 02:29:35 Ortszeit“
  (`w1_board_log.txt`, `w4_board_log.txt`), Taskleiste 02:33 auf
  `w6_board_taskleiste.png`; PC `w3b_zeit.png` (Schalter „Internetzeit“,
  ini `ntp=0/1`).
- [x] **W4 Bekannte Netze speichern + automatisch wiederverbinden** (auch nach
  Neustart). Nachweis: Neustart-Log „verbunden“ ohne Eingabe.
  Nachweis: `docs/nachweise/w4_board_log.txt` – normaler Build, Neustart:
  „1 bekannte Netze“ → „automatischer Versuch“ → „verbunden in 4537 ms“;
  PC `kern/tests/wlan.rs` (Abbruch → Wiederverbinden, Trennen von Hand hält an).
- [ ] **W5 iTWT** zum Stromsparen. Nachweis: Log iTWT-Setup ok.
  Stand 22.11.: Code fertig (iTWT beim Abdunkeln, Abbau beim Aufwachen);
  die C6-Firmware 0.0.0 beantwortet die Anfrage nicht
  (`docs/nachweise/w5_board_log.txt`) → wartet auf W2 (Abweichung).
- [x] **W6 Signalstärke in der Taskleiste**. Nachweis: Foto.
  Nachweis: `docs/nachweise/w6_board_taskleiste.png` (4 Balken, verbunden);
  Antippen öffnet Einstellungen → WLAN.
- [x] **W7 Transportpuffer im PSRAM** (`ESP_HOSTED_MEMPOOL_PREFER_SPIRAM`).
  Nachweis: sdkconfig + freier interner RAM vorher/nachher in STATUS.
  Nachweis: `docs/nachweise/w7_speicher.txt` – vorher 5 KB intern frei,
  „mempool OOM“, C6-Antworten verloren; nachher 17–20 KB frei im Betrieb,
  Bildpuffer unverändert (siehe Abweichung „W7 vor W1“).
- [x] **W8 iperf-Durchsatz** gemessen. Nachweis: Werte TCP/UDP in STATUS.
  Nachweis: `docs/nachweise/w8_durchsatz.txt` – TCP 11,6 (PC→Board) /
  9,8 Mbit/s (Board→PC), UDP 24,6 / 24,3 Mbit/s (eigenes Werkzeug statt
  iperf, siehe Abweichung).
- [x] **W9 KolibriOS-Netzwerk auf lwIP** (sysfunc 75 Sockets, 76/77,
  network-/http-Bibliothek nativ). Nachweis: KolibriOS-Netzwerk-C-Programm
  (z. B. Downloader) lädt Datei.
  Nachweis: `docs/nachweise/w9_board_log.txt` – `http_tcp_demo.c`
  (libc.obj-Beispiel, unverändert) löst kolibrios.org auf, verbindet, holt
  den Anfang der Antwort („HTTP/1.1 200 OK“, ein `recv` mit 512 Byte),
  endet sauber; `netz_laden.c` (eigenes Programm gegen dieselbe
  KolibriOS-API) lädt die Seite vollständig (6 Empfänge, 5247 Byte =
  Content-Length) und speichert sie unter `/rd/1/laden.html`. Funktionen 74
  (Geräte), 75 (Sockets, `kern::netz` auf `std::net`/lwIP), 76 (IPv4:
  Adresse, Maske, Gateway, DNS); network.obj nativ (`kolibri::netzwerk`);
  PC-Tests `kern/tests/netz.rs` + `netzwerk::tests` (64 Bit und i686).
  Siehe Abweichung „W9 http.obj, Funktion 77“.
- [x] **W-Review** – Review-Subagent.
  Nachweis: Review-Subagent 22.11. (Diff W0–W9 gegen PLAN und Auftrag 9);
  Befunde behoben in Commit `95fe94e` (Wettlauf Wiederverbinden/Verbinden
  von Hand, Socket-Leck beim Zwangsbeenden, Details Funktion 74/75/76),
  Rest als Abweichungen „W9 Funktion 74/76 teilweise“, „W3 WPA3“.
- [ ] **U1 Updates über WLAN** (Auftrag 12.7, nach Block D; 13.3: Nachweis
  mit lokalem HTTPS-Testserver am PC, Test-Zertifikat nur im Test-Build,
  normaler Build prüft weiter gegen GitHub; kein Release, kein Push – der
  echte Release erst in O8 mit OK) – Einstellungen
  „Nach Updates suchen“: neueste Version vom GitHub-Release über HTTPS
  (Zertifikatsprüfung an), Prüfsumme prüfen, in den OTA-Platz schreiben,
  Neustart; Versionsnummer unter „Info“. Nachweis: Log Update a→b, Foto
  Info, Rückfall bei absichtlich defektem Abbild (Rollback aus B0).
  Stand 22.11.: Code fertig, Abschluss wartet auf O8 (Release, nur mit
  deinem OK). Belegt (`docs/nachweise/u1_board_log.txt`): HTTPS zu
  api.github.com mit Zertifikatsprüfung (404, noch kein Release),
  abgelaufenes und selbst signiertes Zertifikat abgelehnt, fremde Datei als
  Abbild abgelehnt (Kopfprüfung), Startplatz unverändert; PC: Einstellungen
  → System (Suchen, Installieren, Fortschritt), Versionsvergleich,
  Release-JSON nur mit HTTPS-Adressen. Offen bis O8: Update a→b am Board
  und Rückfall mit absichtlich defektem Release-Abbild.

## E. Browser (Auftrag 5)

- [x] **B0 Flash neu aufteilen** (Auftrag 12.5, vor B1) – vorher alle Daten
  auf dem Board sichern (Würfelwelt-Welt, NVS-Spielstände, Einstellungen),
  danach zurückspielen; Platz für OTA (zwei App-Plätze) mit automatischem
  Zurückspringen, wenn die neue Version nicht startet (Rollback). Nachweis:
  Prüfsummen der Sicherung vorher/nachher gleich, Partitionstabelle, Log
  Rollback-Test.
  ✔ 22.11. (vorgezogen vor D: die App-Partition war zu 96 % voll, das WLAN
  passte nicht): Sicherung des ganzen Flashs zweimal, gleich (SHA-256),
  WAD/PAK im Abzug byte-gleich mit den PC-Dateien, NVS gültig
  (`docs/nachweise/b0_sicherung_pruefung.txt`). Neue Tabelle
  (`firmware/partitions.csv`): esp32os_a/esp32os_b (OTA, je 5,9 MB),
  doomapp/quakeapp (je 2 MB, Desktop-Code nicht mehr hineingelinkt: 1,7 MB),
  daten 16 MB; nvs unverändert. Alte Daten-FAT ausgepackt + DOOM1.WAD in
  ein neues Abbild (Gegenprobe SHA-256, `b0_daten_abbild.txt`); PAK vom
  Board roh auf die SD gerettet (CRC32 = PC-Datei). Nachher: Einstellungen,
  Würfelwelt, Uhr da; Doom (WAD aus /daten) 54,9 FPS; Quake (PAK von der
  SD) läuft; Rollback-Test: defektes Abbild in esp32os_b → Bootloader
  startet wieder esp32os_a („Rollback: esp32os_b ungültig“,
  `b0_board_log.txt`).
- [x] **B0b Progressive JPEGs** (Auftrag 12.4) – falls die HW-JPEG-Einheit
  sie nicht kann (Ergebnis P2): Software-Decoder. Nachweis: progressives
  Testbild dekodiert, Foto.
  ✔ 22.11.: eigener Decoder (`kolibri::dekoder`) kann SOF2 (Spektralauswahl,
  schrittweise Näherung, EOB-Läufe, Restart). PC: 4 progressive Testbilder
  wie Pillow/libjpeg (Mittel 0,02–0,09, wie Baseline); Board
  `docs/nachweise/b0b_board_progressiv.png`, Log `b0b_board_log.txt`:
  640×480 in 888 ms, pixelgleich mit dem Baseline-Foto.

- [x] **B1 NetSurf portiert** (Framebuffer-Frontend oder KolibriOS-Port, Wahl
  in STATUS) und rendert lokale HTML-Seite. Nachweis: Foto.
  Wahl: Upstream-NetSurf 3.11 mit Framebuffer-Frontend (STATUS „Nacht
  22.11.“; im KolibriOS-Repo gibt es nur einen Lader für ein fertiges
  x86-Programm, keine Quellen). Erst A6 vorgezogen (Abweichung „B1
  zurückgestellt“), am Morgen des 22.11. begonnen:
  Teil 1 ✔ Quellen per `esp32/netsurf/werkzeuge/quellen_holen.py`
  (`netsurf-all-3.11.tar.gz`, SHA-256 geprüft, generierte Dateien ohne
  make/gperf); Crate `esp32/netsurf` übersetzt libwapcaplet,
  libparserutils, libhubbub, libcss, libdom (Quelllisten aus den
  Upstream-Makefiles); `cargo test -p netsurf` (64 Bit und i686): HTML →
  DOM (auch fehlerhaftes HTML repariert), CSS geparst.
  Teil 2 ✔ NetSurf-Kern für den PC übersetzt (content, Fetcher data/
  resource/about/file, Handler HTML/CSS/Text/BMP/ICO/GIF, utils, desktop;
  ohne curl/JavaScript), dazu libnsutils, libutf8proc, libnsbmp, libnsgif,
  zlib 1.4 (zlib.net, SHA-256 wie veröffentlicht); iconv-Ersatz auf den
  Codecs von libparserutils (`netsurf/c/iconv.c`), POSIX-Header für die
  MSVC-Laufzeit (`netsurf/compat`). Tests (64 Bit und i686): URLs wie im
  Browser (auflösen, IDNA/Punycode), Zeichensätze (ISO-8859-1/-5,
  Windows-1252, nicht Darstellbares als `&#x…;`).
  Teil 3 (begonnen) ✔ libnsfb (Kern, Plotter, RAM-Fläche; Test: Rechteck
  und Linie in den Speicher gezeichnet, 64 Bit und i686) ✔ Framebuffer-
  Frontend übersetzt (gui, fbtk, interne Schrift); Schrift aus
  `glyph_data` mit dem Upstream-Werkzeug, 30 Symbole wie
  `tools/convert_image.c` (Python statt libpng), symbolische Links des
  Archivs als Kopie, getopt_long-Ersatz (`netsurf/c/getopt.c`).
  ✔ Messages (en, de) aus FatMessages (perl, wie upstream) ✔ Prüflauf ohne
  Bildschirm: das unveränderte Frontend lädt eine Seite in die Prüf-Fläche
  „probe“ (`netsurf/c/probe_lauf.c`), 64 Bit und i686 –
  `esp32/netsurf/tests/erwartet/b1_pc_seite.png`: Werkzeugleiste,
  Adresszeile, roter 200×100-Block, blaue Überschrift, Text, deutsche
  Statuszeile „Fertiggestellt“.
  Teil 4 (PC, 22.11. abends) ✔ Programm-Modus auf 32 Bit
  (`netsurf/compat/ns_programm.h`): malloc & Co. und strdup aus dem Heap
  des Programms, exit() beendet nur das Programm; QuickJS holt seinen
  Speicher über JS_NewRuntime2 ✔ Neustart: alle beschreibbaren statischen
  Daten der NetSurf-Bibliotheken liegen in einem Abschnitt (PC: .nsd/.nsb
  per Pragma; je Übersetzungseinheit zuerst eine beschreibbare Marke, sonst
  legt LLVM nie beschriebene Variablen schreibgeschützt in denselben
  Abschnitt), `netsurf/c/programm_daten.c` sichert ihn beim ersten Start
  (Daten und BSS – Konstruktoren vor main haben dort schon eingetragen)
  und spielt ihn bei jedem weiteren zurück ✔ Fläche „esp32os“
  (`netsurf/c/flaeche_esp32os.c`): Kern-Fenster über die Arbeitsfläche,
  RGB565 über Funktion 65 (Teilrechtecke mit Zeilenversatz), Maus/Touch,
  Rad, Tasten (ASCII + Pfeile usw.), Schliessen-Knopf ✔ Programm
  `netsurf::programm` („browser“, eine Instanz, Startparameter = Adresse).
  Nachweis (i686, alle Tests in einem Prozess als Kern-Programme):
  `neustart_gleiches_bild` (zwei Starts, gleiches Bild),
  `browser_im_kern_fenster` (Programm im Kern-Fenster, JS ändert die
  Überschrift, zweite Instanz abgelehnt, Schliessen per Tipp, zweiter
  Start) – Bild `netsurf/tests/erwartet/b1_pc_browser_2.png`.
  Teil 5 (Board, 22.11. abends) ✔ `netsurf/build.rs` beschreibt die
  Quellgruppen einmal und schreibt sie für ESP-IDF als
  `<Quellen>/erzeugt/netsurf_quellen.cmake` (gleiche 659 Dateien wie die
  PC-Tests; Stand-Datei im Komponentenordner, damit der Board-Bau
  Änderungen bemerkt) ✔ Komponente `firmware/komponenten/netsurf` mit
  Linker-Fragment: Daten in iram0_data, BSS (230 KB) im PSRAM, beide
  umrahmt für den Neustart; nie beschriebene Tabellen (IDNA, Zeichensätze,
  Codepages, libcss-Tabelle …) in die Flash-Daten statt in den internen
  RAM; Bilder des Frontends `const` (Generator) – NetSurf belegt intern
  noch 11,5 KB (vorher 87 KB) ✔ `utils/config.h` fürs Board als geprüfte
  Kopie (ESP_PLATFORM bei mmap, dirfd/fstatat/unlinkat, SIGPIPE: NetSurfs
  eigene Ausweichwege), `compat/board/sys/utsname.h` ✔ Ressourcen als
  schreibgeschütztes Dateisystem /nsres direkt aus dem Firmware-Abbild
  (`firmware/komponenten/esp32os/eingebettet.c`) ✔ Programm „browser“
  im Desktop (Symbol Globus, Heap 10 MB, JS-Grenze 4 MB).
  Nachweis Board (Firmware 5,36 MB = 86 % des Platzes):
  `docs/nachweise/b1/b1_board_*.png` (lokale Seite file:///nsres/de/
  welcome.html, 1,1 s, erster Start nach dem Aufspielen ohne Neustart)
  und `docs/nachweise/b1/neustart/b1_neustart_0..2.png` (per Autopilot:
  Symbol antippen → Seite in 0,7 s, ✕ → Desktop, Symbol erneut → zweiter
  Start mit zurückgespielten Daten, gleiche Seite); Log ohne Reset/Panik,
  PSRAM frei 13,5 MB ohne / 2,1 MB mit Browser.
  Umlaute über die Bildschirmtastatur kommen mit B6 (die nsfb-Tastencodes
  kennen nur ASCII).
Auftrag 13.9 (22.11.): eigener, guter Browser direkt am Board (ohne Pi,
ohne Fernmodus), möglichst viele echte Webseiten inklusive JavaScript. Weg:
NetSurf für HTML/CSS/Layout (B1 begonnen), moderne JavaScript-Engine statt
Duktape (QuickJS-ng, ES2020+) mit eigener DOM-/Web-API-Anbindung;
Entscheidung und Begründung: STATUS „Browser-Architektur“ (22.11.).

- [x] **B2 HTTPS** über mbedTLS + Zertifikatsbündel, Netz über
  esp_http_client/lwIP, Cookies. Nachweis: https://de.wikipedia.org lädt,
  Foto; Zertifikatsfehler wird abgelehnt.
  Dazu (Nutzer 22.11.): Standard-Suchmaschine DuckDuckGo HTML
  (html.duckduckgo.com/html/); Wörter statt Adresse in der Adressleiste →
  Suche; Startseite mit grossem Suchfeld. Nachweis am Board: per Touch
  suchen, ein Ergebnis antippen, die Seite lädt.
  Stand 22.11. nachts (PC): eigener Abrufer `netsurf/c/abruf.c` (ersetzt
  NetSurfs curl-Abrufer, http+https) über die Transport-Schicht
  `c/transport.h` – Board `transport_esp.c` (esp_http_client mit
  Zertifikatsbündel, je Abruf ein Task, höchstens 2 gleichzeitig), PC
  `transport_pc.c` (nur http, für Tests); Cookies über NetSurfs urldb.
  Adressleiste `c/eingabe.c` (Regel: Schema → wie eingegeben; ein Wort mit
  Punkt, IPv4, localhost → https://; sonst Suche über SearchEngines,
  DuckDuckGo HTML zuerst). Startseite `netsurf/res/{de,en}/welcome.html`.
  PC-Tests `eingabe_adresse_oder_suche`, `abruf_ueber_http` (302 mit
  Set-Cookie → Cookie kommt bei der nächsten Anfrage mit, grüne Seite),
  `startseite_mit_suchfeld` – 64 Bit 13/13, i686 15/15 grün.
  Stand 22.11. abends (Board, `docs/nachweise/b2/`): HTTPS geht – alle
  Abrufe von de.wikipedia.org/wikimedia/duckduckgo 200, Zertifikate geprüft;
  expired.badssl.com wird abgelehnt (Fehlerseite, Foto + Log). Touch-Suche
  per Autopilot: Startseite → Suchfeld → Tastatur (Taskleiste) → „esp32“ →
  Enter → DuckDuckGo-Ergebnisse (Fotos) → blättern → „More at Wikipedia“
  antippen → Abruf von de.wikipedia.org/wiki/ESP32 startet.
  Dafür behoben: interner RAM (CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL 16 KB →
  1 KB: beim Desktop-Start 104 statt 32 KB frei, vorher Abstürze in
  pthread/newlib/lwIP), Abrufer mit 2 festen Arbeitern statt Task je
  Abruf, Funktion 37.3 je Fenster (Kern-Test `maus_ereignisse_je_fenster`;
  kurzes Tippen geht im Browser nicht mehr verloren).
  Stand 22.11. nachts – Wikipedia lesbar: Ursachen gemessen und behoben:
  (1) Programm-Heap: eine nach Adresse sortierte Freiliste, 240 Mio.
  Listenschritte für eine Wikipedia-Seite (734 je malloc/free) → kleine
  Blöcke je Grössenklasse ohne Suche (2 Schritte je Aufruf; Kern-Tests
  `kleine_bloecke_ohne_suche`, `gross_nach_vielen_kleinen`); (2) jede
  Systemfunktion – auch jedes malloc/free – nahm in `pruefe_ende` die
  Kernsperre, die der Compositor lange hält → Merker je Faden ohne Sperre;
  (3) Rückfallebene ohne JavaScript (Nutzer 22.11., `c/ohne_js.c`): wird eine
  Seite nach 12 s nicht angezeigt oder sprengt ein Skript die JS-Grenze,
  wird JavaScript für die Seite abgeschaltet; Knopf „Ohne JS“/„Mit JS“ in
  der Werkzeugleiste lädt neu (PC-Tests `ohne_js_rueckfall_speicher`,
  `ohne_js_rueckfall_zeit`, `ohne_js_liste`); (4) Neu-Layout erst nach
  500 ms ohne DOM-Änderung; (5) Zwischenspeicher am Board 2 MB statt 12 MB,
  JS-Grenze 2 MB statt 4 MB. Werkzeug: `netsurf/werkzeuge/proxy.py` +
  Test `seite_ueber_proxy` (echte Seiten am PC nachstellen).
  Nachweis (Board, `docs/nachweise/b2/`): Zertifikatsfehler abgelehnt
  (`b2_board_zertifikat_abgelehnt.png`, Log); Wikipedia direkt geladen und
  angezeigt (`b2_board_wikipedia_direkt.png`, „Fertiggestellt 25.8 s“);
  Touch per Autopilot: Startseite → Suchfeld → Tastatur → „esp32“ → Enter →
  DuckDuckGo-Ergebnisse → blättern → „More at Wikipedia“ antippen → Artikel
  angezeigt (`b2_board_wikipedia_nach_touch.png`, Log
  `b2_board_log_touchsuche.txt`). PC: 64 Bit 16/16, i686 18/18 grün.
  Bekannt langsam (B10): jeder Abruf baut TLS neu auf (1,5–2 s), Box-Aufbau
  und Layout einer Wikipedia-Seite dauern am Board ~20 s.
- [x] **B3 Cache im PSRAM begrenzt**. Nachweis: Konfigwert + Speicherlog.
  Konfigwert: `NS_SPEICHER_CACHE` = 2 MB am Board (build.rs, gui.c-Kopie
  setzt `memory_cache_size` vor netsurf_init; NetSurfs Untergrenze) –
  NetSurf teilt 75 % Quelldaten (1536 KB) / 25 % Bilder (512 KB); vorher
  12 MB bei 10 MB Heap. Messung: llcache.c als geprüfte Kopie hält nach
  jedem Aufräumen Grösse, Grenze und freigebbaren Teil fest; Board-Log
  „ns_takt“ alle 5 s mit Heap und Zwischenspeicher.
  Nachweis: PC-Test `zwischenspeicher_grenze` (6 zwischenspeicherbare Seiten
  à 700 KB über einen Prüfserver: freigebbarer Teil ≤ Grenze, danach 0,7 MB
  statt 4,2 MB im Speicher; mit der alten Grenze schlüge er fehl) – 64 Bit
  17/17, i686 19/19 grün. Board (`docs/nachweise/b3/`): Touch-Suche →
  Wikipedia, Verlauf `b3_board_cache_verlauf.txt`: Grenze 1536 KB,
  freigebbar höchstens 111 KB, gesamt höchstens 1583 KB (alles von der
  offenen Seite belegt), Browser-Heap Spitze 9,4 MB < 10 MB, Seite
  angezeigt (`b3_board_wikipedia.png`).
- [ ] **B4 Adressleiste, Lesezeichen, Verlauf** per Touch. Nachweis: Autopilot,
  Fotos.
- [ ] **B5 Touch-Scrollen und Pinch-Zoom**. Nachweis: Autopilot, Fotos.
- [ ] **B6 Texteingabe in Formularen** über Bildschirmtastatur (Hook).
  Nachweis: Autopilot sucht in Wikipedia-Suchfeld, Foto Ergebnis.
- [ ] **B7 JavaScript** (Auftrag 13.9) – moderne Engine (ES2020+), Web-
  Funktionen DOM, Ereignisse, Timer, fetch/XHR, localStorage; Speicher pro
  Seite begrenzt (Seite bricht ab, Browser und System laufen weiter).
  Nachweis: PC-Tests (Skripte ändern DOM, Timer, fetch, localStorage,
  Speichergrenze), Board-Foto einer JS-Seite.
  Stand 22.11.: QuickJS-ng 0.18.0 eingebunden (Holskript, SHA-256 von
  GitHub), PC-Tests `javascript_modern` (ES2022/2023, async/await) und
  `javascript_grenzen` (Speichergrenze → „out of memory“, Zeitgrenze →
  unterbrochen, danach normal weiter), 64 Bit und i686.
  Stand 22.11. nachmittags: DOM- und Web-Anbindung steht am PC –
  `netsurf/c/js_quickjs.c` (NetSurfs js.h mit QuickJS: Laufzeit je
  Fenster mit Speicher- und Zeitgrenze, Kontext je Dokument, Grundfunktionen
  auf libdom, Ereignisse über libdom, Timer über NetSurfs Planer) und
  `netsurf/js/vorspann.js` (Node/Element/Document/Text, EventTarget mit
  Fang-/Sprudelphase, Event/CustomEvent/MouseEvent…, querySelector(All)
  mit eigener Selektor-Auswertung, classList, style, dataset, innerHTML/
  outerHTML/insertAdjacentHTML (Fragment-Parser mit Tabellen-Kontext),
  on…-Attribute, setTimeout/Interval/rAF, Promise-Aufträge, console, URL/
  URLSearchParams, location, navigator, atob/btoa). Nachweis: PC-Test
  `javascript_aendert_seite` (64 Bit und i686): Skript ersetzt ein rotes
  Rechteck durch ein grünes und hängt ein blaues an (Bild
  `netsurf/tests/erwartet/b7_pc_js_seite.png`), 22 Konsolen-Prüfungen
  (Ereignisse je genau einmal, stopPropagation, preventDefault, Timer,
  Fehler in einem Skript stoppt die folgenden nicht).
  Neu-Layout nach DOM-Änderungen nach dem Laden (`netsurf/c/neu_layout.c`):
  Änderungen werden 100 ms gebündelt, dann baut NetSurf den Box-Baum aus
  dem DOM neu (der alte bleibt bis dahin sichtbar), Stilauswahl neu (auch
  später eingefügte Stylesheets), Formularfelder behalten ihr Textfeld
  (getippter Text), iframes werden umgehängt (bei geänderter Menge neu
  angelegt), Fokus/Auswahl umgehängt; bei Fehler oder Schliessen kommt der
  alte Baum zurück. Nachweis: PC-Test `javascript_neu_layout` (Timer nach
  dem Laden färbt um, fügt einen Block ein, ein später eingefügtes
  `<style>` verbreitert ihn; Formularfeld, freies Feld und iframe bleiben;
  ≥ 2 Neu-Layouts), Bild `netsurf/tests/erwartet/b7_pc_neu_layout.png`.
  Offen in B7: Neu-Layout nach Klick/Eingabe am Board prüfen (am PC gibt
  der Prüflauf keine Eingaben), localStorage dauerhaft je Herkunft (jetzt nur im Speicher),
  document.cookie über NetSurfs Cookie-Ablage (B2), fetch/XHR (B2),
  Masse aus dem Layout (getBoundingClientRect, offset*, getComputedStyle
  liefern noch 0 bzw. nur das style-Attribut; scrollIntoView, select(),
  normalize() tun noch nichts), DOMContentLoaded kommt erst mit load
  (NetSurf meldet das Ende des Parsens nicht), Board.
- [ ] **B8 Bedienung** (Auftrag 13.9) – Tabs, Adressleiste mit Suche,
  Downloads, Zoom, Text markieren und kopieren (Zwischenablage), progressive
  JPEGs (B0b). Nachweis: Autopilot + Fotos je Funktion.
- [ ] **B9 20-Seiten-Test** (Auftrag 13.9) – Liste mit 20 echten Webseiten
  (einfach bis JavaScript-lastig: Wikipedia, Suchmaschine, Nachrichten,
  GitHub, Formulare, Menüs …), alle am Board getestet; Tabelle in STATUS
  (geht / teilweise / geht nicht, Grund). Nachweis: Tabelle + Fotos.
  Fest auf der Liste (Nutzer 22.11.): Google (www.google.com) und
  DuckDuckGo HTML (Standardsuche).
- [ ] **B10 Verbessern** – was in B9 am meisten bringt, beheben, Tabelle
  aktualisieren. Nachweis: Tabelle vorher/nachher.
- [ ] **B-Review** – Review-Subagent.

## F. Übrige Alltagsprogramme (Auftrag 5)

- [ ] **A1 Texteditor/Notizen** (öffnen, bearbeiten, speichern). Nachweis:
  Autopilot, Datei auf SD geprüft.
- [ ] **A2 Rechner**. Nachweis: Autopilot „12×3,5=“ → 42, Foto.
- [ ] **A3 Uhr mit Wecker und Timer** (Ton bei Ablauf). Nachweis: Autopilot,
  Foto + Ton-Log.
- [ ] **A4 Kalender**. Nachweis: Foto, Monatswechsel per Wischen.
- [ ] **A5 Musikplayer (MP3 von SD)** über ES8311. Nachweis: Log Wiedergabe,
  Autopilot Pause/Weiter/Nächster.
- [x] **A6 Taskmanager** (Prozesse, Speicher, beenden). Nachweis: Autopilot
  beendet Programm, Foto.
  **Pflicht (Vorgabe Nutzer 21.11.):** Hängende Programme (keine
  Systemaufrufe mehr, z. B. Endlosschleife) lassen sich zwangsweise beenden
  (Board: Task löschen, Heap/Fenster frei). Nachweis: Testprogramm mit
  Endlosschleife ohne Systemaufruf per Taskmanager beendet, System läuft weiter.
  Nachweis 22.11.: Board-Autopilot (`docs/nachweise/a6_board_liste.png`,
  `…_ausgewaehlt.png`, `…_beendet.png`, `…_menue_danach.png`,
  `a6_board_log.txt`): Taskmanager zeigt Programme mit PID, Fäden,
  Heap belegt/Grösse, Summe und freien RAM/PSRAM; `endlostest` (Schleife
  ohne Systemaufruf, belegt und gibt ständig Heap frei) antippen,
  „Beenden“ → nach 2 s kooperativ erfolglos zwangsweise angehalten (Log
  „beende“ 29,0 s → „Ergebnis 1“ 31,5 s), Fenster und Taskleisten-Knopf weg,
  PSRAM frei 15,1 → 15,3 MB, Meldung „hing - zwangsweise beendet“; danach
  öffnet das Startmenü normal, keine Fehler im Log. PC:
  `kern/tests/zwang.rs` (kooperativ; im Kern- und im geschützten Abschnitt
  nie angehalten; `pruefe_ende` erst nach dem geschützten Abschnitt;
  Endlosschleife angehalten, Region frei, Protokoll, zweiter Aufruf findet
  nichts, neues Programm läuft; 64 Bit und i686),
  `desktop/tests/taskmanager.rs` (Startmenü → Taskmanager per Touch,
  Hinweis ohne Auswahl, Systemdienst nicht beendbar, Hänger beendet; PC
  kann Threads nicht anhalten → „liess sich nicht anhalten“, siehe
  Abweichung). Review-Subagent vorgezogen, Befunde behoben (Abweichung
  „A6 Zwangsbeenden nur ausserhalb von Kern und Treibern“).
- [ ] **A-Review** – Review-Subagent.

## G. Spiele (Auftrag 5, 9)

Auftrag 13.8: Jedes Spiel ist ein eigenes Programm im Startmenü (Ordner
„Spiele“) und ein Symbol auf dem Desktop; Start im Vollbild, mit der
Home-Taste zurück in den Desktop. Der alte Launcher fällt in ESP32-OS weg;
der Knopf „Spiele“ öffnet ihn nicht mehr.

- [ ] **S0 Spiele im Desktop** (Auftrag 13.8) – Ordner „Spiele“ im
  Startmenü, vier Desktop-Symbole, kein Launcher mehr erreichbar. Nachweis:
  Fotos Menü und Desktop, Autopilot startet jedes Spiel.
- [ ] **S1 Pudding-Panik** startet aus dem Desktop im Vollbild, ≥ 55 FPS,
  Home-Taste zurück. Nachweis: Bildbilanz-Log, Autopilot.
- [ ] **S2 Würfelwelt** startet aus dem Desktop im Vollbild, ≥ 27,5 FPS,
  Home-Taste zurück. Nachweis: Log, Autopilot.
- [ ] **S3 Doom** (eigener Port) startet aus dem Desktop (ggf. Neustart in
  eigene Partition), ≥ 55 FPS; Home-Taste bzw. Beenden kehrt direkt in den
  ESP32-OS-Desktop zurück, nie in den Launcher. Nachweis: Log.
- [ ] **S4 Quake** (eigener Port, Warp-Fix) startet aus dem Desktop, ≥ 27,5
  FPS, „y“-Abfrage per Touch-Knopf; Rückkehr direkt in den ESP32-OS-
  Desktop. Nachweis: Log + Foto Dialog.
- [ ] **S5 Spieldaten nicht im Release** (Auftrag 12.8) – `DOOM1.WAD` und
  `pak0.pak` weder im Release noch in der Firmware; beim ersten Start von der
  SD laden oder per WLAN herunterladen, mit Hinweis auf die
  Shareware-Lizenz. Nachweis: Release-/Firmware-Inhalt ohne die Dateien,
  Autopilot Erststart (SD und Download), Foto Hinweis.
- [ ] **S-Review** – Review-Subagent.

## H. Optimierung und Abnahme (Auftrag 9)

- [ ] **O1 Optimierung** (Dirty-Rects, IRAM-Hotpaths, PSRAM-Heaps) – Messwerte
  vorher/nachher in STATUS.
- [ ] **O2 ELF-Lader von SD prüfen** (`espressif/elf_loader` auf P4) –
  Ergebnis in STATUS, umsetzen falls tragfähig.
- [ ] **O3 Lizenz** – GPL-2.0-Vermerk für übernommenen KolibriOS-/NetSurf-Code
  und -Daten im Repo. Nachweis: Datei `LICENSES/…` + README-Abschnitt.
- [ ] **O4 Abnahme-Durchlauf 9** – jeder Punkt aus 9 einmal per Autopilot am
  Board. Nachweis: Fotos/Logs.
- [ ] **O5 1 h Langtest am Board** (inkl. Browser + WLAN) ohne Absturz.
  Nachweis: Log 60 min, keine Panics/Resets.
- [ ] **O6 STATUS.md aktuell** (was läuft, Messwerte, offen). Nachweis: Datei.
- [ ] **O7 Startzeit** (Auftrag 12.9) – Zeit bis zum Desktop messen,
  optimieren, vorher/nachher in STATUS. Nachweis: Boot-Log mit Zeitstempeln.
- [ ] **O8 GitHub-Veröffentlichung** (Auftrag 12.10, letzter Punkt) –
  `LICENSE` (GPL-2.0), README auf Englisch mit Fotos und Flash-Anleitung,
  Release v1.0 mit fertiger Firmware, Web-Flasher. Releases nur mit OK des
  Nutzers (Push-Regeln in CLAUDE.md).
  README nennt als bekannte Einschränkung: WLAN-Passwort unverschlüsselt im
  NVS (Auftrag 13.5, keine eFuses).
  **Release-Firmware nie mit `-Wlan` bauen.** Dafür baut man mit
  `bauen_flashen.ps1 -Release`; das Skript bricht ab bei `-Wlan`, bei jeder
  gesetzten Umgebungsvariable `ESP32OS_WLAN_*` (Sitzung/Benutzer/System) und
  wenn SSID oder Passwort aus `wlan.local` im fertigen `.elf`/`.bin` stehen
  (Abbilder werden dann gelöscht). Vorbereitet 22.11.: Prüfung im Skript,
  am PC getestet (Parser ok; Wächter-Block: -Wlan und Variable abgelehnt;
  Byte-Suche mit erfundenen Werten: Treffer abgelehnt, sauberes Abbild
  durch). Offen vor O8: GPL-2.0 (NetSurf/KolibriOS) gegen Apache-2.0
  (ESP-IDF/esp_hosted) für das Binär-Release klären.
  Nachweis: Dateien im Repo, Release-Entwurf; Push erst nach Freigabe.
- [ ] **Wenn Zeit bleibt:** PDF-Betrachter, Kamera-App (OV5647), weitere
  KolibriOS-C-Programme.

## Abweichungen

- **B2, Rückfallebene ohne JavaScript (22.11.):** Gewünscht war „die Seite
  ohne JavaScript anzeigen“. Automatisch (Zeitgrenze 12 s, JS-Speicher-
  grenze) wird JavaScript an Ort und Stelle abgeschaltet statt die Seite
  neu zu laden – am Board passten die alte Seite (mit JS) und die neu
  geladene zusammen nicht in den 10-MB-Heap (gemessen: 10,3 MB → Fehler-
  seite). Folge: `<noscript>`-Inhalte erscheinen dabei nicht. Der Knopf
  „Ohne JS“ lädt die Seite wirklich ohne JavaScript neu (dann mit
  `<noscript>`). JS-Speichergrenze am Board 2 MB statt 4 MB.

- **K1 Beenden von aussen ist in K1 kooperativ:** Ein Programm endet bei
  seiner nächsten Systemfunktion. Zwangsweises Beenden hängender Programme
  ist Pflicht in A6 (Vorgabe Nutzer).
- **K1 Absturzschutz gilt für Rust-Paniken und Speicher voll,** nicht für
  CPU-Ausnahmen (z. B. Zugriffsfehler in C-Code): diese starten das Board
  neu. Warum: gemeinsamer Adressraum ohne MMU-Schutz (Auftrag 7). Ersatz:
  D12 – Neustart in den Desktop mit Meldung, welches Programm abstürzte.
- **K2 → K6: Funktionen 70/80 (Dateien) und 68.27 (Datei laden)** werden mit
  dem Dateisystem in K6 umgesetzt und dort nachgewiesen. Warum: Sie hängen an
  der Pfadabbildung (SD, interne Partition), die K6 festlegt; so entsteht
  kein doppelter Code.
- **K2 Mauszeiger-Funktionen 37.4–37.8:** mit D10 umgesetzt; der Zeiger ist
  nur im Touchpad-Modus sichtbar (sonst gibt es bei Touch keinen Zeiger).
- **K2 Gesamtumfang 18/26/48/68:** umgesetzt sind 18.2/3/7/13/16/17/18/21,
  26.9/10, 48.0/2/3/4/5/6/7, 68.12/13/20. Weitere Unterfunktionen (z. B.
  18.25 „immer oben“ für die Taskleiste, 48.8 Skin laden, 68.19 DLL laden)
  kommen mit dem Punkt, der sie braucht (D1, K4, K7), und werden dort
  nachgewiesen.
- **Schrift 8×16 deckt wie im Original nur Codepunkte < 1419 ab** (Latein,
  Griechisch, Kyrillisch); z. B. „–“ und „€“ erscheinen als Kästchen – wie
  in KolibriOS.
- **K3 C-Programme brauchen `-Wno-int-conversion` (GCC 14):** Der Original-
  Header weist Zeiger an `uint32_t`-Felder zu; der Original-Compiler (tcc)
  erlaubt das. Header bleibt unverändert, nur die Warnstufe ist angepasst.
- **K3 C-`malloc`:** in K7 gelöst – der libc-Aufsatz leitet malloc/calloc/
  realloc/free auf Funktion 68 (Programm-Heap) um.
- **K4 Taskleisten-Grafik:** Die KolibriOS-Taskleiste (@taskbar) ist ein
  eigenes Programm; ihr Aussehen kommt aus den Systemfarben `taskbar` /
  `taskbar_text` des Skins (dtp, jetzt geladen) und eigenen Symbolen. Die
  Taskleiste selbst wird mit D1 gebaut und dort nachgewiesen.
- **K4 `.skn` ohne kpack:** Original-Skins im Image sind kpack-gepackt; der
  Kern liest ungepackte `.skn` (wie `skn_bauen.py` sie erzeugt) und lehnt
  gepackte mit Meldung ab. Funktion 48.8 mit Dateipfad kommt mit K6
  (Dateisystem); aus dem Speicher geht sie jetzt (`sys::skin_setzen`).
- **K6 symbolische Verknüpfungen (70.11/70.12):** Die abgebildeten
  Dateisysteme sind FAT; wie der FAT-Treiber von KolibriOS liefern sie
  Code 2 („nicht unterstützt“).
- **K6 Speicher nur im Desktop-Start eingehängt,** solange die Konsole der
  Standard ist (Spiele brauchen den internen RAM); ab D1 immer.
- **D2 Touch-Faktor 2 und alte Programme:** Programme mit festen
  Koordinaten (z. B. basic_gui: Knöpfe bei y = 30) zeichnen in den doppelt
  hohen Kopf (48 px). Entscheid: Faktor 2 bleibt (Kopf/Schliessen ≥ 7 mm
  geht vor); Programmknöpfe liegen im Stapel über den Kopfknöpfen und
  bleiben antippbar. Programme, die 48.4 (Skinhöhe) abfragen, passen sich an.
- **K7 Umfang nach Bedarf:** nicht umgesetzt (kein gewähltes Programm braucht
  sie): box_lib menubar, pict_button, pathview; libimg `img_encode`
  (Speichern) und Exoten-Formate (ICO, TGA, PCX, TIFF …); libini
  `ini_get_shortcut`. Kommen mit dem Programm, das sie braucht.
- **K7 JPEG:** Baseline (auch erweitertes Huffman) wird dekodiert;
  progressive/arithmetische JPEG → Fehler „Format“. Restart-Marker sind
  umgesetzt, aber ungetestet (Pillow erzeugt keine). Bildbetrachter (P2)
  nutzt Hardware-JPEG, Grenzen siehe dort.
- **K7 dialog.c:** Das Original-Beispiel ist nur mit tcc übersetzbar
  (`(rgb_t)uint`-Cast); Ersatz `kolibri/c/test_dialog.c` mit gleichem Ablauf.
- **K7 Umlaute in Textfeldern:** Funktion 2 liefert cp866-Codes (ohne ä ö ü).
  Erweiterung: al = 3 → Unicode in Bits 16–31 (unsere box_lib, Bildschirm-
  tastatur D8). Felder mit UTF-8-Schrift nehmen so alle Zeichen; cp866-Felder
  (ältere Programme) nur cp866-Zeichen – wie im Original.
- **K2-Korrektur (bei K7 gefunden):** Funktion 37.1 rechnet wie der
  KolibriOS-Kern (`gui/mouse.inc`) immer relativ zum Arbeitsbereich.
- **Mehrere C-Programme in einem Binary:** `kolibri/werkzeuge/praefix.py`
  erzeugt Hüllen `c/gen/<name>.c`, die globale Symbole umbenennen
  (Originalquellen bleiben unverändert).
- **D3–D7 Gesten-Zuordnung (Entscheid):** Bewegung in den ersten 300 ms
  ist Wischen (Scrollen); wer in alten Programmen mit gedrückter Taste
  ziehen will (Schieberegler, Markieren), hält 300–600 ms und zieht dann.
  Zoom ist eine ESP32-OS-Erweiterung (`sys::zoom_holen`), KolibriOS kennt
  kein Zoom-Ereignis; alte Programme sehen nur ein Mausereignis.
- **D8 Tastaturtasten senden keine Scancodes** (Bits 16–23 = 0); Programme
  im Scancode-Modus (66.1 = 1) bekommen nur ASCII/Unicode. Alle gewählten
  Programme arbeiten im ASCII-Modus.
- **Systemfenster nehmen die Maus für sich:** Druck über Tastatur, Taskleiste
  oder Kontextmenü (Ebene ≥ 1) erreicht nur dieses Fenster (wie in
  KolibriOS, wo das angeklickte Fenster aktiv würde).
- **Prioritäten am Board:** Programme 3, Anzeige-/Eingabeschleife 5 – ein
  rechnendes Programm kann die Bedienung nicht blockieren.
- **P1 Dateimanager nativ statt portiert:** KolibriOS hat keinen
  Dateimanager in C (KFM/KFar sind FASM, Eolite ist C--) → nach Auftrag 1.5
  selbst im KolibriOS-Stil (Eolite als Vorbild, Originalsymbole). „Öffnen“
  startet Bildbetrachter/Notizen/Musik nach Endung – diese Programme kommen
  mit P2/A1/A5, bis dahin meldet der Dateimanager „… fehlt“. Blättern per
  Wischen (D5) und Bildlaufanzeige, keine eigenen Pfeilknöpfe.
- **P2 Bildbetrachter nativ:** KolibriOS-Bildbetrachter (kiv) ist FASM →
  selbst im KolibriOS-Stil (Auftrag 1.5). Bilder bis 2,1 Megapixel
  (RGBA im 12-MB-Programm-Heap), grössere → Meldung statt Absturz.
  Hardware-JPEG liefert YUV444, RGB rechnet die Software (die
  DMA2D-Farbumrechnung nutzt TV-Bereich und übersättigt, gemessen).
  Progressive JPEGs: Hardware nur Baseline → seit B0b der Software-Dekoder.
- **P3 NTP erst mit WLAN:** Die Uhr wird in P3 von Hand gestellt; der
  NTP-Abgleich braucht das Netz und ist als W3b in Block D eingeplant.
- **P3/P5 Akku nicht messbar:** Die Platine lädt den Akku, misst die
  Spannung aber an keinem GPIO (`firmware/src/board.rs`). Die Akku-Seite
  sagt das; mit einem Spannungsteiler an GPIO 21 (`BATTERY_SUGGESTED`)
  zeigt sie Prozent. Braucht eine Lötarbeit des Nutzers. Entscheid Nutzer
  21.11.: Spannungsteiler wird später eingelötet → dann in `board.rs`
  `BATTERY = Some(BATTERY_SUGGESTED)` setzen, Teiler mit Multimeter
  abgleichen, Akku-Seite und Taskleiste (P5) am Board nachweisen.
- **P3 Einstellungen in einer ini statt im NVS:** `/sys/settings/esp32os.ini`
  auf der internen FAT (KolibriOS-Stil, mit libini); bleibt wie NVS über
  den Neustart erhalten (Nachweis Boot-Log). Ohne Strom steht die Uhr bei
  1970 – dann gilt die zuletzt gesicherte Zeit (alle 10 min, beim Stellen,
  vor „Neustart“), bis NTP sie stellt.
- **P3 Hintergrundbild halb aufgelöst:** gespeichert als 320×180 RGB565
  (115 KB), weil die interne Partition nur 1 MB hat; mehr Platz mit B0.
- **P3 Sprache und offene Fenster:** Beim Umschalten zeichnen alle Fenster
  neu; Fenstertitel der Shell-Dialoge (z. B. Info) wechseln erst beim
  nächsten Öffnen (Funktion 0 übernimmt den Titel nur einmal).
- **P4 WLAN-Schritt mit W3:** Die Ersteinrichtung fragt bis Block D nur
  Sprache, Tastatur und Uhrzeit ab; der WLAN-Schritt kommt mit W3 dazu.
- **P5 wartet auf Nutzer (22.11.):** Aufwachen per PWR-Knopf: unbekannt, ob
  der Knopf an einem GPIO hängt; Entwicklungs-Build `ESP32OS_KNOPFSUCHE=1`
  protokolliert jeden Pegelwechsel – braucht einmal BOOT/PWR drücken.
  Strommessung im Standby: kein Messgerät vorhanden. P5 bleibt bis dahin
  offen; der Rest ist umgesetzt und nachgewiesen.
- **Startmenü: Beispiele unter „Mehr Programme…“ (22.11.):** Am Board hatte
  das Menü mit Bildschirmfoto 11 Einträge und ragte über die Taskleiste;
  Beispielprogramme (Beispiel-GUI, Steuerelemente, Bild-Demo) sind jetzt
  ein Untermenü.
- **B0 vorgezogen (22.11.):** vor Block D statt vor B1 – das WLAN passte
  nicht mehr in die factory-Partition (96 %).
- **B0 Quake-PAK auf der SD:** Mit zwei OTA-Plätzen für ESP32-OS ist im
  Flash kein Platz für die 18,7-MB-PAK; sie liegt auf der SD-Karte unter
  `ESP32-OS/quake/pak0.pak` (passt zu Auftrag 12.8). Quake braucht die SD.
  Am Board 25–28 FPS statt 27,5 (SD-Zugriffe) → in S4 prüfen (PAK in den
  PSRAM laden).
- **B0 Updates nur für ESP32-OS:** Rollback/OTA gilt für esp32os_a/b; Doom-
  und Quake-Programm bleiben eigene Plätze (melden sich ebenfalls als gültig).
- **W1 esp_hosted 3.1.0 als gekürzte lokale Kopie (22.11.):** Die
  Registry-Fassung enthält Beispiele/Tests mit Pfaden über 260 Zeichen; der
  Komponenten-Manager scheitert unter Windows beim Kopieren. Ersatz:
  `werkzeuge/komponenten_holen.py` (feste Version, SHA-256, ohne
  Beispiele/Tests/Doku, nach `C:\ec\v3.1.0\espressif__esp_hosted`),
  eingebunden per `override_path`. 3.x statt 2.x, weil der 3.x-Host laut
  Anleitung auch mit alter C6-Firmware (0.0.6+/1.x/2.x) arbeitet.
- **W7 vor W1 gezogen (22.11.):** Ohne PSRAM-Puffer bekam esp_hosted keinen
  internen DMA-Speicher (19 KB frei nach dem Start der Anzeige) – die
  Antworten des C6 gingen verloren. Die Option heißt in 3.x
  `EH_HOST_PORT_DMA_PREFER_SPIRAM` (nicht `ESP_HOSTED_MEMPOOL_PREFER_SPIRAM`)
  und wirkt auf dem P4 erst mit einer kleinen Anpassung an esp_hosted
  (`komponenten_holen.py`, prüft den Originaltext).
- **W3 PHY-Modus (22.11.):** Die C6-Firmware (0.0.0) kennt die Abfrage des
  ausgehandelten PHY-Modus nicht. „802.11ax (HE)“ im Log kommt darum aus den
  Fähigkeiten des Zugangspunkts (Wi-Fi 6) bei einem C6, der 802.11ax kann –
  das Log sagt das dazu. Mit neuer C6-Firmware (W2) fragt der Treiber den
  ausgehandelten Modus ab.
- **W1 Reserve für das WLAN (22.11.):** Die Bildpuffer nahmen je nach
  Startreihenfolge fast den ganzen internen RAM; dann hing der WLAN-Start
  still. Jetzt lassen sie 64 KB frei (`hw::INTERN_RESERVE`), ein Wächter
  meldet einen hängenden Start nach 15 s. Die Lage der Bildpuffer blieb
  dabei gleich (61/61 KB intern, 40 KB PSRAM).
- **W5 wartet auf W2 (22.11.):** iTWT ist umgesetzt, die C6-Firmware 0.0.0
  kennt die iTWT-Anfrage aber nicht. Mit neuer C6-Firmware erneut prüfen.
- **W2 wartet auf dich (22.11.):** Die C6-Firmware ist 0.0.0; ein Update
  vom P4 aus (OTA über SDIO) gibt es erst ab C6-Firmware 2.6. Mit 0.0.0 geht
  es nur per Kabel am C6 (Nachtregel 3: nicht ohne nachgewiesenen Rückfall).
- **B0b vor U1 gezogen (22.11.):** U1 braucht für den Nachweis ein Release
  auf GitHub (O8, erst mit deinem OK zum Pushen) – wartet also auf dich.
- **W8 eigenes Messwerkzeug statt iperf (22.11.):** Auf dem PC gibt es kein
  iperf; `werkzeuge/durchsatz.py` + `src/durchsatz.rs` messen TCP/UDP in
  beide Richtungen, der PC baut alle Verbindungen auf (keine Firewall-Regel).
  Dabei gefunden: Empfang brach unter Last ab (alte C6-Firmware streamt
  ohne Modus-Meldung) → Anpassung an esp_hosted (dynamische Puffer).
- **W9 http.obj, Funktion 77 (22.11.):** http.obj (HTTP-Client-Bibliothek)
  baue ich nach, sobald ein gewähltes Programm sie braucht (Auftrag 2.4:
  „nur so weit, wie gewählte Programme sie brauchen“) – der Browser bringt
  seinen eigenen Abrufer mit. Funktion 77 ist in KolibriOS Futex/Pipes
  (kein Netzwerk) und kommt mit dem ersten Programm, das sie nutzt.
  `socketpair` (75.10) gibt es mit lwIP nicht (Fehler EOPNOTSUPP).
- **C-Programme: exit() (22.11.):** `exit()` aus einem KolibriOS-C-Programm
  hätte die Host-libc und damit das ganze System beendet; jetzt beendet es
  nur das Programm (Funktion −1, `kos_exit`).
- **W9 Funktion 74/76 teilweise (22.11., Review):** lwIP führt keine Zähler
  je Gerät → 74.6–9/11–16 und 76-Zähler liefern 0; 74.2–4 (Gerät
  zurücksetzen/anhalten/Zeiger) → −1. 76: IPv4-Adresse/Maske/Gateway/DNS
  und die MAC des C6 werden gelesen; Setzen (76.3/5/7/9) → −1, die
  Adressen kommen per DHCP; ARP 76.5.3–7 → −1. Bei TCP gilt ein `bind`
  vor `connect` nicht (std::net verbindet ohne vorheriges Binden; die
  lokale Adresse wählt lwIP).
- **W3 WPA3 (22.11., Review):** Am Board stand nur ein WPA2-Netz zur
  Verfügung (Testnetz); WPA3/SAE ist eingestellt (PWE beide), mit einem
  WPA3-Netz aber nur in der Simulation geprüft.
- **U1 wartet auf O8 (22.11.):** Update a→b und der Rückfall mit einem
  defekten Release-Abbild brauchen ein echtes GitHub-Release (Push nur mit
  deinem OK). Flash-Schreiben aus Fäden mit PSRAM-Stapel (alle Programme)
  löste beim OTA ein Assert aus (Abbildprüfung/otadata über mmap) – der
  Schreibteil läuft darum in einem kleinen Task mit internem Stapel.
- **Einstellungen gingen einmal verloren (22.11.):** Ein Absturz des
  (damals fehlerhaften) Update-Tests fiel in das Sichern der Uhrzeit nach
  dem NTP-Abgleich; die ini wurde gekürzt und nicht mehr geschrieben →
  Standardwerte und Ersteinrichtung. Jetzt schreibt libini absturzsicher
  (`.neu`, Gegenprobe, ersetzen; Rückfall beim Laden). Deine Werte am Board
  habe ich wiederhergestellt (Deutsch, Helligkeit 10, Bildschirm aus 120 s,
  Ruhe 300 s, Touchpad aus, Layout CH, Lautstärke 6, Verlauf blau).
- **B1 zurückgestellt, A6 vorgezogen (22.11.):** Der NetSurf-Port (NetSurf
  + libcss, libdom, hubbub, libparserutils, libwapcaplet, libnsutils,
  libnsfb, für RISC-V und die i686-Tests) dauert mehrere Tage und wäre am
  Morgen nur halb fertig und nicht nachweisbar gewesen (Nachtregel 2). A6
  ist von dir ausdrücklich als Pflicht verlangt, hängt nicht am Browser und
  war in der Nacht vollständig nachweisbar. B1 ist der nächste Punkt.
- **A6 Zwangsbeenden nur ausserhalb von Kern und Treibern (22.11.):** Ein
  Faden wird nur angehalten und ausgetragen, wenn er gerade keinen
  Kern-Code und keinen geschützten Abschnitt ausführt (`kern::geschuetzt`:
  WLAN-Suche/Verbinden, Sockets und Namensauflösung, Update, JPEG-Einheit –
  dort hält er Sperren oder Treiber, die nicht dem Programm gehören) und
  die Sperre seines Heaps nicht gesetzt ist. Sonst wird er nicht angefasst
  und es wird erneut versucht; er endet dort meist selbst (blockierende
  Socket-Aufrufe brechen mit `ECONNABORTED` ab, `pruefe_ende` wartet bis
  nach dem geschützten Abschnitt). Bleibt er > 5 s dort, meldet der
  Taskmanager „liess sich nicht anhalten“, das Programm endet beim nächsten
  Systemaufruf. stdout/stderr der C-Programme gehen auf das Debug-Board
  (Funktion 63, im Kern-Abschnitt) statt an die stdout-Sperre der Host-libc
  (sonst stünde nach dem Anhalten mitten in `printf` jede Firmware-Ausgabe).
  Globale Sperren im Desktop (Anwendungen, Haken, Hintergrund) nur noch im
  Kern-Abschnitt. libc-Funktionen mit globalen Sperren (z. B. `localtime`,
  `getenv`) nutzen die C-Programme nicht; kommen sie dazu, müssen sie über
  den Kern laufen. Review-Subagent (22.11.) fand diese Lücken; alle behoben.
  Am PC lassen sich Threads nicht anhalten: Fenster weg, der Thread läuft
  weiter, Meldung „liess sich nicht anhalten“ (PC-Test); echtes Anhalten
  prüft `kern/tests/zwang.rs` (nachgebildetes vTaskSuspend) und das Board.
  Systemdienste („@…“) lassen sich im Taskmanager nicht beenden.
- **B1 „Seite komplett speichern“ fehlt (22.11.):** NetSurfs
  `desktop/save_complete.c` braucht POSIX-Regex, das weder die MSVC-Laufzeit
  (PC-Tests) noch newlib (Board) hat. Das Framebuffer-Frontend bietet diese
  Funktion ohnehin nicht an, und kein anderer Teil ruft sie. Ersatz: keiner
  nötig; kommt sie später dazu, braucht sie eine Regex-Bibliothek.
- **B1 Zeichensätze (22.11.):** NetSurf erwartet iconv. Ersatz: eigener
  iconv auf den Codecs von libparserutils (UTF-8, UTF-16, US-ASCII,
  ISO-8859-x, Windows-125x …) – dieselben Zeichensätze, die NetSurf beim
  HTML-Parsen ohnehin kennt; andere (z. B. Shift_JIS, UTF-16LE als eigener
  Name) werden wie bei iconv mit Fehler abgelehnt.
- **B1 Ressourcen nicht unter /sys/netsurf (22.11.):** Geplant war, die
  Ressourcen (CSS, Messages, Willkommensseiten) beim ersten Start in den
  Flash zu schreiben. Am Board hängte sich das System dabei dreimal auf
  (Watchdog) bzw. stürzte ab (beide Kerne: ein Kern las eine in den
  Speicher abgebildete Flash-Tabelle, der andere lief auf einem
  PSRAM-Stapel, während ≈200 KB geschrieben wurden). Ersatz: Ressourcen
  im Firmware-Abbild, gelesen über das schreibgeschützte Dateisystem
  /nsres – kein Schreiben, immer passend zur Firmware. Die eigentliche
  Ursache (Flash-Schreiben parallel zu abgebildetem Lesen bei Code im
  PSRAM) ist inzwischen gefunden und mit der Flash-Sperre behoben (STATUS
  „Flash-Absturz“); /nsres bleibt trotzdem – schneller Start, kein
  Verschleiss, Ressourcen passen immer zur Firmware.
- **B1 Browser-Heap 10 MB statt 16 MB (22.11.):** Die Firmware ist um
  2,7 MB gewachsen (NetSurf + QuickJS); der Code liegt am Board als Kopie
  im PSRAM (XIP), der leere Desktop lässt 13,5 MB frei. Mit 16 MB starteten
  die Desktop-Dienste nicht mehr (Fehler 30). Folge: Solange der Browser
  läuft (2,1 MB frei), starten grosse Programme wie Einstellungen und
  Bildbetrachter (je 12 MB) nicht. JS-Grenze am Board 4 MB.
