# STATUS – ESP32-OS

Stand: 24.11.2026 – **Gesamtauftrag 24.11.** (ersetzt den vom 23.11.
nachts): Teil A erledigt (v1.0.2 veröffentlicht), Teil B Browser, Teil C
v1.1 (Konsole, WLAN 6, Standby), Teil D Vorsprung (Absturzschutz,
Dateimanager, Shell, Medienplayer, App-Store); nach B, C, D je eine
Stabilitätsrunde und ein Release-Vorschlag. Plan: PLAN.md, „Gesamtauftrag
24.11.“ und Abschnitte I–L.
Repo: dieser Ordner, Branch `esp32-os`; öffentlich wird nur `main`
gepflegt (eigener Anfangscommit, siehe CLAUDE.md).

## Jetzt

- **25.11., 2 Uhr:** S5 abgehakt (Board-Fotos aller 18 Programme vorher/
  nachher, `docs/nachweise/s5/`). Browser-Absturz behoben, Lecksuche lief
  danach ohne Absturz durch (3,6 KB in zwei Runden, Zuordnung offen).
  Gepusht: main dcf62e4. Läuft: M4 Lauf 6 (bis ~3:25). Danach: C3/D5
  Board-Nachweise. Wochenlimit 81 %.

- **25.11., 1 Uhr:** M4 Lauf 5: 0 Abstürze in 75 min, intern aber −8 KB.
  Lecksuche zweimal durch denselben Browser-Absturz (nach 629 s) beendet –
  Ursache gefunden und behoben (statische Sperre im Heap des Browsers,
  PLAN M4). Läuft: Tests + Build; danach Lecksuche erneut, S5-Board-Fotos,
  M4 Lauf 6.

- **24.11., 23:40:** Gepusht (main aae6bec): D1 Shell (Board-Foto), S5
  Fensterränder (PC-Test 21 Programme), Review-Befunde behoben, kleinere
  Dienst-Heaps. Läuft: M4 Lauf 5 (bis ~0:10; Browser startet jetzt, 2
  Startfehler nur wenn Einstellungen/Musik noch offen – Pool voll). Danach:
  S5 Board-Fotos vorher/nachher, M4-Auswertung.

- **24.11., 23 Uhr – Nutzer schläft, ich arbeite allein weiter.** Stand:
  C1 und D1 am Board nachgewiesen (Fotos `docs/nachweise/c1/`), S2
  abgehakt (17,8 → 25,2 Bilder/s), S3 gemessen (Grenze 55-Hz-Takt,
  Abweichung). Läuft: M4 Lauf 5 (Browser passt jetzt in den Pool), Review
  des Diffs. Danach: Commit/Push, C3/D4/D5-Board-Nachweise, M5.

- **Zwischenstand 24.11., 22:45:** M4 Lauf 4 (m4e) nach 25 min
  abgebrochen: 0 Abstürze, intern stabil (220 → 217 KB), aber der Browser
  startete nie („Fehler 30“) – er braucht 10,6 MB am Stück, das grösste
  freie Pool-Stück war 10,5 MB. Behoben: Dienste mit kleinerem Heap
  (@tastatur 256, @taskbar 256, @menu 384 KB; Spitzen am Board 54–56 KB).
  Neu am PC: **D1 Shell** – `>`, `>>`, `<`, Pipes, cat/grep/wc/head/tail/
  sort, Skripte; zwei Fehler der Original-Shell behoben (`ls -1`,
  Skript-Erkennung). **T1** ColibriOS geprüft (siehe Offene Fragen).
  Läuft: S3-Bildrate am Board (nachher), danach vorher, C1/D1-Foto, M4 Lauf 5.

- **Zwischenstand 24.11., 22:15:** M4 Lauf 3 (Pool, 4 MB Systemreserve)
  abgebrochen: dem System blieben nur 0,4–1,3 MB PSRAM (Fensterpuffer,
  WLAN, Kern) – zu knapp für neue Fenster. Jetzt 6 MB Reserve, dafür
  @icon 2,5 statt 4 MB (Spitze am Board 948 KB, PC-Test mit Hintergrundbild
  grün): System 3,4 MB frei, Pool 11,1 MB frei (Browser braucht 10,6).
  Läuft: M4 Lauf 4 (m4e). Fertig am PC: S1 Taskleiste per Ereignis (Test
  < 150 ms), S2 Dateimanager blättert nur die Liste (Test: gleicht dem
  Vollbild), S4 abgehakt. Vorbereitet: S3 Bildrate je Phase am Board
  (`s3_lauf.ps1`, Auswertung `s3_auswertung.py`), vorher/nachher für S2.

- **Zwischenstand 24.11., 22 Uhr (Vorrang M):** Programm-Pool gebaut
  (`kern::bereichspool`, PC-Test: 200 Runden Menü/Musik/PDF ohne
  Zerstückelung; Firmware reserviert ihn beim ersten Programmstart, Stand in
  der Speicherwacht). Läuft am Board: M4 Dauertest Lauf 3 mit Pool. Bereit:
  Lecksuche `leck_lauf.ps1` (Heap-Spur über 2 Runden), läuft danach.

- **M4 Dauertest Lauf 2 (75 min): 0 Abstürze.** Interner RAM 221 → 213 KB
  (kleines Leck, ~1 KB je 10 min) und 13 Startfehler grosser Programme
  wegen zerstückeltem PSRAM → M4 noch nicht erledigt. Nächstes: Leck mit
  der Heap-Spur suchen, Programmbereiche aus einem festen Pool.
- **Neu: Abschnitt S (Bedienung)** – Taskleiste im selben Bild, Dateimanager
  flüssig, fps-Messung der Oberfläche, Ersteinrichtung per X erledigt.
  Kommt nach den Speicher-Punkten M; Stabilität hat Vorrang. Läuft gerade:
  M4 Dauertest am Board (71 min).

- **24.11. abends: Board wieder angeschlossen** – Board-Tests laufen in der
  Reihenfolge von Vorrang M. Läuft: M1 Messlauf (`m1_lauf.ps1`, Mess-Build
  mit Stabilitäts-Autopilot, danach normale Firmware). Statisch (aus dem
  ELF): Variablen der C-Programme jetzt im PSRAM – statischer interner RAM
  199 → 166 KB. Erste Messung: unter Last nur 86 KB intern frei, grösster
  Block 29 KB (Ziel ≥ 100 KB); grösster Brocken: Bildstreifen der Anzeige
  (125 KB) – sie kommen im Desktop-Modus in den PSRAM. Der Startknopf
  schliesst ein offenes Menü jetzt (der Autopilot traf vorher dein
  Quake-Symbol). Hinweis: Die Ersteinrichtung startet bei dir bei jedem
  Einschalten – einmal bis „Fertig“ durchtippen, dann nicht mehr.
- M1-Tabelle (Messlauf 3, 40 s nach Start, intern belegt): main 223 KB,
  render1 63 KB, ipc0 45 KB, pthread (WLAN/lwIP) 22 KB, Rest < 5 KB je Task.
  Läuft (M2 Schritt 1): Bildstreifen im Desktop-Modus in den PSRAM
  (`hw::PIXEL_INTERN`), Messlauf `m2a`. Nicht committet (Tests stehen aus):
  Menü-Umschalter, Linker-Fragment der C-Programme, Autopilot als Datei,
  Bericht je Task, PIXEL_INTERN.

- **VORRANG seit 24.11. abends: Stabilität und Speicher (PLAN, Abschnitt
  M).** Teil C und D pausieren. Reihenfolge M1 Messen → M2 interner RAM
  leer → M3 feste Grenzen → M4 Dauertest 60 min → M5 v1.0.3 (dein OK) →
  M6 Speicher für den Browser. Ohne Board baue ich Messung, Umzüge ins
  PSRAM, Grenzen und den Dauertest-Autopiloten vor; Messwerte und der
  Dauertest folgen, sobald das Board zurück ist.

- **Board ist weg** (seit 24.11. 06:20). Kein Zugriff auf COM3, bis du
  „Board ist wieder da, mach weiter“ schreibst. Bis dahin PC-Arbeit und
  Vorbereitung der Board-Tests (Liste unten, „Board-Tests“).
- Ohne Board erledigt (24.11.): Review Teil B (3 Befunde behoben), BR3
  lexbor geprüft, Fehlerbericht Fall 1 und 2 behoben (PC-Test), Stabilitäts-
  runde vorbereitet (PC-Test + 38-min-Autopilot), C1 Konsole am PC fertig
  (Shell, Matrix, Touch-Test), C2 C6-Firmware gebaut und Update vom P4 über
  OTA ohne Adapter vorbereitet (mit automatischem Rückweg, Review-Befunde
  behoben), C3 Standby vorbereitet, D5 Absturzschutz gebaut (nur am Board
  prüfbar, Review-Befunde behoben), D4 Papierkorb im Dateimanager (PC-Test
  per Touch). Alles mit Board-Skript in der Liste unten.
- Als Nächstes ohne Board: D1 Shell ausbauen, dann D2, D3.
- 24.11. vormittags: Meldung „Verbindung fehlgeschlagen (-28674)“ bei dir
  unterwegs = ESP_ERR_HTTP_CONNECT (Server nicht erreichbar, auch im
  Browser) – vermutlich das WLAN vor Ort (Anmeldeseite/kein Internet). Die
  Meldung ist jetzt lesbar (Commit „Update: lesbare Meldung …“, kommt mit
  dem nächsten Update). Geplant: Ersatz-Namensserver und Anzeige von
  IP/Gateway/DNS in den Einstellungen (siehe „Offene Fragen“).
- Wochenlimit beim Start des Gesamtauftrags: 72 % (Grenze 90 %).

## Dein Fehlerbericht vom 24.11. (Stand)

- **Fall 2 (Home/Startknopf bei offenem Browser → Neustart, „@menu“):** am
  PC nicht nachstellbar (15 Mal ohne Fehler) – es ist der Speicher am Board.
  „Illegal instruction“ ist auf dem P4 die Form, in der ein `abort()`
  erscheint: Das Menü bekam keinen Speicher für sein Fenster, und Rust
  bricht dann die ganze Firmware ab. Den PSRAM hatte der Browser gefüllt
  (empfangene Daten ohne Obergrenze). Behoben: Fenster werden nur noch
  angelegt, wenn der Speicher reicht – sonst endet nur dieses Programm mit
  „Speicher voll“; der Browser puffert höchstens 512 KB und lässt 1,5 MB
  PSRAM frei. Am PC getestet; Board-Nachweis folgt.
- **Fall 1 (Browser lädt nichts):** Statt ESP_ERR_HTTP_CONNECT zeigt der
  Browser jetzt die Ursache: DNS-Fehler, „keine Verbindung ins Internet –
  Anmeldeseite oder Router ohne Internet?“ (mit IP, Router, DNS) oder
  TLS-Fehler. Dazu Ersatz-Namensserver 1.1.1.1 und der DNS-Server in den
  Einstellungen. Die neue Meldung zeigt am Board, woran es in deinem WLAN
  liegt – das kann ich ohne Board nicht sehen.
- **Fall 3 (Browser/Menü oft):** Stabilitätsrunde mit Autopilot ist
  vorbereitet (38 min) und läuft als Erstes, wenn das Board zurück ist.
- Neue Firmware kommt mit v1.0.3 (OK in „Offene Fragen“, sobald SB grün ist).

## Board-Tests (Reihenfolge, sobald das Board wieder da ist)

**Zuerst (Vorrang M, Stabilität und Speicher):** M1 `m1_lauf.ps1 -Ziel
…\m1` (Mess-Build, Tabelle der Verbraucher), dann nach den Umzügen ins PSRAM
(M2) M4 `m4_lauf.ps1 -Ziel …\m4` (71 min, 0 Abstürze, interner RAM am Ende
= Anfang). Danach v1.0.3 (M5) und erst dann die Liste unten (BR1, C, D).

Jeder Test mit Nachweis in PLAN.md; Skripte im Repo unter
`esp32/werkzeuge/board/` (`-Ziel` = Ordner für Log, Fotos, Auswertung;
jedes Skript schreibt am Ende wieder die normale Firmware aufs Board).

1. **Grundcheck:** Firmware vom Stand `esp32-os` (Entwicklungsschalter
   aus) per Kabel schreiben, Einstellungen/WLAN bleiben; Start, WLAN,
   Update-Prüfung (lesbare Fehlermeldung). **Fehlerbericht 24.11.:** Browser
   offen + Startknopf 20 Mal (kein Neustart; wenn Speicher knapp: nur
   „Speicher voll“), Browser in einem WLAN ohne Internet (Meldung prüfen),
   Log mit freiem PSRAM bei offenem Browser.
2. **BR1 Messlauf 30 Seiten:** `br_lauf.ps1 -Ziel …\br1` (Seiten in
   `br_seiten.txt`, je Seite frischer Browser 60 s, Foto `bild_<i>.png`,
   Tabelle `tabelle.md` aus `br_auswertung.py`); Ergebnis/Grund nach den
   Fotos ausfüllen → `docs/browser_seiten.md`.
3. **BR2 Runden:** nach jeder Verbesserung dieselbe Liste
   (`br_lauf.ps1 -Ziel …\br2_n`), Vorher/Nachher. Erster Kandidat aus den
   alten Daten: Browser-Heap 10 MB ist auf grossen Seiten voll (Spitze
   10,4 von 10,6 MB, HTML bei 512 KB abgeschnitten) – am Board den freien
   PSRAM mit laufendem Browser messen und, wenn Platz ist, den Heap
   vergrössern.
4. **SB Stabilitätsrunde:** `stabil_lauf.ps1 -Ziel …\sb1` (38 min,
   Autopilot aus `stabil_autopilot.txt`, Auswertung `auswertung.txt`);
   jeden Absturz nachstellen und beheben, dann wiederholen.
5. **C1 Konsole:** `c1_lauf.ps1 -Ziel …\c1` (Autopilot aus dem PC-Test
   `konsole_touch.rs`: Konsole über das Menü, `ver`, `touch neu`, `ls`,
   `rm neu` auf der Bildschirmtastatur, Kreuz; Matrix öffnen und schliessen;
   Fotos nach 41 s und 66 s).
6. **C2 WLAN 6:** Messung vorher (`werkzeuge/durchsatz.py`,
   Verbindungsaufbau aus dem Log); dann `c2_lauf.ps1 -Ziel …\c2` (C6-Update
   vom P4 über OTA, ohne Adapter; erwartet „C6-Update: fertig“ und danach
   Versionen gleich 3.1.0; Rückweg automatisch); W5 iTWT-Setup beim
   Abdunkeln, Messung nachher. Lehnt die Werks-Firmware OTA ab: C2 wartet
   auf einen Adapter (`c6_flashen.ps1`), der Rest geht weiter.
7. **C3 Standby:** `c3_lauf.ps1 -Ziel …\c3` (Matrix, Ruhezustand 90 s,
   Wecken per Touch, Fotos, Log „aufgeweckt … WLAN verbunden“).
8. **SC Stabilitätsrunde** (mit Konsole und Standby).
9. **Teil D:** D5 Absturzschutz `d5_lauf.ps1 -Ziel …\d5` (drei Mal
   absturz_test: Schreiben 0, Lesen 4, unerlaubter Befehl – erwartet drei
   abgefangen, kein Neustart, Foto mit Meldung); D4 Papierkorb: Foto
   Dateimanager → Papierkorb (Board-Foto per Autopilot wie C1); dann D1,
   D2 (Ton), D3; danach **SD Stabilitätsrunde**.

## Offene Fragen

- **S5 „Editor-Fenster“:** Gefunden und behoben ist der abgehackte obere
  Rand bei PasswordGen, TinyHashView und Beispiel-GUI (alte
  KolibriOS-Programme zeichneten in die höhere Touch-Titelleiste). Welches
  Fenster meinst du mit „Editor“? Die Notizen haben eine vollständige
  Titelleiste, liegen aber wie Dateimanager und Einstellungen bündig am
  oberen Bildschirmrand (y = 0). Wenn das stört: Vorschlag, alle Fenster
  mit einigen Pixeln Abstand zum oberen Rand zu öffnen – sag Bescheid.

- ~~**T1 ColibriOS**~~ – beantwortet (24.11. spät): nach Teil D nur die
  Wetter-App übernehmen (Lizenz vorher prüfen, Touch, Standort in den
  Einstellungen, Daten über WLAN); passt sie nicht, eine eigene mit einem
  freien Wetterdienst ohne API-Schlüssel – PLAN T2. (alter Text:)
- **T1 ColibriOS (geprüft 24.11. abends, nur Webseiten gelesen):**
  ColibriOS ist das umbenannte KolibriOS (Commit „Rename KolibriOS to
  ColibriOS across the tree“, 31.10.2026; Quellen
  git.colibrios.org/colibrios/colibrios, Ports git.colibrios.org/colibrios/ports).
  Lizenz: `COPYING.TXT` der Quellen und `LICENSE` der Ports sind die
  GPL-2.0 (Juni 1991); die Webseite sagt nur „GPL“. Einzelne Programme
  tragen eigene Köpfe (wie bei kmatrix, das GPL-3.0 war) – jedes Programm
  vor dem Übernehmen einzeln prüfen. Mit unserem GPL-2.0-only verträglich
  ist „GPL“ ohne Version, „GPL-2.0“ und „GPL-2.0-or-later“, nicht
  GPL-3.0. build.colibrios.org/ports war nicht erreichbar (DNS); das
  Ports-Repo enthält ein C/C++-SDK (newlib, SDL2, Qt5) und Ports grosser
  Programme (SDL2-Spiele, Quake 2/3, Wesnoth, NetSurf, Transmission) – die
  brauchen SDL/C++ bzw. x86 und passen nicht zu unserem ksys.h-Weg.
  Kandidaten im Quellbaum (Ordner gesehen, C-Quellen und ksys.h noch nicht
  geöffnet): programs/other – Timer, Weather, calc, table, period,
  planet_v, rainbow, mousepos, pipet (PasswordGen und TinyHashView haben wir
  schon); programs/games – 2048, 15, flood-it, c4, checkers, LaserTank,
  sudoku, tetris, snake, xonix. Viele Programme dort sind FASM oder C--
  (x86) und lassen sich nicht übernehmen. **Frage:** Soll ich nach Teil D
  einige dieser C-Programme (Vorschlag: Timer, 2048, flood-it, sudoku)
  einzeln prüfen und übernehmen? Dafür müsste ich den Quellbaum
  herunterladen (git clone, ca. 1–2 GB, nur in den Scratch-Ordner). Ohne
  dein OK mache ich das nicht; bis dahin bleibt es bei dieser Liste.

- ~~**C2 Adapter**~~ – erledigt (24.11.): Du willst den Weg über OTA ohne
  Adapter. Geprüft: sehr wahrscheinlich ja (die Werks-Firmware ist ein
  älterer esp_hosted-Koprozessor; die OTA-Befehle sind älter als 2.6, nur
  „activate“ fehlt dort – ältere schalten bei „end“ um). Sicher zeigt es
  erst das Board; der Versuch ist ungefährlich. Umgesetzt mit Rückweg
  (Probezeit 90 s, sonst zurück zur alten Firmware) – Details in PLAN C2
  und Abweichungen. (alter Text:) **C2 (eigene C6-Firmware):** Die Firmware ist gebaut und geprüft, das
  Schreiben ist vorbereitet (mit Sicherung und Rückweg). Das allererste Mal
  geht es nur mit einem **USB-UART-Adapter (3,3 V) an den C6-Pins** (TX,
  RX, GND, dazu BOOT/GPIO9 und EN) – die alte C6-Firmware kann kein Update
  über SDIO. Hast du so einen Adapter und darf ich das machen, wenn das
  Board zurück ist? Bis dahin: C2-Messung „vorher“ läuft trotzdem, der Rest
  wartet. (Vernünftigste Lösung ohne Antwort: C2 bleibt nach der
  Vorher-Messung offen, kein Eingriff am C6.)

- ~~**kmatrix (C1) – Lizenz:**~~ Entschieden (24.11.): nicht einbauen,
  eigene „Matrix“ bleibt (Abweichung in PLAN). (alter Text:) kmatrix steht unter GPL-3.0-or-later, unsere
  Firmware enthält NetSurf unter GPL-2.0-only. Beides zusammen in einer
  Firmware ist nicht erlaubt (GPLv3 verlangt v3 für das Ganze, NetSurf
  erlaubt nur v2). Vernünftigste Lösung (umgesetzt): ein eigenes Programm
  „Matrix“ (GPL-2.0) mit derselben Idee, das die Konsole genauso nutzt
  (Farben per Esc-Sequenz, Schreibmarke setzen, Taste beendet). kmatrix
  selbst ginge nur als eigenes Abbild ohne NetSurf (wie Doom/Quake) –
  sag Bescheid, falls du das willst.

- **BR3 lexbor statt libdom/hubbub (geprüft 24.11., nicht eingebaut):**
  Empfehlung: **nicht einbauen.** Gründe:
  1. *Lizenz:* lexbor steht unter Apache-2.0, NetSurf unter GPL **nur
     Version 2** („version 2 of the License“, ohne „oder später“). Nach
     Auffassung der FSF ist Apache-2.0 nur mit GPLv3 vereinbar, nicht mit
     GPLv2 (Patentklausel). Eine Firmware mit NetSurf und lexbor dürften wir
     so nicht verteilen; NetSurf umlizenzieren können wir nicht.
  2. *Aufwand:* NetSurf ist eng an die libdom-Schnittstelle gebunden –
     25 Quelldateien mit 1260 Aufrufen (130 verschiedene `dom_*`-Funktionen),
     dazu die CSS-Auswahl von libcss (arbeitet auf libdom-Knoten) und unsere
     QuickJS-Anbindung (4 Dateien, 183 Aufrufe). Ein Ersatz hiesse, den
     HTML-Inhalt, die CSS-Auswahl und die JS-Anbindung neu zu schreiben –
     Monate, nicht Tage.
  3. *Speicher:* lexbor wirbt mit Geschwindigkeit, nicht mit kleinerem
     Speicherbedarf; eine belastbare Ersparnis gegenüber libdom ist nicht
     belegt (am Board nicht messbar ohne den Umbau). Unsere Speicherprobleme
     lagen bisher an Lecks und fehlenden Prüfungen (BR1/BR2), nicht an
     libdom selbst; die Seiten laden mit 32 MB PSRAM.
  Wenn du es trotzdem willst, bräuchte es eine Ausnahme der Lizenzfrage
  (z. B. Rechtsrat) – bis dahin bleibe ich bei libdom/hubbub.

- **Netz unterwegs (24.11.):** Ich baue einen Ersatz-Namensserver (1.1.1.1,
  falls das WLAN keinen liefert) und zeige IP/Gateway/DNS in den
  Einstellungen an – vernünftigste Lösung, ohne Rückfrage. Was zeigten bei
  dir `http://neverssl.com` und `http://1.1.1.1`?

- ~~OK für das Release v1.0.2?~~ Erledigt: OK erhalten, veröffentlicht 24.11.
  (alter Text:) Fertig vorbereitet (`release/v1.0.2/`,
  Text `docs/release/v1.0.2.md`, Prüfungen grün): Quake „New Game“,
  Tastatur-Rückmeldung, Browser reisst die Firmware nicht mehr mit.
  Veröffentlichen (Tag, Release, danach Web-Flasher von main) mache ich
  erst nach deinem OK.

## Release v1.0.1 (23.11. abends)

- **Veröffentlicht:** https://github.com/TheUnkn0wnC0DE/ESP32-OS/releases/tag/v1.0.1
  – Einstellungen mit 6 statt 12 MB Heap. Dein Board läuft mit 1.0.1
  (per Kabel nach esp32os_a geschrieben, Einstellungen und WLAN bleiben);
  von dir bestätigt: die Einstellungen öffnen.
- **Update von 1.0:** In 1.0 öffnen die Einstellungen nach dem Einschalten
  nicht, damit ist auch „Nach Updates suchen“ nicht erreichbar → 1.0.1 per
  Kabel (espflash oder Web-Flasher), steht im Release-Text.
- **Web-Flasher** liefert 1.0.1; von dir am Board getestet, Platz A danach
  bitgleich zum Release (`docs/nachweise/o8/o8_webflasher.txt`). Die
  README nennt ihn jetzt mit URL. O8 ist erledigt.

## Release v1.0 (23.11. abends)

- **Veröffentlicht:** https://github.com/TheUnkn0wnC0DE/ESP32-OS/releases/tag/v1.0
  (Tag v1.0, 9 Dateien, SHA256SUMS). Abbilder ohne WLAN-Daten, ohne lokale
  Pfade (Rust `--remap-path-prefix`, NetSurf-C `-ffile-prefix-map`; die
  Release-Prüfung verwirft Abbilder mit Pfad oder Benutzername); Suche nach
  Benutzername und echtem Namen in allen Dateien: 0 Treffer.
- **Firmware nicht auf main** (deine Wahl): die Push-Prüfung fand den
  Suchbegriff des Sicherheitsberichts als Cookie-Attributnamen im Abbild;
  der Web-Flasher-Workflow holt die Dateien aus dem Release.
- **Web-Flasher:** https://theunkn0wnc0de.github.io/ESP32-OS/ (ESP Web
  Tools 10.4.0, Stub für Chip-Revision v1.x). Der Board-Test braucht dich:
  der eingebaute Browser hat keine Portauswahl („No port selected“).
- **Manuell mit espflash** aus den Release-Dateien: am Board gestartet
  (Desktop, WLAN). esp32os.bin braucht mit Standard-Baudrate 5 min.
- **U1 nachgewiesen:** 0.9 → v1.0 über WLAN per Touch, Platz B bitgleich
  zum Release (`docs/nachweise/u1/`).
- **Fehler in v1.0 gefunden:** „Einstellungen“ öffnet nach dem Hochfahren
  nicht (12 MB am Stück, grösster PSRAM-Block 12 288 KB). Behoben (6 MB),
  braucht ein Bugfix-Release v1.0.1 – nur mit deinem OK.

## README-Wünsche und grosse Fotos (23.11. spät)

- **README:** Foto-Platzhalter entfernt, Titelbild ist ein grosser
  Desktop-Screenshot (`docs/images/desktop.png`), darunter die vier kleinen.
  Quick Start beginnt mit „No binary release yet, build from source“; der
  Web-Flasher wird erst erwähnt, wenn er über GitHub Pages erreichbar ist.
  TikTok als Link.
- **Bildbetrachter, grosse Fotos (P2-Nachtrag):** JPEG/PNG stückweise
  gelesen und beim Dekodieren verkleinert. Am Board mit echtem 12-MP-Foto
  (Wikimedia Commons, CC0, iPhone 14) per Touch nachgewiesen:
  12 MP 5,9 s (1/4), progressiv 7,3 s (1/8), 48 MP 11,4 s (1/8);
  Nachweise `docs/nachweise/p2_gross/`. Erst 8,5 s → schneller gemacht
  (Lesepuffer 256 KB, 32-Bit-Bitleser, Runden ohne Bibliotheksaufruf,
  Farbumrechnung vorberechnet; Ergebnis bitgleich).
- **Bremse SD-Karte:** die 236-GB-Karte liefert am Board nur 2,3 MB/s
  (4 Bit, 40 MHz eingestellt; Pufferausrichtung egal). Am SD-Treiber
  nichts geändert (Karte mit deinen Daten). Funktion 70 öffnet je Aufruf
  die Datei neu und sucht die Stelle (9–18 ms je Aufruf).
- **Entwicklungswerkzeug:** `ESP32OS_DATEI_EMPFANG=1` +
  `esp32/firmware/werkzeuge/datei_senden.py` legen Testdateien per WLAN
  nur in `ESP32-OS/Bild_Test` (Name geprüft, `../` abgelehnt).
- Die README-Kopie im Abschnitt „Aufräumen“ unten zeigt den Stand vor
  diesen Änderungen.

## Aufräumen vor dem Release (23.11. abends)

Auftrag: oberste Ebene nur mit dem, was ein Besucher braucht; Entwicklungs-
dokumente nach `docs/development/`; Fremdes und Überholtes einordnen oder
entfernen; README je wichtigem Ordner; neue README. Entscheidungen:

- **Verschoben:** `AUFTRAG-ESP32-OS.md`, `PLAN.md`, `STATUS.md` →
  `docs/development/`. `CLAUDE.md` → `.claude/CLAUDE.md` (Claude Code lädt
  die Datei dort). Sie ist nicht mehr im Repo: die Push-Prüfung verbietet
  alles unter `.claude/` auf `main`, und die Arbeitsregeln (mit Push-Regeln
  und Namen privater Dateien) nützen Besuchern nichts. Ihre Geschichte
  bleibt im lokalen Zweig `esp32-os`.
- **Entfernt** (bleibt in der Git-Geschichte):
  - `pudding/` (PC-Fassung von Pudding-Panik) mit `golden/` (ihre
    Referenzbilder), `screenshots/`, `docs/02_kekse.png`, `06_bruecke.png`,
    `09_ziel.png`, `wackelpudding.gif`, `spielen.cmd`, `bauen_nativ.cmd` –
    alles nur für die PC-Fassung. ESP32-OS hat das Spiel in `esp32/game`
    und die PC-Simulation in `esp32/sim`.
  - `mcu_demo/` (Cortex-M7-Beispiel), gehört nicht zu ESP32-OS.
  - `esp32/docs/` (Bilder, GIF, WAV, Testbild): Ausgaben der PC-Simulation,
    jederzeit neu erzeugbar (`pudding-sim shots|gif|wav|testbild`).
- **Bleibt:** `spatz/` (3D-Engine, von fast allen Crates in `esp32/`
  benutzt) mit dem Workspace der Wurzel (`Cargo.toml`, jetzt nur `spatz`);
  `rustfmt.toml` (Format-Regeln für alle Crates); `LICENSES/`.
- **Neu:** `CHANGELOG.md`; `.gitattributes` (Shell-Skripte mit LF, sonst
  bricht bash unter Windows nach dem Auschecken ab); `docs/images/` (Bilder
  der README, aus den Board-Nachweisen); `docs/nachweise/o8/` (Grössen und
  Prüfsummen des Release-Pakets als Beleg für die README);
  `esp32/werkzeuge/alle_tests.sh` (Testlauf aller Suiten, lag bisher nur
  lokal); README in `docs/`, `docs/development/`, `docs/nachweise/`,
  `esp32/`, `spatz/` und jedem Crate unter `esp32/`; die Gehäuse-README
  auf Englisch.
- **Unterwegs korrigiert:** Release-Text versprach eine Stoppuhr (gibt es
  nicht) und eine „German“-Tastatur (es sind Schweizerdeutsch und US).
- Nachweis: alle Testsuiten grün mit `esp32/werkzeuge/alle_tests.sh`
  (241 Tests in 8 Suiten, FEHLER=0), `spatz` 36 Tests grün, Firmware-Bau aller Abbilder
  grün (`bauen_flashen.ps1 -NurBauen`).
- Lokal und nicht im Repo (gitignored): `extern/`, `release/`,
  `sicherung/`, `wlan.local`, `*.local.*`.

### Ordnerbaum (zwei Ebenen, nur versionierte Dateien)

```text
ESP32-OS/
├── docs/
│   ├── development/
│   ├── flasher/
│   ├── images/
│   ├── nachweise/
│   ├── release/
│   └── README.md
├── esp32/
│   ├── desktop/
│   ├── doom/
│   ├── firmware/
│   ├── game/
│   ├── gehaeuse/
│   ├── kern/
│   ├── kolibri/
│   ├── konsole/
│   ├── netsurf/
│   ├── quake/
│   ├── sim/
│   ├── werkzeuge/
│   ├── wuerfelwelt/
│   ├── Cargo.lock
│   ├── Cargo.toml
│   └── README.md
├── LICENSES/
│   ├── Apache-2.0.txt
│   ├── CC0-1.0.txt
│   ├── GPL-2.0-netsurf-COPYING.txt
│   ├── GPL-2.0.txt
│   ├── MIT-nanomp3.txt
│   ├── MIT-netsurf-libcss.txt
│   ├── MIT-netsurf-libdom.txt
│   ├── MIT-netsurf-libhubbub.txt
│   ├── MIT-netsurf-libnsbmp.txt
│   ├── MIT-netsurf-libnsfb.txt
│   ├── MIT-netsurf-libnsgif.txt
│   ├── MIT-netsurf-libnsutils.txt
│   ├── MIT-netsurf-libparserutils.txt
│   ├── MIT-netsurf-libwapcaplet.txt
│   ├── MIT-quickjs-ng.txt
│   ├── MIT-utf8proc.md
│   ├── README.md
│   └── Zlib.txt
├── spatz/
│   ├── examples/
│   ├── src/
│   ├── Cargo.toml
│   ├── LICENSE-APACHE
│   ├── LICENSE-MIT
│   └── README.md
├── .gitattributes
├── .gitignore
├── Cargo.lock
├── Cargo.toml
├── CHANGELOG.md
├── LICENSE
├── README.md
├── rustfmt.toml
└── THIRD_PARTY_NOTICES.md
```

### README.md

````markdown
# ESP32-OS

A KolibriOS-compatible OS running natively on an ESP32-P4 touch console.

<!-- Photo of the board in the hand: docs/images/board.jpg (to be added) -->

| | |
|---|---|
| ![Desktop](docs/images/desktop.png) | ![Web browser](docs/images/browser.png) |
| ![PDF viewer](docs/images/pdf-viewer.png) | ![Würfelwelt](docs/images/wuerfelwelt.png) |

ESP32-OS re-implements the KolibriOS system functions in Rust instead of emulating x86. Original KolibriOS C programs are compiled for the RISC-V CPU and run unchanged. On top of that run a touch desktop, system programs, the NetSurf web browser and four games.

## Facts

Measured on the board (Waveshare ESP32-P4-WIFI6-Touch-LCD-5, chip revision v1.x, 360 MHz):

| | Value | Evidence |
|---|---|---|
| OS image | 5.4 MiB (5,687,168 bytes), 91 % of its 5.9 MiB flash slot | [o8_release_paket.txt](docs/nachweise/o8/o8_release_paket.txt) |
| Power-on to desktop | 3.5 s | [o4_1_apps_log.txt](docs/nachweise/o4/o4_1_apps_log.txt) |
| Browser scrolling | 32 frames/s (Wikipedia article) | [o1_scroll_log.txt](docs/nachweise/o1/o1_scroll_log.txt) |
| PDF page (A4 with photo, text, shapes) | 0.24 s | [z1_board_log.txt](docs/nachweise/z1/z1_board_log.txt) |
| Games | Pudding-Panik 55, Doom 55, Würfelwelt 27.6, Quake 27.5 frames/s | [o4/](docs/nachweise/o4/) |
| Wi-Fi throughput (TCP) | 11.6 Mbit/s to the board, 9.8 Mbit/s from the board | [w8_durchsatz.txt](docs/nachweise/w8_durchsatz.txt) |
| Endurance | 1 h with browser and Wi-Fi, no reset | [o5_auswertung.txt](docs/nachweise/o5/o5_auswertung.txt) |

## Quick start

1. Download the files of the latest release from the Releases page, or open the web flasher (`docs/flasher/`, Chrome or Edge) and skip to step 4.
2. Connect the board's USB TO UART port.
3. Flash with [espflash](https://github.com/esp-rs/espflash):
   ```
   espflash write-bin 0x2000   bootloader.bin
   espflash write-bin 0x8000   partition-table.bin
   espflash write-bin 0x10000  otadata-leer.bin
   espflash write-bin 0x20000  esp32os.bin
   espflash write-bin 0xC00000 doomapp.bin
   espflash write-bin 0xE00000 quakeapp.bin
   ```
4. Optional: copy the shareware files `DOOM1.WAD` to `ESP32-OS/doom/` and `pak0.pak` to `ESP32-OS/quake/` on the microSD card.

No binary release has been published yet. Until then, build from source as described below.

## Features

| Area | What works |
|---|---|
| Kernel | KolibriOS system functions in Rust, one heap per program, crash isolation with error dialog, compositor with dirty rectangles, two firmware slots with automatic rollback |
| KolibriOS programs | Original C programs, unchanged: PasswordGen, TinyHashView and the clayer samples; native box_lib, libimg, proc_lib, libini, network |
| Desktop | Icons, taskbar with Wi-Fi signal and clock, start menu, tap, long press, swipe and pinch, on-screen keyboard (Swiss German, US), English and German interface, first-run wizard |
| Programs | File manager (microSD and internal flash), image viewer (JPEG in hardware, PNG, GIF, BMP), PDF viewer, settings, notes, calculator, clock with alarm and timer, calendar, MP3 player, task manager, screenshot tool |
| Wi-Fi 6 | Via the ESP32-C6: scan and connect by touch, remembered networks, time via NTP |
| Browser | NetSurf 3.11, HTTPS with certificate check, tabs, bookmarks, history, downloads, zoom, forms, JavaScript (QuickJS-ng: DOM, events, timers, fetch, cookies, localStorage) |
| Games | Pudding-Panik, Würfelwelt, Doom, Quake, started from the desktop |
| Updates | Firmware update over Wi-Fi from GitHub releases with SHA-256 check and rollback (not yet tested end to end, needs the first release) |

## Limitations

- Wi-Fi passwords are stored unencrypted in flash (no flash encryption, no eFuses).
- The browser has 10 MB of memory. Large web applications do not load; some web APIs are missing.
- The PDF viewer shows text in the system font, opens files up to 4 MB and does not decode CCITT, JBIG2 or JPEG 2000 images.
- The image viewer opens images up to about 0.85 megapixels.
- All programs share one address space. A faulty program is stopped and reported, but can still disturb the system.
- No standby, no battery gauge, no camera support yet (planned for 1.1).
- Release images combine GPL-2.0 code (NetSurf, KolibriOS) with Apache-2.0 code (ESP-IDF). Whether they may be distributed in this form is being clarified; the source code is not affected.
- The build runs on Windows only.

Full list with reasons: `docs/development/PLAN.md`, section "Abweichungen" (German).

## Hardware

Waveshare ESP32-P4-WIFI6-Touch-LCD-5 ([documentation](https://docs.waveshare.com/ESP32-P4-WIFI6-Touch-LCD-5)):

- ESP32-P4, dual-core RISC-V (360 MHz on chip revision v1.x, 400 MHz on v3.x), 32 MB PSRAM, 32 MB flash, hardware JPEG decoder, 2D accelerator (PPA)
- ESP32-C6 as Wi-Fi 6 co-processor (SDIO, esp_hosted)
- 5" IPS panel 720×1280 (MIPI-DSI), capacitive touch (GT911)
- ES8311 audio codec with speaker, microSD slot, LiPo connector

The desktop is drawn at 640×360 and scaled ×2 to the panel in landscape. A 3D-printable case is in [`esp32/gehaeuse/`](esp32/gehaeuse/).

## Building

Requirements (Windows, PowerShell): Git, Python 3, [rustup](https://rustup.rs), LLVM (`winget install LLVM.LLVM`) and `cargo install ldproxy espflash --locked`. The Rust toolchain is pinned in `esp32/firmware/rust-toolchain.toml`. ESP-IDF 5.5 is downloaded on the first build (1 to 2 GB).

1. Fetch the browser sources (NetSurf, QuickJS-ng, zlib; SHA-256 checked, stored in `C:\ns`):
   ```powershell
   python esp32\netsurf\werkzeuge\quellen_holen.py
   ```
2. Build all images:
   ```powershell
   powershell -File esp32\firmware\bauen_flashen.ps1 -NurBauen
   ```
   The default profile is for chip revision v1.x. For v3.x set `esp_idf_sdkconfig_defaults` in `esp32/firmware/Cargo.toml` to `sdkconfig.defaults.rev3_x` and run `cargo clean` once. The script maps the repository to drive `O:` for short paths.
3. Flash (use your COM port):
   ```powershell
   powershell -File esp32\firmware\bauen_flashen.ps1 -Port COM3
   ```
4. Optional game data for local use: `python esp32\doom\wad\holen.py` and `python esp32\quake\pak\holen.py`.

PC tests (Git Bash): `bash esp32/werkzeuge/alle_tests.sh`. A release package is built with `esp32\firmware\release_paket.ps1 -Version 1.0`.

## Repository layout

| Path | Content |
|---|---|
| [`esp32/`](esp32/) | ESP32-OS source: kernel, KolibriOS layer, desktop, browser, games, firmware |
| [`spatz/`](spatz/) | software 3D engine used by the games |
| [`docs/`](docs/) | images, web flasher, release notes, evidence, development documents |
| [`LICENSES/`](LICENSES/) | license texts of all third-party components |

## License

ESP32-OS is licensed under the GNU General Public License v2.0 ([`LICENSE`](LICENSE)). The `spatz` crate is MIT OR Apache-2.0; built into the firmware it is part of the GPL-2.0 work. Third-party components and their licenses are listed in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). Doom and Quake game data are not included.

## Credits

- [KolibriOS](https://kolibrios.org): system function interface, C programs, libraries, skin, fonts and icons
- [NetSurf](https://www.netsurf-browser.org): web browser
- [QuickJS-ng](https://github.com/quickjs-ng/quickjs): JavaScript engine
- [doomgeneric](https://github.com/ozkl/doomgeneric): Doom port
- [quakegeneric](https://github.com/erysdren/quakegeneric): Quake port

ESP32-OS was developed with Claude Code.

Follow the progress on TikTok: @The_Unknown_Coder
````

## Nacht 23.11. (selbständig, eine Zeile je fertigem Punkt)

- ✔ B4: Adressleiste (erstes Antippen leert sie), Knöpfe „Merken“ und
  „Liste“ (Lesezeichen + Verlauf als antippbare Seite), Daten in
  `/daten/netsurf`, bleiben über Neustart – am Board per Autopilot belegt.
  Unterwegs gefunden: RAM-Fläche von libnsfb war am Board nicht angemeldet
  → **keine Bitmaps, also auch keine Bilder in Webseiten**; behoben.
- ✔ B6: Formularfeld antippen → Bildschirmtastatur geht von selbst auf
  (oben, wenn das Feld unten liegt) und danach wieder zu – am Board mit der
  Wikipedia-Suche belegt.
- ✔ B5 (mit Abweichung): Wischen und Pinch-Zoom gehen am Board; beim
  Wischen 11–12 Bilder/s statt 2–4, **30 nicht erreicht** (Verschieben von
  265 KB im PSRAM, DMA-Versuch ging nicht – Frage unten). Behoben unterwegs:
  Bildübergabe an den Kern je Bildpunkt, Compositor setzte verdeckte
  Fenster neu zusammen, drei Zeichenaufrufe je Bild, Suche nach scrollbarem
  Bereich bei jedem Schritt.
- ✔ B8: am Board belegt – Bilder in Webseiten (fehlten ganz: NetSurf war
  ohne JPEG/PNG), Tabs, Download (1 MB nach /daten/…/Downloads), Markieren
  → Zwischenablage des Systems. Unterwegs: Grossbuchstaben, Umlaute, „%“
  und „{|}~“ der Bildschirmtastatur kamen im Browser falsch an (behoben).
  Offen: markierter Bereich am Board stimmt nicht (Abweichung, in B10).
- ✔ B9: 20 echte Seiten am Board getestet (Tabelle oben): 7 geht,
  2 teilweise, 11 geht nicht – Hauptursache JS-Grenze/JS-Umgebung und der
  volle Heap (bei MDN hängt der Browser). Das ist jetzt B10.
- ✔ B10: JS-Laufzeit je Seite, Rückfallebene lädt ohne JS neu, Reserve im
  Programm-Heap, grosse HTML-Seiten abgeschnitten → 9 geht / 4 teilweise /
  7 geht nicht (vorher 7 / 2 / 11), kein Absturz/Hänger mehr bei den 20
  Seiten. Offen: „Zu wenig Speicher“ direkt nach schweren Seiten.
- ✔ B-Review: 12 Befunde (u. a. vier Fehler beim Schliessen von Tabs:
  freigegebener Speicher weiterbenutzt), alle behoben bzw. in PLAN.md
  richtiggestellt, neuer Test „Tab schliessen während er lädt“.
- ✔ A1 Notizen: neues Programm (Startmenü, Desktop, Dateimanager öffnet
  .txt damit), am Board getippt, gespeichert und von der SD-Karte wieder
  geöffnet (Testordner `/sd0/1/ESP32-OS/Notizen_Test`).
- ✔ A2 Rechner: neues Programm mit grossen Tasten, am Board per
  Autopilot „12 × 3,5 =“ getippt → 42 (Foto und Log).
- ✔ A3 Uhr mit Wecker und Timer: Timer und Wecker klingeln am Board mit
  Signalton jede Sekunde; der Wecker weckt das Board auch aus dem
  Ruhezustand und startet die Uhr, wenn sie zu ist. Nebenbei einen
  Heap-Fehler beim Neustart von Programmen gefunden und behoben.
- ✔ A4 Kalender: Monatsansicht, Monatswechsel per Wischen am Board.
- ✔ A5 Musikplayer: MP3 von der SD-Karte über den Lautsprecher (eigener
  Decoder-Weg, Ton-Ring im Kern), Pause/Weiter/Nächster/Zurück am Board.
  Testtöne selbst erzeugt (keine fremde Musik im Repo).
- ✔ A-Review: 5 Befunde (u. a. Mutex im Programm-Heap, Notizen schrieben
  grosse Dateien gekürzt zurück, zu kleine Tasten), behoben bzw. als
  Abweichung begründet; dabei am Board ein abgeschnittenes Startmenü
  gefunden (jetzt Ordner „System...“).
- ✔ S0–S4: Spiele als Desktop-Symbole und Ordner „Spiele...“, alter
  Launcher weg; am Board alle vier aus dem Desktop gestartet und zurück
  (Pudding 55 FPS, Würfelwelt 27,5, Doom 55, Quake 27,5), Quake-Abfrage
  „Wirklich beenden?“ per OK-Knopf. Entwicklungs-Schalter dafür:
  `ESP32OS_SPIEL_TIPPEN="s:x:y,…"` (tippt im Spiel).
- ✔ S5: Spieldaten nachweislich nicht in der Firmware (Prüfwerkzeug, auch
  in der Release-Prüfung); fehlen sie, Hinweis mit Shareware-Lizenz und
  Ablageort. Abweichung: kein Herunterladen per WLAN (keine offizielle
  Quelle der losen Dateien). Nebenbei: Notizen/Musik ohne SD-Karte zeigten
  auf einen Pfad, den der Kern nicht kennt (jetzt `/rd/1/…`).
- ✔ S-Review: 3 Befunde – unbrauchbare Spieldaten führten in den alten
  Launcher (jetzt ganz weg: zurück in den Desktop mit Hinweis, am Board
  gezeigt), Pudding ohne Home-Taste als Abweichung, Release-Prüfung lehnt
  jetzt alle Entwicklungs-Schalter ab.
- ✔ O3 Lizenz: Ordner `LICENSES/` mit allen Lizenztexten, README verweist.
- ✔ O7 Startzeit: 5,57 s → 4,16 s bis zum Desktop (festes Warten im
  Startbild entfernt).
- ✔ O2 ELF-Lader geprüft: auf dem P4 tragfähig, nicht umgesetzt
  (Abweichung: Programme statisch eingelinkt, eigene Bau-Kette nötig).
- ✔ O5 Langtest: 1 h am Board über WLAN, Wikipedia-Artikel 36-mal neu
  geladen, kein Reset, keine Panic, Browser-Heap Spitze 7,4 von 10 MB.
- ✔ B7 JavaScript: fetch/XHR, document.cookie (Cookie-Fix in urldb.c),
  localStorage dauerhaft, Masse aus dem Layout, DOMContentLoaded früh,
  Formulare mit input/submit/change, Verlauf; am Board JS-Seite mit
  Neu-Layout nach Klick und Eingabe. Grenzen als Abweichung in PLAN.
- ✔ O1 Optimierung: Scrollen im Browser 12,1 → 32,2 Bilder/s (die Anzeige
  liess beim Warten auf den PPA keine Programme laufen; Bildschirm nur noch
  wo geändert zusammengesetzt und ausgegeben), Desktop-Symbole 923 → 162 ms.
  Nebenbei: Browser-Absturz bei grossem JPEG und zerstückeltem Heap behoben.
- ✔ O4 Abnahme am Board: jeder Punkt aus Abschnitt 9 per Autopilot
  (Zuordnung in `docs/nachweise/o4/o4_abnahme.txt`); Spiele 55,0 / 27,6 /
  55,0 / 27,5 FPS. Unterwegs behoben: Bildbetrachter startete neben dem
  Dateimanager nicht (Speicher, als „fehlt“ gemeldet), Doom-Zähler zeigte
  29 statt 35 (Frage unten).
- ✔ O6: diese Datei.
- ✔ Z1 PDF-Betrachter („Wenn Zeit bleibt“): eigener PDF-Leser, öffnet aus
  dem Dateimanager, Rollen/Blättern/Zoom per Touch; am Board A4-Seite in
  0,24 s. Grenzen (u. a. Systemschrift statt eingebetteter Schriften, keine
  Scans mit CCITT/JBIG2/JPEG 2000) in PLAN unter „Abweichungen“.
- ✔ Z3 weitere KolibriOS-C-Programme: PasswordGen und TinyHashView
  (Prüfsummen MD5/SHA-1/SHA-256, am Board gleich wie am PC gerechnet),
  Quellen unverändert, im Startmenü unter „Mehr Programme...“; dazu
  `@notify` für ihre Meldungen. Dabei im Kern behoben: Mausposition und
  Funktion 9 bei Fenstern ohne Flag „relativ“, Zwischenablage (Grösse im
  Kopf wie KolibriOS, vorher Absturz beim Lesen), C-Programme bekamen
  keine Befehlszeile (argc/argv). Warum andere Programme nicht: PLAN,
  Abweichung „Z3“.
- ✔ Review PDF-Betrachter/Kern (Subagent): 7 Befunde, alle behoben – vor
  allem kaputte/bösartige PDFs konnten den Stapel sprengen oder den
  Speicher des Systems leeren (am Board: Abbruch der Firmware); jetzt
  Grenzen überall und je ein Test.

## B9: 20 echte Webseiten am Board (Nacht 23.11.)

Jede Seite per Autopilot über die Adressleiste getippt (Bildschirmtastatur),
Foto ~50 s danach; Fotos und Log-Auszug in `docs/nachweise/b9/`.
Bewertung: geht / teilweise / geht nicht, mit Grund.

Stand vor B10 (Runden A–C, 23.11. nachts):

| # | Seite | Ergebnis | Grund |
|---|---|---|---|
| 1 | www.google.com | teilweise | Logo, Gmail/Bilder/Anmelden; Skripte sprengen die JS-Grenze (2 MB) → JS aus, Suchfeld nicht sichtbar |
| 2 | DuckDuckGo HTML (Suche „esp32“ aus der Adressleiste) | geht | Ergebnisse mit Text und Bildern, 4,5 s |
| 3 | example.com | geht | 1,3 s |
| 4 | news.ycombinator.com | geht | Liste vollständig; JS nach 12 s abgeschaltet (Rückfallebene) |
| 5 | lite.cnn.com | geht | Liste vollständig, 2,3 s |
| 6 | www.tagesschau.de | teilweise | nur Sprunglinks sichtbar; JS-Umgebung fehlte („window is not defined“) |
| 7 | www.heise.de | geht nicht | leere Seite; JS-Umgebung fehlte, Seite 400 KB |
| 8 | github.com/espressif/esp-idf | geht nicht | „Zu wenig Speicher“ – Heap des Browsers (10 MB) voll |
| 9 | httpbin.org/forms/post | geht | Formular mit Feldern und Auswahl |
| 10 | old.reddit.com | geht nicht | leitet auf eine Anmeldeseite um, die leer bleibt |
| 11 | www.bbc.com/news | geht nicht | leer; JS-Umgebung fehlte („Fehler: null“) |
| 12 | stackoverflow.com/questions | geht nicht | Server antwortet 403 (Prüfseite, die JS braucht) |
| 13 | docs.python.org/3 | geht | Startseite der Doku, 22 s |
| 14 | developer.mozilla.org/de | geht nicht | Heap voll → **Browser hängt**, später hängt auch die Anzeige |
| 15 | www.openstreetmap.org | geht nicht | leer; JS-Grenze → JS aus (Karte braucht JS) |
| 16 | www.youtube.com | geht nicht | leer; JS-Grenze → JS aus |
| 17 | www.srf.ch | geht nicht | leer nach JS-Grenze → JS aus |
| 18 | www.w3.org | geht | Navigation und Text; japanische/chinesische Namen als Kästchen (Schrift) |
| 19 | www.spiegel.de | geht nicht | nach JS-Grenze zeigt die Seite ihren `<head>` (Titel, JSON, CSS) als Text |
| 20 | de.wikipedia.org (Hauptseite) | geht nicht | leer nach JS-Grenze (Artikel wie „ESP32“ gehen, siehe B2/B8) |

**Nach B10** (Runden D und E, 23.11. ~03:15–03:50, gleiche Seiten; Fotos
und Logs in `docs/nachweise/b10/`):

| # | Seite | vorher | nachher | Grund nachher |
|---|---|---|---|---|
| 1 | www.google.com | teilweise | teilweise | ohne JS neu geladen → echte Zustimmungsseite „Bevor Sie zu Google weitergehen“ |
| 2 | DuckDuckGo HTML | geht | geht | |
| 3 | example.com | geht | geht | |
| 4 | news.ycombinator.com | geht | geht | nach 12 s ohne JS neu geladen, 3,6 s |
| 5 | lite.cnn.com | geht | geht | |
| 6 | www.tagesschau.de | teilweise | geht nicht | „Zu wenig Speicher“ beim Verbinden (direkt nach schweren Seiten); vorher stürzte der Browser hier ab |
| 7 | www.heise.de | geht nicht | **geht** | Startseite mit Artikeln, 6,3 s |
| 8 | github.com/espressif/esp-idf | geht nicht | geht nicht | „Zu wenig Speicher“ (Fehlerseite, Browser läuft weiter) |
| 9 | httpbin.org/forms/post | geht | geht | |
| 10 | old.reddit.com | geht nicht | geht nicht | Anmeldeseite, ohne JS neu geladen (Foto fehlt, Übertragung) |
| 11 | www.bbc.com/news | geht nicht | geht nicht | ohne JS neu geladen → Fehlerseite „Unerwarteter Fehler“ |
| 12 | stackoverflow.com/questions | geht nicht | geht nicht | Server antwortet 403 |
| 13 | docs.python.org/3 | geht | geht | 2,5 s statt 22 s |
| 14 | developer.mozilla.org/de | geht nicht (Browser hängt) | **teilweise** | Inhalt da, Kopfzeile überlappt |
| 15 | www.openstreetmap.org | geht nicht | **teilweise** | Seite mit Suche, ohne Karte (braucht JS) |
| 16 | www.youtube.com | geht nicht | geht nicht | „Zu wenig Speicher“ beim Verbinden |
| 17 | www.srf.ch | geht nicht | **teilweise** | Navigation und Hinweis „nur mit JavaScript“ |
| 18 | www.w3.org | geht | geht | |
| 19 | www.spiegel.de | geht nicht | geht nicht | „Zu wenig Speicher“ beim Verbinden (vorher `<head>` als Text) |
| 20 | de.wikipedia.org (Hauptseite) | geht nicht | geht nicht | „Zu wenig Speicher“ beim Verbinden |

Nachher: 9 geht, 4 teilweise, 7 geht nicht (vorher 7 / 2 / 11) – und der
Browser stürzt bei keiner der 20 Seiten mehr ab oder hängt (vorher bei MDN,
tagesschau.de). Offen: „Zu wenig Speicher“ beim Verbinden direkt nach
schweren Seiten, obwohl der Heap des Browsers dann nur ~3–8 MB belegt –
Ursache noch nicht gefunden (Kandidat: der Inhalt der vorigen Seite samt
JS-Laufzeit bleibt bis zum Aufräumen des Zwischenspeichers liegen).

Vorher-Summe: 7 geht, 2 teilweise, 11 geht nicht. Häufigste Ursache: die JS-Grenze
(2 MB) wird gesprengt, JavaScript wird mitten in der Seite abgeschaltet –
die Seite bleibt leer oder kaputt; dazu am Board „Fehler: null“ beim Anlegen
der JS-Umgebung (die Laufzeit des Fensters ist von vorigen Seiten schon voll)
und der volle 10-MB-Heap (GitHub, MDN – dort hängt der Browser). → B10.

## Fragen an dich (Nacht 23.11.)

- **Zur Kenntnis (B10, Runde 46):** Der Browser wurde mitten im Lauf vom
  Kern beendet („Speicher voll“, tagesschau.de). Die restlichen Tipps des
  Autopiloten landeten danach auf dem Desktop und öffneten den Dateimanager
  auf der SD-Karte (`/sd0/1/ESP32-OS`, Ordner „quake“). Laut Fotos wurde
  dort nichts angelegt, umbenannt oder gelöscht (gleiche Ansicht bis zum
  Ende). Ursache ist behoben (Reserve im Programm-Heap, siehe B10); schwere
  Seiten kommen in Testläufen ans Ende.
- **Zur Kenntnis (O4):** Im Abnahme-Durchlauf tippte der Autopilot im
  Dateimanager einmal in den Ordner `old/` statt `Screenshots/` (nur
  angesehen, nichts geändert). Gespeichert wurden nur Bildschirmfotos in
  `/sd0/1/Screenshots` (ESP32-OS-eigener Ordner).
- **B5, 30 Bilder/s:** erledigt mit O1 (32,2 Bilder/s), siehe PLAN.
- **Spieltempo (O4, Frage):** Die Hauptschleife der Spiele läuft am Board
  mit ~63–67 Durchgängen/s, das Panel mit 55 Hz – so schon vor ESP32-OS
  (S1: 63,2, S3: 66,3). Die Spielzeit je Durchgang (`dt`) zählt mindestens
  eine Bilddauer; Spiele, die damit rechnen (Pudding-Panik), laufen deshalb
  etwas schneller als die Uhr (bis ~15 %). Doom rechnet mit der echten Uhr –
  nur sein FPS-Zähler zeigte deshalb 29 statt 35 (behoben). Ich habe das
  Spieltempo bewusst nicht angefasst, weil du Pudding-Panik so kennst.
  Soll ich es auf die echte Zeit umstellen (Pudding wäre dann ~13 %
  langsamer)?
  **Antwort (23.11.):** Pudding-Panik bleibt, wie es ist.

## Fragen an dich (Veröffentlichung, O8)

- **Lizenz der Firmware-Abbilder:** Die Firmware verbindet GPL-2.0-Code
  (NetSurf, KolibriOS-Teile) mit Apache-2.0-Code (ESP-IDF, esp_hosted).
  Ob ein Binär-Release so weitergegeben werden darf, ist eine
  Rechtsfrage, die ich nicht entscheide – bitte vor dem OK zum Release
  klären (Quellcode im Repo ist nicht betroffen).
  **Antwort (23.11.):** veröffentlichen; die Lizenzfrage steht ehrlich im
  Release-Text unter „Known issues“, dazu das schriftliche Angebot für den
  vollständigen Quellcode samt Patches (Cookie-Fix, PLAN N1).
- **Release v1.0:** lokal fertig – `VERSION` = „1.0 (2026-11-23)“, Paket
  mit `release_paket.ps1 -Version 1.0` gebaut (Release-Prüfung: alle sechs
  Abbilder ohne WLAN-Zugangsdaten, keine Entwicklungs-Schalter, keine
  Spieldaten) unter `release/v1.0/` (nicht im Repo; SHA256SUMS,
  manifest.json), Web-Flasher `docs/flasher/`, Text `docs/release/v1.0.md`.
  Veröffentlicht wird erst mit deinem OK: Abbilder ins GitHub-Release, für
  den Web-Flasher nach `docs/flasher/firmware/`.
  **Antwort (23.11.):** OK – VERSION „1.0“, Paket neu bauen, Tag v1.0,
  Release „ESP32-OS 1.0“, GitHub Pages mit Board-Test, danach U1.

## Für dich: Zusammenfassung der Nacht 22.11.

**Fertig (je mit Nachweis in PLAN.md):** P6 Bildschirmfoto · B0 Flash neu
aufgeteilt (zwei ESP32-OS-Plätze mit Rollback; vorher ganzer Flash zweimal
gesichert und geprüft) · B0b progressive JPEGs · WLAN W0, W1, W3, W3b, W4,
W6, W7, W8, W9 und W-Review (Befunde behoben) · A6 Taskmanager mit echtem
Zwangsbeenden (Endlosschleife am Board per Autopilot beendet, System lief
weiter). U1 (Updates über WLAN) ist als Code fertig und teilweise
nachgewiesen, der Abschluss braucht O8. Nichts gepusht.

**Wichtigste Entscheidungen** (Gründe im Abschnitt darunter): esp_hosted
3.1.0 als gekürzte lokale Kopie (läuft mit der alten C6-Firmware) · W7 und
B0 vorgezogen · B0b vor U1 · B1 (NetSurf, mehrtägig) erst zurückgestellt,
A6 vorgezogen, B1 am Morgen begonnen (Upstream 3.11, Framebuffer-Frontend)
· Taskmanager: erst kooperativ, nach 2 s Zwang; Systemdienste nicht
beendbar · Einstellungen werden jetzt absturzsicher geschrieben.

**Deine Antworten vom 22.11. morgens** (Auftrag Abschnitt 13, PLAN Block 0):
Reihenfolge jetzt V1 Touch oben links → V2 Einstellungen sicher speichern →
Browser (Block E, mit JavaScript) → Rest. Akku/Strom (P5) wartet aufs
Einlöten, C6-Update (W2/W5) später – Anleitungen unten. Updates (U1) mit
lokalem Testserver statt Release. Keine eFuses (README-Hinweis in O8).
Spiele werden eigene Programme im Desktop (Block G), der alte Launcher fällt
weg.

**Erledigt am 22.11. vormittags:** V1 Touch (Ursache: GT911-Treiber
quittierte auch ohne neue Daten → verlorene „losgelassen“-Meldung; eigene
Abfrage, 10 min am Board ohne gemeldete Berührung) · V2 Einstellungen
(drei Ursachen, Sperre + Prüfsumme + temporäre Datei + Sicherungskopie;
Stromausfall an allen 6 Stellen am Board und am PC nachgestellt) · V3
SD-Reste und `C:\ns\src` gelöscht · V4 Anleitungen (unten). V1-Gegenprobe
von dir: langer Druck auf die Uhr löst das Bildschirmfoto weiter aus.

**Am Board jetzt:** normale Firmware im Platz esp32os_a (startet in den
Desktop, WLAN + Internetzeit laufen). Der zweite Platz esp32os_b enthält
Testdaten vom OTA-Test (wird nie gestartet, das nächste Update
überschreibt ihn). Intern liegen Testdateien `/rd/1/laden.html` und
`/rd/1/bilder/` (W9, B0b).

**Stand 22.11. abends – der Browser läuft am Board:** B1 ist abgehakt.
NetSurf 3.11 mit QuickJS-ng ist ein eigenes ESP32-OS-Programm „browser“
(Symbol Globus auf dem Desktop). Am Board nachgewiesen: lokale Seite
gerendert, Schliessen per ✕, zweiter Start (Fotos `docs/nachweise/b1/`).
JavaScript (DOM, Ereignisse, Timer, Neu-Layout nach Änderungen) läuft am
PC; am Board fehlt dafür noch eine Seite mit JS (kommt mit B2, sobald
echte Seiten laden). Zwei Abweichungen (PLAN): Ressourcen im
Firmware-Abbild statt im Flash-Dateisystem, Browser-Heap 10 MB.
Danach der Flash-Absturz geklärt und behoben (Abschnitt „Flash-Absturz“):
Flash-Schreiben parallel zu abgebildetem Lesen; Flash-Sperre, Nachweis
120 s Dauerlast ohne Neustart.
**22.11. abends – Veröffentlichung:** Repo anonymisiert (Subagent im
Worktree), `main` ohne alte Geschichte auf GitHub TheUnkn0wnC0DE/ESP32-OS
(790d419), danach Lizenztexte/O:-Prüfung/UTC (26d99fb, gepusht).
Commits ab jetzt als TheUnkn0wnC0DE, in UTC; Push-Regeln in CLAUDE.md
(nur `main`, nur `origin`, kein force, vorher Prüfungen).

**B2 HTTPS abgeschlossen (Details PLAN B2, `docs/nachweise/b2/`):** HTTPS
mit Zertifikatsprüfung, Zertifikatsfehler abgelehnt, Suche aus der
Adressleiste und Startseite (DuckDuckGo HTML), Touch-Suche → Ergebnis
antippen → Wikipedia-Artikel angezeigt. Wikipedia ist am Board lesbar, aber
langsam (~40 s nach dem Antippen). Unterwegs gefunden und behoben:
- interner RAM (malloc < 1 KB intern statt < 16 KB, Abschnitt „Interner RAM“);
- Programm-Heap: Grössenklassen statt einer sortierten Liste (vorher 240
  Mio. Listenschritte für eine Seite);
- jede Systemfunktion nahm in `pruefe_ende` die Kernsperre (auch jedes
  malloc/free) – jetzt Merker je Faden;
- Funktion 37.3 je Fenster (kurzes Tippen ging im Browser verloren);
- Rückfallebene ohne JavaScript (12 s / JS-Grenze) mit Knopf „Ohne JS“ –
  Abweichung in PLAN: automatisch an Ort und Stelle statt Neuladen;
- Zwischenspeicher 2 MB, JS-Grenze 2 MB am Board.
Werkzeug für B9/B10: `esp32/netsurf/werkzeuge/proxy.py` + PC-Test
`seite_ueber_proxy` stellt echte HTTPS-Seiten am PC nach.
**B3 abgeschlossen:** Zwischenspeicher am Board 2 MB (1,5 MB Quelldaten,
0,5 MB Bilder) statt 12 MB; PC-Test zeigt, dass die Grenze unter Druck hält,
Board-Log über eine ganze Sitzung (`docs/nachweise/b3/`).
**Nächster Schritt:** B4 (Adressleiste, Lesezeichen, Verlauf) laut Plan;
der Nutzer hatte B5 als Nächstes genannt – vor B4 nachfragen.

## Browser-Architektur (Auftrag 13.9, Entscheidung 22.11.)

- **HTML/CSS/Layout: NetSurf 3.11** (rendert am PC schon, siehe B1). Stärken:
  klein, für eingebettete Geräte gebaut, HTML5-Parser, CSS 2.1 + Teile von
  CSS3 inkl. Flexbox. Grenzen: kein CSS Grid, keine Web Components/Shadow
  DOM, kein SVG ausser libsvgtiny.
- **JavaScript: QuickJS-ng 0.18.0** statt Duktape (das NetSurf mitbringt,
  nur ES5). Grund: ES2023 (Klassen mit privaten Feldern, async/await,
  BigInt …), klein (Amalgam eine C-Datei), **Speichergrenze je Laufzeit**
  (eine Laufzeit je Seite → Seite bricht mit „out of memory“ ab, Browser
  und System laufen weiter) und **Unterbrechung** (Endlosschleifen nach
  Zeitgrenze). Geholt und geprüft wie NetSurf (SHA-256 von GitHub).
  Nachweis am PC: `netsurf/src/lib.rs` Tests `javascript_modern`,
  `javascript_grenzen` (64 Bit und i686).
- **DOM- und Web-Anbindung: eigene Schicht für QuickJS** an NetSurfs
  JS-Schnittstelle (`content/handlers/javascript/js.h`: Seite anlegen,
  Skripte ausführen, Ereignisse auslösen) auf libdom. Upstream-Bindungen
  (WebIDL + nsgenbind, nur für Duktape) dienen als Vorlage. Reihenfolge nach
  Nutzen: Window/Document/Node/Element, Ereignisse, Timer, console,
  location/navigator, localStorage (Datei je Herkunft unter /sys),
  XMLHttpRequest und fetch (über den Abrufer von B2), dann was die
  20-Seiten-Liste (B9) am häufigsten verlangt.
  **Stand 22.11. nachmittags:** steht am PC (`netsurf/c/js_quickjs.c` +
  `netsurf/js/vorspann.js`, Umfang in PLAN B7). Aufteilung: C liefert nur
  Grundfunktionen (Knoten-Hüllen mit fester Identität, Baum, Attribute,
  Text, Fragment-Parser, libdom-Ereignisse, Timer, Protokoll); die
  DOM-Klassen selbst sind JavaScript – so bleibt die C-Schicht klein und
  der DOM-Umfang lässt sich ohne C-Änderung erweitern. Zwei Eigenheiten von
  libdom gleicht der Vorspann aus: das Ziel eines Ereignisses steht auch im
  Fang- und Sprudelpfad, und libdom sprudelt auch Ereignisse mit
  `bubbles: false`. Inline-Handler (`onclick="…"`) bekommen nur Elemente,
  die solche Attribute haben (sonst kostete jede Seite eine Hülle je
  Element). Die PC-Tests mit Prüflauf laufen je in einem eigenen
  Kindprozess, weil NetSurf in einem Prozess nur einmal startet (siehe B1
  „Neustart“ weiter unten).
  **Neu-Layout steht am PC** (`netsurf/c/neu_layout.c`, ohne Änderung an
  den NetSurf-Quellen): nach Änderungen (100 ms gebündelt) entsteht der
  Box-Baum neu aus dem DOM, der alte wird erst danach freigegeben. Was
  Zeiger in den alten Baum hält, wird umgehängt: Formularfelder (getippter
  Text bleibt), iframe-Fenster, Fokus, Auswahl; Bilder werden über den
  Cache neu angefordert und die alten danach freigegeben. Kosten am Board
  noch nicht gemessen (kommt mit dem Board-Bau von B1).
- **Neu-Layout nach DOM-Änderungen:** NetSurf baut den Box-Baum einmal nach
  dem Laden. Ändern Skripte das DOM danach (das tun fast alle modernen
  Seiten), muss der Box-Baum neu entstehen – gebündelt (z. B. alle 100 ms)
  aus dem geänderten DOM, dann neu layouten. Das ist die grösste
  Einzelaufgabe und entscheidet, wie viele Seiten wirklich funktionieren.
- **Ehrliche Erwartung:** Einfache bis mittlere Seiten (Wikipedia,
  Suchmaschinen mit HTML-Ergebnissen, Foren, viele Nachrichtenseiten in der
  Lesefassung) sollten gut gehen. Grosse Single-Page-Apps (GitHub-Oberfläche,
  Seiten auf React/Angular mit MBs an JavaScript, CSS Grid) werden „geht
  teilweise“ oder „geht nicht“ sein – 32 MB PSRAM und ein 400-MHz-Kern sind
  dafür knapp. Die Tabelle in B9 zeigt es genau, B10 verbessert gezielt.
- Weitere Bausteine: Abrufer über esp_http_client mit Zertifikatsbündel
  (B2, Cookies), eigene Bedienoberfläche statt der fbtk-Leiste (Tabs,
  Adressleiste mit Suche, Lesezeichen, Verlauf, Downloads, Zoom, Text
  markieren/kopieren über die Zwischenablage des Kerns, Formulare mit der
  Bildschirmtastatur), JPEG/PNG über `kolibri::dekoder` (Hardware-JPEG und
  progressiv aus B0b).

## Anleitung: Akkuanzeige und Strommessung (für P5, wenn der Akku drin ist)

**Spannungsteiler (Akkuanzeige):**
1. Leiste: Pin 15 = GPIO21 (ADC1 Kanal 5), Pin 13 = GND. Die Leiste ist
   nicht wie beim Raspberry Pi belegt – Pin 1 vorher am Platinenaufdruck
   prüfen.
2. Akku+ → **200 kΩ** → Pin 15 → **100 kΩ** → Pin 13 (GND); **100 nF**
   parallel zu den 100 kΩ (beruhigt die Messung). Bei 4,2 V liegen am Pin
   1,4 V (erlaubt bis ca. 3 V). Der Teiler zieht dauernd ca. 14 µA.
3. Mir Bescheid geben – ich schalte in `esp32/firmware/src/board.rs`
   `BATTERY = Some(BATTERY_SUGGESTED)` ein. Abgleich: Akkuspannung mit dem
   Multimeter messen, angezeigte Spannung im Log ablesen, Faktor =
   3,0 × gemessen ÷ angezeigt.

**Strommessung:**
1. Multimeter auf mA (bzw. µA für den Ruhezustand) **in Reihe** in die
   Akku-Plusleitung (zwischen Akku+ und Platine). Alternativ ohne Akku: ein
   USB-Strommesser zwischen Netzteil und Board (misst bei 5 V).
2. Drei Zustände je ca. 30 s ablesen: Desktop aktiv (Bildschirm an),
   Bildschirm aus (Einstellungen → Energie, kurze Zeit wählen), Ruhezustand
   (Startmenü → Ruhezustand). WLAN dabei einmal an, einmal getrennt.
3. Werte mit Zustand an mich – ich trage sie in STATUS ein, damit ist P5
   nachgewiesen. Achtung beim µA-Bereich: Beim Aufwachen fliesst wieder
   viel Strom – vorher auf mA zurückschalten, sonst Sicherung im
   Multimeter.

## Anleitung: C6-Firmware aktualisieren (W2, später)

Ausgangslage: Der ESP32-C6 (WLAN-Chip) meldet esp_hosted-Firmware 0.0.0.
(24.11. berichtigt: Das Update vom P4 aus über OTA geht sehr wahrscheinlich
auch mit der alten Firmware – nur „activate“ kam erst mit 2.6; Weg:
`werkzeuge/board/c2_lauf.ps1`. Die Anleitung hier ist der Weg per Kabel,
falls OTA abgelehnt wird.) **Vorher am Schaltplan von Waveshare prüfen**
(ESP32-P4-WIFI6-Touch-LCD-5, Seite C6): wo UART TX/RX, BOOT (GPIO9) und EN
des C6 herausgeführt sind (Stiftleiste oder Testpunkte) – das weiss ich
noch nicht sicher, darum hier als Prüfschritt.

1. **Sichern (Rückweg!):** USB-UART-Adapter mit **3,3 V** an C6-TX/RX/GND.
   P4 im Download-Modus halten (BOOT gedrückt beim Einstecken), damit er den
   C6 nicht über GPIO54 zurücksetzt. C6 in den Download-Modus: GPIO9 auf
   GND, EN kurz auf GND. Dann:
   `esptool --chip esp32c6 -p COMx flash_id` (Flash-Grösse ablesen) und
   `esptool --chip esp32c6 -p COMx read_flash 0 <Grösse> c6_sicherung.bin`
   – zweimal lesen, beide Dateien müssen gleich sein (SHA-256 vergleichen).
2. **Firmware bauen:** esp-hosted-mcu in der Version des Hosts (3.1.0),
   Projekt `slave`, Ziel esp32c6, Transport SDIO – das bereite ich vor
   (`idf.py set-target esp32c6`, Konfiguration wie der Host: SDIO, 4 Bit).
3. **Aufspielen:** C6 wieder in den Download-Modus,
   `esptool --chip esp32c6 -p COMx write_flash 0x0 <bootloader> …` (die
   genaue Zeile erzeugt `idf.py`). Kabel ab, Board normal starten.
4. **Prüfen:** Im Log muss stehen `coprocessor=3.1.0` (statt 0.0.0), das
   WLAN verbindet sich wie vorher. Danach geht W5 (iTWT) und künftige
   Updates gehen vom P4 aus (W2).
5. **Wenn es schiefgeht** (kein WLAN, C6 meldet sich nicht):
   C6 in den Download-Modus und die Sicherung zurück:
   `esptool --chip esp32c6 -p COMx write_flash 0 c6_sicherung.bin` –
   danach ist alles wie vorher. Der P4 und ESP32-OS sind davon nicht
   betroffen; ohne C6 läuft alles ausser WLAN.

## Nacht 22.11. (selbständig, Entscheidungen)

- P5: PWR-Knopf und Strommessung warten auf dich → Abweichung, weiter mit
  P6. Grund: ohne Drücken am Board bzw. Messgerät nicht nachweisbar.
- P6 fertig. Startmenü: Beispielprogramme unter „Mehr Programme…“, weil
  das Menü am Board sonst über die Taskleiste ragte.
- Absturz gefunden und behoben: `static Mutex` (Symbol-Zwischenspeicher)
  wurde zuerst in einem Programmfaden gesperrt → pthread-Objekt im
  Programm-Heap → nach Programmende überschrieben (Load access fault beim
  Öffnen des Dateimanagers). Regel: statische Mutexe nur im
  Kern-Abschnitt sperren; die Uhr-Korrektur gleich mit abgesichert.
- Am Board meldete der Touch die ganze Nacht eine dauerhafte Berührung bei
  Panel (81, 1232) (linke obere Ecke quer) – liegt etwas auf dem Display?
  Entscheidung: Ein Punkt, der 60 s unverändert liegt, wird ignoriert (nur
  dieser Punkt); Autopilot-Builds ignorieren echten Touch ganz.
- Auf der SD liegen jetzt drei Test-Bildschirmfotos in `/Screenshots`.
- B0 vorgezogen (vor dem WLAN), Grund: factory-Partition 96 % voll. Neue
  Aufteilung: zwei ESP32-OS-Plätze (OTA + Rollback, Bootloader mit
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`), Doom/Quake je 2 MB, 16 MB Daten.
  Vorher den ganzen Flash zweimal gesichert und geprüft
  (`sicherung/`, nicht im Repo). Quake-PAK liegt jetzt auf der SD
  (`ESP32-OS/quake/pak0.pak`, vom Board aus dem Flash kopiert, CRC geprüft).
  Die Doom-/Quake-Programme linken den Desktop nicht mehr mit (1,7 MB).
  Flashen jetzt `bauen_flashen.ps1 -Nur esp32os` („factory“ geht weiter).
- W0: `wlan.local` (in `.gitignore`) hält die Test-Zugangsdaten; nur
  `bauen_flashen.ps1 -Wlan` liest sie (als Build-Variablen, ohne Ausgabe).
  Prüfung `werkzeuge/geheimnis_pruefen.py` zählt nur Treffer (Repo und
  `git log -p --all`), gibt die Werte nie aus.
- W1: esp_hosted **3.1.0** (nicht 2.13.x). Grund: Laut Migrationsanleitung
  arbeitet der 3.x-Host mit C6-Firmware 0.0.6+/1.x/2.x zusammen – die
  Version auf dem C6 ist unbekannt, und so läuft das WLAN auch ohne
  C6-Update. esp_wifi_remote fest auf 1.6.5.
- W1: Die Registry-Fassung von esp_hosted hat Beispiel-/Testordner mit
  Pfaden über 260 Zeichen; der Komponenten-Manager scheitert beim Kopieren
  in den Build-Ordner. Ersatz: `werkzeuge/komponenten_holen.py` lädt das
  Archiv der festen Version, prüft SHA-256 und entpackt ohne Beispiele/
  Tests/Doku nach `C:\ec\v3.1.0\espressif__esp_hosted`; eingebunden per
  `override_path` (gilt projektweit). Windows-Langpfade (Registry) habe ich
  nicht angefasst – Systemeinstellung.
- W1: Autostart von esp_hosted vor `app_main` abgeschaltet
  (`CONFIG_ESP_HOSTED_AUTO_CALL_INIT_BEFORE_APP_MAIN=n`). Grund: Der
  Konstruktor wird per `--undefined` immer gelinkt und hätte den C6-Transport
  auch in Doom/Quake gestartet (Aufgabe Prio 23, SDIO-Interrupts). Das OS
  startet ihn in `wlan.c` selbst.
- W1: Erster Lauf am Board: Transport steht (SDIO 4 Bit, 40 MHz, C6
  erkannt), aber der C6 meldet Firmware **0.0.0** und die erste RPC-Anfrage
  blieb ohne Antwort; dazu „mempool OOM (RX)“: Nach dem Start der Anzeige
  sind nur 19 KB intern frei (4 KB DMA-fähig). W7 (Puffer in den PSRAM)
  darum vorgezogen. Die 3.x-Option `EH_HOST_PORT_DMA_PREFER_SPIRAM` wirkt
  auf dem P4 nicht (fragt PSRAM mit DMA-Kennung an) → kleine Anpassung an
  esp_hosted (`komponenten_holen.py`, mit Prüfung des Originaltexts):
  PSRAM auf die L2-Cache-Zeile ausgerichtet; der SDMMC-Treiber des P4 kann
  DMA mit PSRAM-Puffern.
- W3: In Test-Builds (`-Wlan`) zeigt die Oberfläche keine echten SSIDs
  („Testnetz (wlan.local)“, „Netz 1…“), damit weder dein Netz noch die der
  Nachbarn auf Nachweisfotos landen; das Testnetz steht oben, der
  Passwort-Dialog ist damit (verdeckt) vorbelegt. Normale Builds zeigen die
  echten Namen.
- W3/W4: Bekannte Netze (höchstens 8) liegen im NVS des P4. Das NVS ist
  nicht verschlüsselt (bräuchte Flash-Verschlüsselung/eFuses – nicht ohne
  dich). Unsichere Netze (WEP, WPA ohne 2) werden abgelehnt.
- W1 gelöst: Die Bildpuffer der Anzeige nahmen je nach Startreihenfolge den
  ganzen internen RAM, dann hing der WLAN-Start still. Jetzt lassen sie
  64 KB übrig (`hw::INTERN_RESERVE`), ein Wächter meldet Hänger nach 15 s.
  Lage der Bildpuffer unverändert (61/61 KB intern, 40 KB PSRAM).
- Speicherfehler gefunden (PC-Test stürzte ab): `kern::wlan` legte SSID/
  Passwort aus dem Heap des Programms im Kern ab. Jetzt entsteht alles, was
  im Kern bleibt, im Kern-Abschnitt (wie beim Symbol-Zwischenspeicher).
- W3b: Internetzeit über `pool.ntp.org` (SNTP von lwIP), Schalter in
  „Datum & Zeit“ (Standard an, ini `ntp`). Gestellte Zeit wird sofort
  gesichert (Rückfall nach dem Ausschalten).
- W6: Signal in der Taskleiste (4 Balken; getrennt = rotes Kreuz), Antippen
  öffnet Einstellungen → WLAN.
- **Am Board gemerkt:** Dein Testnetz steht jetzt im NVS (aus dem W3-Lauf
  per Touch). Das Board verbindet sich nach jedem Start selbst. Vergessen:
  Einstellungen → WLAN → „x“ in der Zeile.
- W2 (C6-Firmware): Der C6 hat Firmware 0.0.0. Ein Update vom P4 aus (OTA
  über SDIO) gibt es erst ab C6-Firmware 2.6 → mit 0.0.0 nur per Kabel am
  C6. Laut deiner Regel 3 nur vorbereiten. WLAN läuft auch so (3.x-Host ist
  abwärtskompatibel).
- W5 (iTWT): umgesetzt, aber die C6-Firmware 0.0.0 kennt die Anfrage nicht
  → wartet auf W2.
- W8: Unter Last brach der SDIO-Empfang ab (alte C6-Firmware streamt
  mehrere Pakete, meldet den Modus aber nicht) → zweite Anpassung an
  esp_hosted (dynamische Empfangspuffer). Danach 10–25 Mbit/s ohne Fehler.
- W9: KolibriOS-Sockets (Funktion 75) auf `std::net`/lwIP; network.obj
  nativ. `exit()` in C-Programmen beendete vorher das ganze System (Host-
  libc) – jetzt nur das Programm. Nachweis mit dem Original-Beispiel
  `http_tcp_demo.c` und einem eigenen Download-Programm (`netz_laden.c`,
  lädt die Seite vollständig und speichert sie).
- W-Review (Subagent) fand u. a. einen Wettlauf zwischen automatischem
  Wiederverbinden und Verbinden von Hand (falsches Netz hätte gemerkt
  werden können) und ein Socket-Leck beim Zwangsbeenden am Board – beides
  behoben, Rest siehe PLAN „Abweichungen“.
- B0b vor U1 gezogen: U1 braucht für den vollen Nachweis ein GitHub-Release
  (O8, nur mit deinem OK). U1-Code ist trotzdem fertig: HTTPS mit
  Zertifikatsprüfung (abgelaufene und selbst signierte Zertifikate werden
  abgelehnt), Prüfsumme, OTA in den zweiten Platz, Einstellungen → System.
  Repository für Releases: `TheUnkn0wnC0DE/ESP32-OS` (beim Bauen änderbar
  mit `ESP32OS_UPDATE_REPO`).
- U1: TLS braucht Speicher – mbedTLS im PSRAM, AES/SHA in Software (die
  Hardware-AES fand keinen internen DMA-Speicher mehr und stürzte ab).
- Einstellungen: 11 Kacheln (6 × 2), neu „WLAN“ und „System“.
- B1 (NetSurf): Wahl Framebuffer-Frontend von NetSurf (upstream), eigene
  libnsfb-Fläche auf ein Kern-Fenster, Abrufe über esp_http_client mit dem
  Zertifikatsbündel aus U1. Grund: Das Framebuffer-Frontend ist das
  kleinste, ohne GTK/SDL. Der KolibriOS-„Port“ kommt nicht in Frage: Im
  KolibriOS-Repo liegt unter `programs/network/netsurf` nur ein Lader, der
  ein fertiges x86-Programm von ftp.kolibrios.org herunterlädt – die
  Quellen sind nicht dabei (geprüft 22.11. früh; vorher hatte ich hier
  fälschlich „Menuet-Bibliotheken“ als Grund genannt). Erst A6 vorgezogen
  (mehrtägiger Port, halbfertig nicht nachweisbar), am Morgen begonnen.
- B1 Quellen: `netsurf-all-3.11.tar.gz` von download.netsurf-browser.org
  (offiziell, 9,1 MB, Dezember 2023; NetSurf mit allen eigenen
  Bibliotheken), SHA-256 `4dea880f…44b7b` im Skript festgelegt. Ablage wie
  bei esp_hosted ausserhalb des Repos unter `C:\ns` (kurze Pfade).
  `C:\ns\src` ist ein erster Probe-Entpackordner von mir – kann weg, ich
  lösche ausserhalb des Repos nichts. Das Holskript löscht nichts mehr (ein
  früherer Stand hat einmal seinen eigenen, kurz zuvor angelegten Ordner
  `C:\ns\3.11` neu entpackt). Es gibt hier kein make/gperf: Quelllisten
  liest build.rs aus den Upstream-Makefiles, die gperf-Tabelle von libhubbub
  erzeugt das Skript gleichwertig (Binärsuche statt perfektem Hash).
- B1 zlib: NetSurf braucht zlib (gepackte Messages, Hash-Tabellen). Statt
  eines Stummels die echte zlib 1.4 von zlib.net (1,5 MB), SHA-256
  `02dd1597…d584` wie auf zlib.net veröffentlicht, nach `C:\ns\zlib-1.4`.
- B1 Kern: Der NetSurf-Kern läuft am PC (64 Bit und i686), übersetzt wie
  upstream als Release-Variante (NDEBUG). iconv: eigener Ersatz auf den
  Codecs von libparserutils; „Seite komplett speichern“ weggelassen
  (braucht Regex) – beides als Abweichung in PLAN.
- B1 nächster Schritt (Plan): NetSurf als KolibriOS-C-Programm wie die
  übrigen C-Programme (`/FIkolibri_libc.h`: malloc → Heap des Programms über
  Funktion 68, Dateien über Funktion 70) – so gibt Beenden/Zwangsbeenden
  allen Speicher frei. Framebuffer-Frontend upstream, libnsfb mit eigener
  Fläche „esp32os“ (Puffer → Fenster über Funktion 65, Ereignisse über
  Funktion 10/23/37 → nsfb-Ereignisse). Programmbau nur i686 + Board (wie
  alle C-Programme); die Bibliotheksproben bleiben auch auf 64 Bit.
- B1 Stand am Morgen: libnsfb und das ganze Framebuffer-Frontend lassen
  sich am PC übersetzen (Schrift und Symbole erzeugt). Offener Haken:
  Programme laufen im gemeinsamen Adressraum, statische Daten überleben
  einen Neustart – NetSurf merkt sich z. B. das Beenden in
  `static bool fb_complete` (gui.c); ein zweiter Start ginge sofort zu.
  Lösung kommt mit dem Programm-Bau: entweder geprüfte Anpassungen wie bei
  esp_hosted (Originaltext-Prüfung) oder die Daten der NetSurf-Objekte beim
  Start neu initialisieren (eigener Linker-Abschnitt am Board). Ein
  PC-Test „zweimal hintereinander starten“ soll das absichern.
  Versuch (22.11. früh, nicht eingecheckt): Der zweite Start im selben
  Prozess scheitert an `guit` (desktop/gui_factory.c, global, von
  netsurf_exit nicht zurückgesetzt) → „operation table failed
  registration“, danach `die()` → `exit(1)`. Lösungsweg (Vorschlag):
  (1) Liste aller beschreibbaren statischen/globalen Daten der NetSurf-
  Objekte (nm, .data/.bss) und je Eintrag prüfen; (2) zurücksetzen ohne
  Upstream-Änderung über Hüll-Dateien (`#include "…/gui.c"` plus eine
  Rücksetzfunktion im selben Übersetzungseinheit – erreicht auch `static`)
  bzw. am Board die .data/.bss der NetSurf-Bibliotheken über einen eigenen
  Linker-Abschnitt beim Start neu laden. `exit()` im Programm beendet über
  kolibri_libc nur das Programm (W9), nicht das System.
  Messung (llvm-nm, i686): 714 beschreibbare Datensymbole (Kern 521,
  Frontend 125, libdom 48, libcss 13, Rest < 5) – zu viele für Handarbeit.
  Entscheidung (Vorschlag für den Board-Bau): alle .data/.bss der
  NetSurf-Bibliotheken per ESP-IDF-Linker-Fragment in einen eigenen
  Abschnitt mit Start-/Ende-Symbolen; beim ersten Programmstart
  Schnappschuss in den PSRAM, bei jedem weiteren Start zurückspielen.
  **Umgesetzt am PC (22.11. abends):** dasselbe Verfahren mit Pragmas
  (`compat/ns_abschnitt.h`, `c/programm_daten.c`); i686-Tests starten
  NetSurf mehrfach im selben Prozess als Kern-Programme. Grösse i686:
  .nsd 364 KB + .nsb 231 KB (inkl. grosser Tabellen, die nie beschrieben
  werden – am Board legt GCC echte Konstanten nach .rodata, die Sicherung
  wird dort kleiner). Zwei Fallen gefunden und behoben: Konstruktoren vor
  main (libnsfb-Flächen) stehen schon in BSS → BSS mitsichern statt
  nullen; LLVM legt nie beschriebene Variablen als Konstanten in denselben
  Abschnitt und macht ihn damit schreibgeschützt → Marke je Datei.
  Startparameter von Programmen sind wie bei KolibriOS auf 256 Zeichen
  begrenzt (lange data:-Adressen würden abgeschnitten).
  Am PC beendet der Kern C-Programme nicht von aussen (das Abwickeln kann
  keine C-Rahmen durchlaufen); Tests schliessen den Browser per Tipp auf
  den Schliessen-Knopf, am Board löscht der Kern den Task.
  Vollständig, ohne Upstream-Änderung. Die PC-Tests starten NetSurf dafür
  je Lauf in einem eigenen Prozess.
- A6 Taskmanager mit echtem Zwangsbeenden: Board hält fremde Programm-Tasks
  mit `vTaskSuspend` an, aber nur ausserhalb von Kern-Code; der Aufräumer
  löscht Task, Stapel, Heap, Fenster, Sockets. „Beenden“ wartet 2 s auf das
  kooperative Ende, dann Zwang. Neu im Startmenü „Task-Manager“; Testprogramm
  `endlostest` (nicht im Menü). Grund für die Umleitung von printf der
  C-Programme auf das Debug-Board: siehe PLAN „A6 Zwangsbeenden nur
  ausserhalb von Kern und Treibern“. Die Anzeigeschleife läuft im Desktop
  mit Priorität 5 über den Programmen (3): eine Endlosschleife blockiert die
  Bedienung nicht.
- A6-Review (Subagent, vorgezogen wegen der heiklen Nebenläufigkeit) fand
  zwei schwere Lücken: Anhalten mit gesetzter Heap-Sperre (Aufräumer hängt)
  und globale Sperren ausserhalb des Kern-Abschnitts (WLAN-Suche, Update,
  Sockets → WLAN/Update für immer blockiert). Behoben mit geschützten
  Abschnitten (`kern::geschuetzt`), Prüfung vor und nach dem Anhalten,
  Austragen unter der Prozesstabelle und Freigabe durch den Aufräumer;
  danach Board-Lauf wiederholt.

## Was läuft

Stand v1.0 (alle Punkte mit Nachweis in PLAN.md; Abnahme O4 am Board):

- **Start:** Board startet direkt in den ESP32-OS-Desktop (Startbild,
  Symbole, Taskleiste mit WLAN-Anzeige und Uhr, Startmenü mit Ordnern
  Spiele/System/Mehr), Desktop fertig 3,4 s nach dem Einschalten;
  Ersteinrichtung beim ersten Start (Sprache, Tastatur, Uhr, WLAN).
- **Kernschicht:** Prozesse mit eigenem PSRAM-Heap, KolibriOS-
  Systemfunktionen nativ, Original-Skin und -Schriften, KolibriOS-C-
  Programme nativ übersetzt, Compositor mit Dirty-Rects, Absturzschutz
  (Meldung, auch nach CPU-Ausnahme), zwei Firmware-Plätze mit Rollback.
- **Touch:** Tippen, Halten = Kontextmenü, Wischen, Zwei-Finger-Zoom,
  Titelleiste ziehen, Doppeltipp maximiert; Bildschirmtastatur (DE/CH/US)
  öffnet sich bei Textfeldern von selbst; Kopieren/Einfügen; Touchpad-Modus.
- **Programme:** Dateimanager (SD + intern), Bildbetrachter (JPEG über die
  JPEG-Einheit, PNG, GIF, BMP, Wischen, Zoom), Einstellungen (Anzeige, Ton,
  Akku, Datum/Zeit mit NTP, Touch, Hintergrund, Speicher, Sprache, Energie,
  WLAN, System mit Updates), Notizen, Rechner, Uhr mit Wecker und Timer,
  Kalender, Musik (MP3 von der SD), Task-Manager, Bildschirmfoto, Info.
- **WLAN (Wi-Fi 6 über den C6):** Netze suchen und verbinden per Touch,
  gemerkte Netze, automatisch nach dem Neustart, Uhrzeit per NTP.
- **Browser (NetSurf 3.11 + QuickJS-ng):** HTTPS mit Zertifikatsprüfung,
  Adressleiste mit Suche, Tabs, Lesezeichen, Verlauf, Downloads, Zoom,
  Text markieren und kopieren, Formulare mit der Bildschirmtastatur,
  JavaScript (DOM, Ereignisse, Timer, fetch/XHR, Cookies, localStorage),
  Rückfall ohne JavaScript; Scrollen mit 32 Bildern/s.
- **PDF-Betrachter:** PDF von SD/intern (auch aus dem Dateimanager),
  Rollen, Blättern, Zoom per Touch; Text in der Systemschrift.
- **Spiele:** Pudding-Panik, Würfelwelt, Doom, Quake starten vom Desktop
  (Doom/Quake in eigenen Partitionen), zurück per Home-Taste;
  Spieldaten von der SD-Karte (nicht in der Firmware).
- **Updates über WLAN:** Code fertig (HTTPS, Prüfsumme, OTA, Rollback),
  der Abschluss braucht das erste Release (U1, O8).
- **Stabilität:** 1 h Langtest mit Browser und WLAN ohne Absturz (O5).

## Messwerte

- O4 Spiele am Board (23.11., vom Desktop gestartet, Bildrate am Panel):
  Pudding-Panik 55,0, Würfelwelt 27,6, Doom 55,0, Quake 27,5 FPS
  (`docs/nachweise/o4/o4_3…6_*_log.txt`).
- Z1 PDF-Betrachter am Board (23.11.): A4-Seite 566×801 mit JPEG, PNG mit
  Transparenz, Text und Flächen 244 ms (vorher in f64: 2474 ms), Textseite
  124 ms, Diagramm 103 ms, gezoomt 1303×921 85 ms; Datei öffnen 52 ms
  (`docs/nachweise/z1/`).
- O1 Scrollen im Browser (23.11., Board, Wikipedia „ESP32“, je
  Wischbewegung): vorher 12,1 Bilder/s, nachher 32,2 Bilder/s (Verschieben
  27 → 4 ms, Zusammensetzen 21 → 6 ms, Ausgabe 27 → 17 ms je Bild;
  `docs/nachweise/o1/o1_scroll_log.txt`).
- O1 Desktop-Symbole beim Start: `@icon` 923 → 162 ms, Desktop fertig 4,30 →
  3,36 s nach dem Einschalten (`o1_icon_log.txt`).
- O7 Startzeit (23.11., ab Einschalten bis Desktop mit Symbolen): vorher
  5,57 s, nachher 4,16 s. Davon ~2,0 s Bootloader/Laden/PSRAM bis
  „ESP32-OS startet“, 0,13 s bis zu den Einstellungen, ~0,8 s Symbole.
- W8 WLAN-Durchsatz am Board (22.11., 2,4 GHz, −45 dBm, Kanal 6, C6-
  Firmware 0.0.0): TCP PC→Board 11,6 Mbit/s, TCP Board→PC 9,8 Mbit/s,
  UDP PC→Board 24,6 Mbit/s (bei 20 Mbit/s Senderate 7 % Verlust),
  UDP Board→PC 24,3 Mbit/s (`docs/nachweise/w8_durchsatz.txt`).
- W7 interner RAM (22.11.): vorher (Puffer intern) nach dem Start 5 KB
  frei, „mempool OOM“, WLAN-Start scheiterte. Nachher (Puffer, Stapel,
  lwIP im PSRAM; Bildpuffer lassen 64 KB Reserve): beim Spielstart 33 KB
  frei, im Betrieb mit Verbindung 12–20 KB (kurz nach dem Start einmal
  2 KB, während WLAN, Anzeige und NTP gleichzeitig hochfahren). Bildpuffer
  unverändert 61/61 KB intern, 40 KB PSRAM (`w7_speicher.txt`).
- W1/W4: Verbindung 4,5–4,7 s ab Auftrag (Transport zum C6 2,1 s nach dem
  Start), NTP-Abgleich 1–3 s danach.
- B0b: progressives JPEG 640×480 in Software 888 ms (Baseline-Software
  810 ms, Hardware 55,7 ms).

- P3 `@icon` (Desktop) zeichnet am Board in 360–650 ms neu (nach „alles
  neu zeichnen“, z. B. Sprache/Hintergrund). Währenddessen reagiert ein
  anderes Programm verzögert; Tipps in sein eigenes Neuzeichnen gehen wie
  in KolibriOS verloren → in O1 beschleunigen. Erledigt (O1): 162 ms beim
  Start.

- P2 JPEG 640×480 (4:2:0) am Board: Hardware-JPEG 55,8 ms inkl.
  YUV→RGB in Software (63 ms im Betrachter inkl. Lesen), Software-Dekoder
  810 ms. Progressives JPEG: Hardware nicht unterstützt (Code 0x105).
  Bildbetrachter zeichnet 566×259 in 62–143 ms (nächster Nachbar).

- K5 Board (Desktop 640×360, PPA ×2, Panel 55 Hz): leerer Desktop 0 neu
  gezeichnete Bilder/s (Compositor-Prüfung 0,01 ms); Fenster 240×160 im
  Kreis verschieben: 55,0 FPS, Compositor 7,3 ms/Bild für ~95 000 px
  (Dirty-Rects, Doppelpuffer), Streifen + PPA ~10–11 ms Ausgabe, p99
  14,5 ms. Optimierung (Hintergrund unter Fenstern nicht füllen) → O1:
  war schon in B5 erledigt (unter einem deckenden Fenster wird nichts
  gefüllt).
- K1 Board: 40 Programmstarts (normal/Absturz/Speicher voll/beendet) in
  ~5 s, PSRAM-Verlust ab Runde 2: 0 B.

## Detailentscheide

- Arbeitsordner ist dieser Ordner (Vorgabe Nutzer). Code per `git fetch` aus
  dem alten Repo des Vorgängerprojekts übernommen (Remote `lokal`, nur
  lesen); `origin` = GitHub `TheUnkn0wnC0DE/ESP32-OS` (nichts pushen ohne OK),
  das frühere GitHub-Repo heisst als Remote jetzt `pudding-panik`.
  Das alte Repo bleibt unverändert, Branch `konsole` dort lauffähig.
- Die drei nicht committeten Firmware-Dateien aus Übergabe 4.1 (`display.rs`,
  `hw.rs`, `pipeline.rs`) sind hierher kopiert und werden in K0 geprüft.
- Quake-Fix `4af4f06` (`lokal/worktree-wf_dd0ea25e-2d8-1`) und Ton `4444567`
  (`lokal/worktree-wf_dd0ea25e-2d8-2`) sind geholt, noch nicht übernommen.

- Bauen: `bauen_flashen.ps1` nutzt hier Laufwerk `O:` (subst auf dieses
  Repo) und Build-Ordner `C:\eo`, damit es nicht mit dem alten Repo (`P:`,
  `C:\pt`) kollidiert. Aufruf als eigener Prozess
  (`powershell -File … -NurBauen > log 2>&1`), nicht mit `*>&1` – sonst bricht
  PS 5.1 an cargo-stderr ab.
- Spieldaten `DOOM1.WAD` / `pak0.pak` aus dem alten Repo kopiert (gitignored).

- **Kernschicht (K1), Crate `esp32/kern`** (std, Board + PC):
  Programme = statische `ProgrammEintrag { name, heap_kb, stapel_kb, start }`.
  Jedes Programm bekommt einen eigenen Bereich (Board: PSRAM) mit eigenem
  First-Fit-Heap (`heap.rs`). `KernAlloc` ist `#[global_allocator]`: Belegen
  nach Faden-Kontext (Programm-Faden → Programm-Heap; Kern-Code und System →
  System-Heap), Freigeben nach Adresse. Regel: Was ein Programm überlebt,
  wird nur in `kern_abschnitt` belegt. Heap voll → Notbelegung aus dem
  System (geführt, beim Aufräumen frei) und Ende „Speicher voll“.
  Ende/Absturz → Aufräum-Thread räumt jeden beendeten Faden sofort ab
  (Board: Faden hält sich an, Aufräumer löscht ihn und gibt TCB/Stapel frei)
  und gibt den Bereich eines beendeten Programms beim nächsten Durchlauf frei. Board (`firmware/src/kern_board.rs`): statische FreeRTOS-
  Tasks, Stapel im Programm-Heap, TCB intern, Kontext in TLS-Platz 1
  (`CONFIG_FREERTOS_THREAD_LOCAL_STORAGE_POINTERS=3`, pthread nutzt 0), Ende
  per `vTaskDelete(NULL)`, Panik beendet nur den Faden. PC: Threads +
  `catch_unwind`.
- Am Board ist zurzeit die Selbsttest-Firmware (K1) aufgespielt: läuft beim
  Start 5 s Selbsttest, danach normal die Konsole.

- **K2 Fenstersystem:** Puffer je Fenster (RGB565), Compositor in
  `fenster.rs`; logische Auflösung vorerst 640×360 (D2 entscheidet endgültig).
  Schriften aus `char.mt`/`charUni.mt` (Original). Tests mit Erwartungsbildern
  (`tests/erwartet/*.png` + `.fnv`, neu mit `ERWARTET_NEU=1`).
- **K3 C-Programme:** Crate `esp32/kolibri` – Original-C-Quellen in `c/`,
  native `include/sys/ksys.h` (Konverter `werkzeuge/ksys_nativ.py`),
  `kos_syscall` in `kern/src/abi.rs` (nur 32 Bit). PC-Tests mit
  `--target i686-pc-windows-msvc` (clang-cl), Board mit riscv32-esp-elf-gcc.
- KolibriOS-Repo nur als Sparse-Klon unter `extern/` (gitignored). Achtung:
  `git grep`/`ls-tree -l` laden dort alle Blobs nach – nicht benutzen.

## Tests (alle am PC, Ordner `esp32`)

```text
cargo test --profile sim -q -p kern                                    # K1, K2, K4
cargo test --profile sim -q -p kolibri --target i686-pc-windows-msvc   # K3/K7 (C-Programme)
cargo test --profile sim -q -p kolibri                                 # K7 Dekoder, libini, Umlaute
python kolibri/werkzeuge/praefix.py name=c/quelle.c …                  # neues C-Programm einbinden
python netsurf/werkzeuge/quellen_holen.py                              # NetSurf-Quellen nach C:\ns (einmal)
cargo test --profile sim -q -p netsurf                                 # B1 Parser/Seite, B7 JavaScript (auch --target i686-pc-windows-msvc)
ERWARTET_NEU=1 …                                                       # Erwartungsbilder neu
powershell -File firmware\bauen_flashen.ps1 -NurBauen -Nur esp32os > log 2>&1
```

- **K5 Anzeige:** Crate `esp32/desktop` = `konsole::Spiel`; `kern::sys::Schirm`
  setzt Dirty-Rects in zwei Puffer zusammen, `FrameData::vollbild565` reicht
  den Puffer ohne Kopie an die Streifen-Pipeline; `Spiel::unveraendert` →
  Firmware zeichnet nichts neu. PPA scharf (`glatt = false`).

- **Board-Builds (Entwicklung):** `ESP32OS_DESKTOP=1` (Desktop statt
  Konsole), `ESP32OS_START=<programm>`, `ESP32OS_AUTOPILOT="ms:x:y:tasten,…"`
  (Touch einspeisen), `KONSOLE_BILDER=s,s` (Fotos, auch bei stehendem Bild),
  `KERN_SELBSTTEST=1` (K1), `KERN_DATEITEST=1` (K6). Neue C-Komponenten:
  `firmware/komponenten/esp32os` (SD), `kolibri/kolibri_c`; das Build-Skript
  baut ESP-IDF neu, wenn dort Dateien neuer sind.
- Am Board zurzeit: Desktop-Testbuild mit basic_gui (K3b).

- **Shell (Crate `desktop`):** `shell.rs` (@start, @icon, @taskbar, @menu,
  info), `gesten.rs` (Touch → Kern-Eingaben, testbar mit Zeitstempeln),
  `tastatur.rs` (@tastatur). Autopilot-Format: „ms:x:y:1“ Finger an (x,y),
  „ms:x:y:0“ los, „ms:x1:y1:x2:y2“ zwei Finger – geht durch die
  Gestenerkennung wie echte Berührung. Fotos blockieren die Schleife ~7 s:
  Zeitplan so legen, dass Tipps nicht in eine Übertragung fallen.
- **Tests mit Gesten:** zwischen Druck- und Loslass-Schritt echte Zeit lassen
  (Programme lesen 37.2), sonst verpassen sie den Klick.

- **Board-Autopilot nur in eigenen Testdaten:** Nach dem Flashen startet
  die Firmware sofort (Autopilot läuft einmal unbeobachtet), `--reset` startet
  ihn ein zweites Mal. Ein abgebrochener erster Lauf verschiebt die Lage der
  Zeilen → Autopilot-Abläufe mit Datei-Änderungen nie blind auf echten Daten;
  Testordner vorher anlegen bzw. Lauf nach dem Flashen abwarten.
- **SD (21.11.):** Vom P1-Board-Test liegen noch der leere Ordner `z` und
  `autoboot (Kopie).txt` (Kopie, 20 B) im SD-Wurzelordner – sonst
  unverändert. FatFs jetzt mit langen Namen (LFN, UTF-8, CP850).
- **Flash (seit B0):** esp32os_a/b je 5,9 MB (ESP32-OS 1,9 MB), doomapp/
  quakeapp je 2 MB (1,7 MB), daten 16 MB.

## Flash-Absturz (22.11. abends, geklärt und behoben)

- **Befund:** Als der Browser beim ersten Start ≈200 KB ins
  Flash-Dateisystem schrieb, hängte sich das Board auf (Watchdog
  `HP_SYS_HP_WDT_RESET`) bzw. stürzte ab (Kern 0 in `read_otadata`,
  Kern 1 mit Store-Fehler).
- **Ursache:** Code und Konstanten laufen aus dem PSRAM
  (`CONFIG_SPIRAM_XIP_FROM_PSRAM`). Dann setzt ESP-IDF
  `SPI_FLASH_CACHE_NO_DISABLE`: Flash-Schreiben/-Löschen über SPI1 nimmt nur
  einen Mutex, sperrt den Cache nicht und hält den anderen Kern nicht an
  (`spi_flash_os_func_app.c`). Die ESP-IDF-Doku nennt das sicher – sie
  übersieht abgebildeten Flash: Wer zur selben Zeit Flash über den Cache
  liest (`esp_partition_mmap`), legt das System lahm. Genau das tun die
  `esp_ota_*`-Funktionen (otadata über mmap, Prüfung des Abbilds in
  `esp_ota_end`), die ESP32-OS beim Start, beim Wechsel zu Doom/Quake und
  beim Update aufruft.
- **Nachstellung** (`firmware/komponenten/esp32os/flashtest.c`,
  Entwicklungs-Build `ESP32OS_FLASHTEST`): Kern 0 löscht/schreibt 64-KB-
  Blöcke im Ende des freien Platzes esp32os_b (nie im Dateisystem), Kern 1
  liest dabei otadata wie `read_otadata`. Ohne Sperre: Watchdog-Neustart in
  den ersten 5 s, bei jedem Start (12 Neustarts in 60 s,
  `docs/nachweise/flash/flash_ohne_sperre_*.txt`).
- **Abhilfe:** Flash-Sperre (`firmware/komponenten/esp32os/flash_sperre.c`):
  hüllt die start/end-Haken des Standard-Flashs, jede SPI1-Operation (FAT,
  NVS, OTA) hält sie; alle Aufrufe, die abgebildeten Flash lesen
  (`esp_ota_*` in `slots.rs`, `update.c`), nehmen sie vorher. Wird als
  Erstes in `main` eingerichtet.
- **Nachweis:** mit Sperre 120 s Dauerlast, 483 Blöcke (≈30 MB gelöscht und
  geschrieben) zwischen 8223 otadata-Abfragen, 0 Fehler, kein Neustart,
  Desktop startet danach normal
  (`docs/nachweise/flash/flash_mit_sperre_120s.txt`). Danach die
  Testreste `/daten/netsurf` (23 Dateien, 177 772 Byte, nur bekannte Namen)
  bei laufendem System gelöscht, ohne Neustart
  (`docs/nachweise/flash/netsurf_testreste_geloescht.txt`).
- **Folge:** Während eines Updates (Löschen des Zielplatzes in
  `esp_ota_begin`) warten andere Schreibvorgänge, bis es fertig ist –
  sie blockieren, statt das System zu gefährden.
- **Doom und Quake (eigene Programme doomapp/quakeapp):** beide binden
  dieselbe Firmware-Komponente ein und richten die Sperre beim Start ein
  (Log „flash_sperre: eingerichtet“; Symbole per `nm` in beiden Abbildern).
  Abgebildeten Flash liest dort nur `esp_ota_*` (Rückkehr zu ESP32-OS) –
  über `slots.rs`, also gesperrt. Die Doom-WAD liest Doom über FAT in den
  PSRAM (kein mmap), Quakes pak0.pak kommt von der SD-Karte; eine Suche im
  ganzen Repo nach `esp_partition_mmap`/`spi_flash_mmap` findet keine
  weiteren Leser.
- **Nachweis Doom:** Entwicklungs-Haken `DOOM_SPEICHERTEST` (nur zur
  Build-Zeit, `doom/doomgeneric/dg_konsole.c` `DG_Speichertest`): neues
  Spiel E1M1, dann während des Spiels alle 2 s speichern, 8-mal
  hintereinander (je 25 350 Byte ins Flash-Dateisystem, mit `stat`
  geprüft), danach normal beenden – zwei Durchläufe, kein Absturz, kein
  Neustart (`docs/nachweise/flash/doom_speichern_waehrend_spiel.txt`).
  **Achtung:** Dieser Test hat die Spielstände 1–6 auf dem Board
  überschrieben (vorher nicht gesichert – Fehler meinerseits). Künftige
  Test-Haken sichern Nutzerdaten vorher oder nutzen eigene Testpfade.

## Interner RAM (22.11. abends, B2)

- **Befund:** Mit Browser und WLAN fiel der interne RAM auf 0 KB. Folgen:
  lwIP-Thread-Semaphoren (`thread_sem_init: out of memory`) → DNS und neue
  Verbindungen scheitern; newlib-Sperre bei `fopen` → `abort`; QuickJS
  `js_mutex_init` → `abort`; Flash-Lesen ohne Hilfspuffer
  (`wl_read failed 0x101`). Schon beim Desktop-Start waren nur 32 KB frei.
- **Ursache:** `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=16384` (Standard): jeder
  malloc-Block unter 16 KB – auch jede kleine Rust-Allokation von Kern und
  Desktop – kommt zuerst in den internen RAM, bis er voll ist.
- **Abhilfe:** Schwelle 1 KB (`sdkconfig.defaults`). Gemessen: Desktop-Start
  104 KB frei (vorher 32), beim Laden von Wikipedia mindestens 48 KB, danach
  80 KB; Desktop weiter 33 FPS. Dazu der Abrufer mit 2 festen Arbeiter-Tasks
  (Stapel im PSRAM) statt eines Tasks je Abruf.
- Doom/Quake bauen mit derselben Einstellung, sobald sie neu gebaut werden –
  dort beim nächsten Bau prüfen.

## Offen

- **O8 Veröffentlichung:** wartet auf dein OK (siehe „Fragen an dich“);
  damit auch der Abschluss von U1 (Update a→b am Board).
- **Später (v1.1, PLAN):** P5 Standby und Akku (Messgerät, Spannungsteiler),
  W2 C6-Firmware, W5 iTWT, Z2 Kamera-App (kein Kameramodul am Board).
- **Wenn Zeit bleibt:** erledigt – PDF-Betrachter (Z1), PasswordGen und
  TinyHashView (Z3); Kamera-App in v1.1.
- Bekannte Grenzen stehen unter „Abweichungen“ in PLAN.md (u. a. Web-
  Funktionen B7, Markieren am Board B8, Bildbetrachter knapp 0,85 MP).
- Startparameter von Programmen (ESP32OS_START) starten vor den
  Desktop-Diensten; ein grosses Programm dort (Browser) kann die Dienste
  verdrängen (Fehler 30) – nur Entwicklungs-Builds betroffen.
- B1-Beobachtung: Einmal scrollte die Willkommensseite zwischen zwei Fotos
  ohne erkennbare Eingabe zum Suchfeld. Seither nicht wieder beobachtet;
  die späteren Läufe (B2–B10, O1, O4) starteten den Browser aber meist
  direkt mit einer Adresse, die Willkommensseite selbst wurde selten
  geprüft – weiter beobachten.
