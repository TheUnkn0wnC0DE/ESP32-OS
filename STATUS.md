# STATUS – ESP32-OS

Stand: 22.11.2026 morgens, Repo: dieser Ordner, Branch `esp32-os` (von
`konsole` @ `44b79f2`). Die Vorgeschichte (Spiele, Firmware, Messwerte,
Übergabe vom 21.11.) liegt im alten Repo des Vorgängerprojekts.

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

## Fragen an dich (Nacht 23.11.)

- **B5, 30 Bilder/s beim Scrollen:** erreicht sind ~12 Bilder/s (vorher
  2–4); der Rest hängt am Verschieben von 265 KB im PSRAM je Bild (Abweichung
  in PLAN.md, mit dem Weg zu 30). Soll ich später (z. B. bei H/Optimierung)
  den grösseren Umbau angehen – NetSurf zeichnet direkt in den Fensterpuffer
  des Kerns, Verschieben über die PPA-Einheit?

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
Ein Update vom P4 aus über SDIO geht erst ab C6-Firmware 2.6 – also einmal
per Kabel direkt am C6. **Vorher am Schaltplan von Waveshare prüfen**
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

- Kernschicht K0–K4 fertig (siehe PLAN.md mit Nachweisen): Prozesse mit
  eigenem PSRAM-Heap, KolibriOS-Systemfunktionen (Fenster, Zeichnen, Text,
  Knöpfe, Ereignisse, Maus, Zeit, Fäden, Speicher), native ksys.h mit
  laufendem Original-C-Programm, Original-Skin Shkvorka mit Touch-Faktor 2.
- Am Board läuft weiterhin die Konsole (Menü 55 FPS, Pudding-Panik 55,
  Doom 55, Würfelwelt 27,5, Quake 27,5); der Desktop kommt mit K5/D1.
- K5: Desktop läuft am Board (Build mit `ESP32OS_DESKTOP=1`); Standard-
  Start bleibt bis D1 die Konsole.
- K3b: `basic_gui.c` läuft am Board (Komponente `kolibri_c`).
- K6: Dateisystem am Board: interne Partition `/sys` = `/rd/1`, microSD
  (SDMMC-Slot 0, 4 Bit, LDO 4) `/sd0/1`.
- K7: Systembibliotheken nativ im Crate `kolibri` (box_lib, libimg,
  proc_lib, libini) + libc-Aufsatz für C-Programme.
- Block A (Kernschicht) inkl. Review fertig.
- D1/D2: Board startet direkt in den ESP32-OS-Desktop (Startbild, Symbole,
  Taskleiste, Startmenü, Info). Symbol „Spiele“ → alter Launcher (einmalig).
- D3–D9: Gesten (Tippen, Halten = Kontextmenü, Wischen, Zoom, Titel ziehen,
  Doppeltipp maximiert), Bildschirmtastatur CH (@tastatur, automatisch bei
  Textfeld-Fokus), Kopieren/Einfügen per Kontextmenü (Funktion 54).
- D10–D12: Touchpad-Modus mit Original-Zeiger, Tastenprüfung, Absturz-
  meldung (auch nach Neustart bei CPU-Ausnahme). Nächster Punkt: D-Review.

### Bedienmasse (D2)

Logisch 640×360, PPA ×2 scharf → Panel 1280×720 quer, ≈ 294 ppi:
1 logischer Pixel = 2 Panel-Pixel = 0,173 mm.

| Element | logisch | mm |
|---|---|---|
| Fensterkopf (Skin ×2) | 48 px hoch | 8,3 |
| Schliessen/Minimieren | 54×36 | 9,3×6,2 |
| Taskleiste | 48 px hoch (Knöpfe 41 px) | 8,3 (7,1) |
| Tastatur-Taste | 46 px Raster, sichtbar 42 px | 7,3 |
| Kontextmenüzeile | 44 px (Knopf 42 px) | 7,3 |
| Startknopf / Fensterknopf | 108×41 / 126×41 | 18,7×7,1 / 21,8×7,1 |
| Desktop-Symbol (Zelle) | 84×84 (Symbol 48) | 14,5 (8,3) |
| Menüzeile | 44 px | 7,6 |
| Dialogknöpfe (proc_lib) | 110×40 | 19×6,9 |
| Text 8×16 | 16 px | 2,8 |
| Text 6×9 | 9 px | 1,6 |

## Messwerte

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
  in KolibriOS verloren → in O1 beschleunigen.

- P2 JPEG 640×480 (4:2:0) am Board: Hardware-JPEG 55,8 ms inkl.
  YUV→RGB in Software (63 ms im Betrachter inkl. Lesen), Software-Dekoder
  810 ms. Progressives JPEG: Hardware nicht unterstützt (Code 0x105).
  Bildbetrachter zeichnet 566×259 in 62–143 ms (nächster Nachbar).

- K5 Board (Desktop 640×360, PPA ×2, Panel 55 Hz): leerer Desktop 0 neu
  gezeichnete Bilder/s (Compositor-Prüfung 0,01 ms); Fenster 240×160 im
  Kreis verschieben: 55,0 FPS, Compositor 7,3 ms/Bild für ~95 000 px
  (Dirty-Rects, Doppelpuffer), Streifen + PPA ~10–11 ms Ausgabe, p99
  14,5 ms. Optimierung (Hintergrund unter Fenstern nicht füllen) → O1.
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

- Alles ab K5 gemäss `PLAN.md`.
- Startparameter von Programmen (ESP32OS_START) starten vor den
  Desktop-Diensten; ein grosses Programm dort (Browser) kann die Dienste
  verdrängen (Fehler 30) – nur Entwicklungs-Builds betroffen.
- B1: Bei einem Lauf scrollte die Willkommensseite zwischen zwei Fotos ohne
  erkennbare Eingabe zum Suchfeld (Fokus im Feld). Im nächsten Lauf
  (Autopilot) nicht wieder aufgetreten; weiter beobachten (V1).
