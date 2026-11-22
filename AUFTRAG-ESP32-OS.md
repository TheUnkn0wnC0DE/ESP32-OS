# Auftrag: ESP32-OS – KolibriOS nativ auf der ESP32-P4-Konsole

Name des Systems: **ESP32-OS** (überall so nennen: Startbildschirm, Info-Dialog, Repo, Doku).

Lies zuerst nur diese Datei und `STATUS.md`. Was hier entschieden ist, nicht
nochmals diskutieren oder nachfragen – umsetzen. Detailentscheide selbst
treffen und kurz in `STATUS.md` notieren. Arbeite selbständig durch, bis alles
unter Punkt 9 erfüllt ist.

## 0. Als Erstes (genau in dieser Reihenfolge)

1. Abschnitt 11 („Dauerregeln“) wörtlich in die `CLAUDE.md` im Repo-Hauptordner
   übernehmen (ergänzen, falls sie schon existiert). So gelten die Regeln auch
   nach `/clear` und nach automatischem Komprimieren.
2. `PLAN.md` anlegen: vollständige Checkliste **aller** Punkte aus 4, 5, 6 und
   9, in der Reihenfolge aus 8. Jeder Punkt mit „fertig, wenn …“ und wie du es
   nachweist. Darunter ein leerer Abschnitt „Abweichungen“.
3. Mir `PLAN.md` einmal kurz zeigen und auf mein OK warten. Danach ohne weitere
   Rückfragen durcharbeiten (Ausnahme: Regel „gleicher Fehler zweimal“).

## 1. Ziel

KolibriOS läuft **nativ, ohne Emulator**, auf meinem Board und ist so
optimiert wie möglich. Das Board startet direkt in den ESP32-OS-Desktop und
ist ein Gerät für den Alltag: Bilder anschauen, Dateien, Notizen, Musik,
Einstellungen, **Webbrowser**, **WLAN (Wi-Fi 6)** und meine Spiele.

**Alles wird nur per Touch bedient.** Es wird nie eine Tastatur, Maus oder
sonst etwas angeschlossen. Jede Funktion muss von A bis Z mit dem Finger
erreichbar sein.

## 2. Fester Weg

Kernel, Treiber und viele Programme von KolibriOS sind x86-FASM und laufen auf
RISC-V nicht. Deshalb:

1. **Kernel = native Neuumsetzung** auf meiner bestehenden Firmware
   (ESP-IDF 5.5, Rust + C): KolibriOS-Systemfunktionen (sysfuncs) mit gleichem
   Verhalten nachbauen. Referenz: `sysfuncs.txt` im KolibriOS-Repo
   (git.kolibrios.org; Pfad per Suche finden).
2. **Original-Look** über die Originaldaten (Skins `.skn`, Systemschriften,
   Farben, Fensterrahmen, Taskleiste) – parsen, nicht nachzeichnen. Für Touch
   vergrössert (siehe 4).
3. **C-Programme aus dem KolibriOS-Repo / Ports-Repo** für RISC-V neu
   kompilieren, gegen eine eigene Fassung des System-Headers (z. B. `ksys.h`,
   per grep finden), die statt `int 0x40` die native Funktion aufruft.
4. **Systembibliotheken**, die C-Programme brauchen und die in FASM sind
   (z. B. box_lib, libimg, proc_lib, libini, http, network), nativ in C mit
   gleicher Schnittstelle nachbauen – nur so weit, wie gewählte Programme sie
   brauchen.
5. **Fehlt ein Alltagsprogramm als portierbarer C-Code**, schreibst du es
   selbst nativ im KolibriOS-Stil (gleiche API, gleicher Look).
6. **Meine Spiele** als Programme einbinden (Trait `Spiel`). Doom/Quake: meine
   bestehenden Ports nehmen, nicht die KolibriOS-Ports.

Nicht-Ziele: x86-Emulator, FASM-Kernel übersetzen, POSIX, Mehrbenutzer,
Bluetooth, angeschlossene Eingabegeräte.

## 3. Hardware (fix)

Waveshare ESP32-P4-WIFI6-Touch-LCD-5: ESP32-P4 (RISC-V, 2 HP-Kerne,
Hardware-JPEG), 768 KB HP-SRAM, 32 MB PSRAM, 32 MB Flash, 5"-IPS 720×1280
MIPI-DSI (HX8394, hochkant, quer per Software, ≈ 294 ppi), Touch GT911
(5 Punkte), Ton ES8311, microSD (SDIO 3.0), Akku. WLAN/BT über
ESP32-C6-Koprozessor an SDIO.

## 4. Touch-Bedienung (Pflicht, überall)

- Tippen = Klick, lange drücken = Rechtsklick/Kontextmenü, wischen = scrollen,
  zwei Finger auseinander/zusammen = zoomen (Browser, Bildbetrachter),
  Titelleiste ziehen = Fenster verschieben, Doppeltipp Titel = maximieren.
- **Bildschirmtastatur** als Systemdienst: geht automatisch auf, sobald ein
  Textfeld den Fokus bekommt (Hook in der nativen box_lib-Editbox und im
  Browser), zusätzlich jederzeit über die Taskleiste. Deutsch-Schweizer
  Belegung mit Umlauten, Sonderzeichen-Ebene.
- Optional zuschaltbar: Touchpad-Modus (Finger bewegt Zeiger relativ) für
  winzige Ziele in alten Programmen.
- Bedienelemente mindestens ca. 7–8 mm gross, Text gut lesbar. Logische
  Auflösung und Skalierung wählst du danach (PPA skaliert).
- Nichts darf eine Taste verlangen: Kopieren/Einfügen per Kontextmenü,
  Dialoge (auch Quakes „y“-Abfrage, siehe STATUS 5.1) per Touch-Knopf,
  Ersteinrichtung (WLAN, Uhrzeit) komplett per Touch.

## 5. Programme

Pflicht:

- **Dateimanager** (SD-Karte + interne Partition, kopieren, verschieben,
  löschen, umbenennen, Ordner).
- **Bildbetrachter** (JPG über Hardware-JPEG, PNG, GIF, BMP; wischen = nächstes
  Bild, zoomen).
- **Webbrowser:** NetSurf (KolibriOS-Port unter `programs/network/netsurf`
  oder Upstream mit Framebuffer-Frontend – was schneller zum Ziel führt).
  HTTPS über mbedTLS mit Zertifikatsbündel, Netzwerk über esp_http_client
  oder lwIP, Cache im PSRAM begrenzt, Adressleiste + Lesezeichen + Verlauf,
  Touch-Scrollen und Zoom.
- **Einstellungen:** WLAN, Helligkeit, Lautstärke, Akku, Datum/Uhrzeit (NTP),
  Touch, Hintergrundbild, Speicherplatz.
- **Texteditor/Notizen**, **Rechner**, **Uhr mit Wecker und Timer**,
  **Kalender**, **Musikplayer** (MP3 von SD), **Taskmanager**.
- Meine Spiele: Pudding-Panik, Würfelwelt, Doom, Quake.

Wenn Zeit bleibt: PDF-Betrachter, Kamera-App (OV5647 an MIPI-CSI), weitere
KolibriOS-C-Programme.

## 6. WLAN (Wi-Fi 6)

- ESP32-C6 über `esp_hosted` (SDIO) + `esp_wifi_remote`. C6-Firmware auf die
  passende esp_hosted-Version bringen (Versionsgleichheit Host/Slave), Update
  vom P4 aus ermöglichen.
- Alles, was der C6 kann: 802.11ax (2,4 GHz), WPA2/WPA3, Netze suchen,
  verbinden, bekannte Netze speichern, automatisch wiederverbinden, iTWT zum
  Stromsparen, Signalstärke in der Taskleiste.
- Transportpuffer in den PSRAM (`ESP_HOSTED_MEMPOOL_PREFER_SPIRAM`), um
  internen RAM zu sparen. Durchsatz mit iperf messen und in `STATUS.md`
  notieren.
- KolibriOS-Netzwerkfunktionen (Sockets, DNS) auf lwIP abbilden, damit
  KolibriOS-Netzwerkprogramme laufen.

## 7. Bestehendes wiederverwenden (nicht neu schreiben)

Branch `konsole` (siehe `STATUS.md`): Display-Pipeline mit PPA und
Zwei-Kern-Streifen-Renderer, Touch, Ton/Synth, NVS, Partitionen,
PC-Simulator `esp32/sim`. Neue Arbeit auf eigenem Branch `esp32-os`;
`konsole` muss lauffähig bleiben. Quake-Fix `4af4f06` und die
Firmware-Änderungen aus STATUS 4.1 testen und übernehmen; Worktrees 3/4
ignorieren, `4444567` (Ton) optional.

Architektur-Vorgaben (Details deine Wahl): Programme als FreeRTOS-Tasks im
gemeinsamen Adressraum mit eigenem Heap im PSRAM; zuerst statisch eingelinkt,
später optional ELF-Lader von SD (prüfen, ob `espressif/elf_loader` den P4
kann). Neu zeichnen nur, was sich ändert (Dirty-Rects); Vollbildspiele direkt.
Dateisystem: SD (FAT) + interne Partition auf KolibriOS-Pfade abbilden.

## 8. Reihenfolge und Sparregeln

Arbeitsreihenfolge (wichtigstes zuerst, damit bei knappem Budget das
Wichtigste fertig ist): Kernschicht → Desktop/Taskleiste/Touch/
Bildschirmtastatur → Dateimanager, Bildbetrachter, Einstellungen →
WLAN → Browser → übrige Alltagsprogramme → Spiele → Optimierung → Langtest.

Budget: Das 5-Stunden-Limit darf voll ausgenutzt werden. Das **Weekly-Limit
muss unter 90 % bleiben (höchstens 92 %)**. Ich prüfe den Verbrauch; wenn ich
„sparen“ sage, Umfang kürzen statt weitermachen.

- Keine parallelen Subagenten, kein /batch, keine Agent-Teams. Erlaubt sind
  nur: ein Subagent für eine einmalige breite Suche, wenn das Kontext spart,
  und die Review-Subagenten aus Abschnitt 11 (immer einer nach dem anderen).
- KolibriOS-Repos **nie** komplett lesen: sparse checkout, `grep` zuerst,
  Dateien nur in Zeilenbereichen, keine Datei zweimal lesen.
- Build- und Logausgaben filtern (`-q`, nur Fehler, `tail`/`grep`).
- Zuerst im PC-Simulator testen; Board nur flashen, wenn ein Teil fertig ist.
- Funktionierenden Code nicht ohne Grund umbauen.
- Wenn der Kontext voll oder unübersichtlich wird: `STATUS.md` aktualisieren,
  committen, mit frischem Kontext weiter (STATUS.md als Übergabe).
- Gleicher Fehler zweimal ungelöst → stoppen und mich fragen, nicht raten.
- Antworten knapp halten, keine langen Zusammenfassungen.

## 9. Abnahme (wann fertig)

- Board startet direkt in den ESP32-OS-Desktop (KolibriOS-Look, touchtauglich).
- Alle Programme aus Punkt 5 (Pflicht) laufen und sind nur per Touch
  vollständig bedienbar, inkl. Texteingabe über die Bildschirmtastatur.
- WLAN: Netz per Touch suchen und verbinden, bleibt nach Neustart verbunden.
- Browser lädt normale HTTPS-Seiten (z. B. Wikipedia), scrollen und zoomen
  per Touch, Texteingabe in Formularen.
- Pudding-Panik, Würfelwelt, Doom und Quake starten aus dem Desktop
  (Neustart in eigene Partition erlaubt, falls RAM es verlangt), FPS nicht
  schlechter als heute (55 / 27,5 / 55 / 27,5).
- 1 h Langtest am Board ohne Absturz (inkl. Browser und WLAN).
- `STATUS.md` aktuell: was läuft, Messwerte, was offen ist.

## 10. Lizenz

Übernommener KolibriOS- und NetSurf-Code und -Daten sind GPL-2.0 – im Repo
vermerken.

## 11. Dauerregeln (in CLAUDE.md übernehmen)

- Bei jedem Start, nach `/clear` und nach dem Komprimieren: zuerst `PLAN.md`
  und `STATUS.md` lesen, dann beim ersten offenen Punkt weitermachen.
  Der vollständige Auftrag steht in `AUFTRAG-ESP32-OS.md` – bei Zweifeln
  dort nachlesen (nur den betroffenen Abschnitt).
- Immer nur am aktuellen Punkt arbeiten. Nichts überspringen, nichts ohne
  Grund vorziehen.
- Ein Punkt wird erst abgehakt, wenn der Nachweis da ist: Test- oder
  Build-Ausgabe, Bildschirmfoto aus dem Framebuffer (`bildschirmfoto.py`)
  oder Messwert. Den Nachweis in einer Zeile beim Punkt in `PLAN.md` notieren.
- Keine Platzhalter: In abgehakten Teilen gibt es kein `todo!()`,
  `unimplemented!()`, keine leeren Stubs und kein „TODO“ – vor dem Abhaken
  per grep prüfen.
- Nichts still weglassen oder vereinfachen. Geht etwas nicht wie geplant:
  unter „Abweichungen“ in `PLAN.md` eintragen (was, warum, Ersatz) und es mir
  sagen. Kein Punkt wird stillschweigend gestrichen.
- Touch-Abläufe automatisch testen: Touch-Ereignisse per Autopilot bzw.
  serieller Schnittstelle einspeisen und das Bildschirmfoto prüfen. Nie
  annehmen, dass etwas per Touch geht, ohne es so getestet zu haben.
- Nach jedem grossen Teil (Kernschicht, Touch/Bildschirmtastatur, WLAN,
  Browser, Alltagsprogramme, Spiele): ein Review-Subagent prüft nur den Diff
  gegen `PLAN.md` und Abschnitt 9 des Auftrags. Nur Befunde zu Korrektheit
  und Anforderungen beheben, keine Stilfragen.
- An schwierigen Stellen (Architektur der Kernschicht, NetSurf-Port,
  esp_hosted/WLAN, Absturzsuche) eine eigene Zeile ausgeben:
  `EFFORT: xhigh empfohlen` – und danach `EFFORT: zurück auf high`.
- Bevor der Kontext voll ist: `PLAN.md` und `STATUS.md` aktualisieren und
  committen.
- Sparregeln aus Abschnitt 8 des Auftrags gelten immer.
- „Fertig“ erst melden, wenn alle Punkte abgehakt sind – mit der Liste aller
  Punkte und ihrem Nachweis.

## 12. Ergänzungen (21.11.2026)

Gelten wie der restliche Auftrag, jeweils mit Nachweis. In Klammern der Block
in `PLAN.md`.

1. **Sprache** (ab sofort, auch für P1): Oberfläche auf Englisch als
   Standard, Deutsch in den Einstellungen umschaltbar. Was schon gebaut ist
   (Desktop, Taskleiste, Startmenü, Dialoge), nachziehen. Bildschirmtastatur
   zusätzlich mit US-Layout, umschaltbar.
2. **Standby und Akku** (Block C, bei den Einstellungen): Bildschirm nach
   einstellbarer Zeit aus, Ruhezustand, Aufwachen per Power-Knopf oder Touch,
   Akkustand in der Taskleiste. Stromverbrauch im Standby messen.
3. **Screenshot** (Block C): Per Taskleiste oder Geste ein Bildschirmfoto
   als PNG auf die SD-Karte speichern.
4. **JPEG** (P2): Prüfen, ob die Hardware-JPEG-Einheit progressive JPEGs
   kann. Falls nicht, braucht es spätestens für den Browser einen
   Software-Decoder dafür.
5. **Flash neu aufteilen** (vor B1): Vorher alle Daten auf dem Board sichern
   (Würfelwelt-Welt, NVS-Spielstände, Einstellungen) und danach
   zurückspielen, mit Nachweis. Dabei gleich Platz für Updates über WLAN
   einplanen (OTA-Platz, automatisches Zurückspringen auf die alte Version,
   wenn die neue nicht startet).
6. **WLAN-Passwort und Sicherheit** (Block D): WLAN-Passwort zum Testen nur
   in einer lokalen Datei, die in `.gitignore` steht. Keine Passwörter oder
   Schlüssel im Repo oder in Commits. Die Zertifikatsprüfung bei HTTPS nie
   abschalten.
7. **Updates** (nach Block D): In den Einstellungen „Nach Updates suchen“.
   Lädt die neueste Version vom GitHub-Release über HTTPS, prüft die
   Prüfsumme, spielt sie ein und startet neu. Versionsnummer unter „Info“
   anzeigen.
8. **Doom und Quake** (Block G): `DOOM1.WAD` und `pak0.pak` kommen nicht ins
   Release. Beim ersten Start von der SD-Karte laden oder per WLAN
   herunterladen, mit Hinweis auf die Shareware-Lizenz.
9. **Startzeit** (Block H): Zeit bis zum Desktop messen, optimieren und in
   `STATUS.md` eintragen.
10. **GitHub-Veröffentlichung** (letzter Punkt): `LICENSE` mit GPL-2.0,
    README auf Englisch mit Fotos und Flash-Anleitung, Release v1.0 mit
    fertiger Firmware, Web-Flasher. Nichts pushen ohne OK des Auftraggebers.

## 13. Antworten und Änderungen (22.11.2026)

Gelten wie der restliche Auftrag, jeweils mit Nachweis. **Reihenfolge:**
zuerst Punkt 4 und 7, dann der Browser (Punkt 9), danach der Rest vom Plan.

1. **Strommessung und Akkuteiler** macht der Auftraggeber, wenn er den Akku
   einlötet. In `STATUS.md` eine kurze Anleitung (welcher GPIO, welche
   Widerstände, wie messen); bis dahin bleibt es offen.
2. **C6-Firmware:** jetzt nicht, WLAN läuft. Schritt-für-Schritt-Anleitung
   für später, inklusive Rückweg, falls es schiefgeht.
3. **Updates:** Kein Release und kein Push. Die Update-Funktion mit einem
   lokalen Testserver auf dem PC nachweisen. Ein Test-Zertifikat gibt es nur
   im Test-Build, im normalen Build bleibt die Prüfung gegen GitHub. Der
   echte Release kommt erst am Schluss mit OK des Auftraggebers.
4. **Touch oben links:** Auf dem Display liegt nichts, es ist nichts dran –
   die Ursache liegt in der Software oder beim Touch-Controller. GT911-
   Rohdaten auslesen und die Ursache finden (z. B. Statusregister nicht
   zurückgesetzt, Punkt 0/0 als Berührung gewertet, Konfiguration, Störungen
   über USB). Nachweis: 10 Minuten ohne Berührung, keine einzige gemeldete
   Berührung.
5. **Keine eFuses.** Das WLAN-Passwort bleibt unverschlüsselt; in der README
   als bekannte Einschränkung erwähnen.
6. **SD-Testreste und `C:\ns\src`:** löschen.
7. **Verlorene Einstellungen:** Ursache finden und beheben. Einstellungen
   sicher speichern (zuerst in eine temporäre Datei, dann umbenennen) und
   eine Sicherungskopie behalten. Nachweis: Stromausfall während des
   Speicherns simulieren.
8. **Spiele:** Der Knopf „Spiele“ öffnet nicht mehr den alten Launcher (der
   soll in ESP32-OS ganz weg). Block G: Pudding-Panik, Würfelwelt, Doom und
   Quake je als eigenes Programm im Startmenü (Ordner „Spiele“) und als
   Symbol auf dem Desktop. Start im Vollbild, mit der Home-Taste zurück in
   den Desktop. Doom und Quake kehren nach dem Beenden direkt in den
   ESP32-OS-Desktop zurück, nie in den alten Launcher.
9. **Browser:** ein eigener, guter Browser, der direkt auf dem Board läuft
   (ohne Pi, ohne Fernmodus) und mit dem auch Webseiten mit JavaScript gehen.
   Der Weg ist frei (bestehende Teile nehmen oder ersetzen, z. B. NetSurf für
   HTML/CSS und eine moderne JavaScript-Engine wie QuickJS-ng, oder selbst
   schreiben). Entscheidend: möglichst viele echte Webseiten funktionieren.
   - JavaScript: modernes JavaScript (ES2020 und neuer) und die häufigsten
     Web-Funktionen (DOM, Ereignisse, Timer, fetch/XHR, localStorage).
     Speicher pro Seite begrenzen, damit der Browser nie das System
     mitreisst.
   - Bedienung: Tabs, Adressleiste mit Suche, Lesezeichen, Verlauf,
     Downloads, Zoom, Touch-Scrollen, Text markieren und kopieren, Formulare
     mit Bildschirmtastatur, Cookies, HTTPS mit Zertifikatsprüfung,
     progressive JPEGs.
   - Nachweis: Liste mit 20 echten Webseiten, von einfach bis
     JavaScript-lastig (z. B. Wikipedia, Suchmaschine, Nachrichtenseite,
     GitHub, Seiten mit Formularen und Menüs). Alle am Board testen und in
     `STATUS.md` eine Tabelle eintragen: geht, geht teilweise, geht nicht,
     mit Grund. Danach verbessern, was am meisten bringt, und die Tabelle
     aktualisieren.
   - An schwierigen Stellen wie bisher „EFFORT: xhigh empfohlen“ ausgeben.
