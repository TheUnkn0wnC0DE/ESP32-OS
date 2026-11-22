# CLAUDE.md – ESP32-OS

Arbeit am Projekt **ESP32-OS** (KolibriOS nativ auf der ESP32-P4-Konsole).
Repo: dieser Ordner (Repo-Wurzel), Branch `esp32-os`. Nur hier arbeiten,
nicht im alten Repo (Vorgängerprojekt). Auftrag: `AUFTRAG-ESP32-OS.md`,
Plan: `PLAN.md`, Stand/Übergabe: `STATUS.md`.

## Dauerregeln (aus Abschnitt 11 des Auftrags, wörtlich)

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

## Projektnotizen

- Vor jedem Commit, der `esp32/kern` oder `esp32/firmware` ändert: Firmware
  bauen (`bauen_flashen.ps1 -NurBauen -Nur esp32os`, als eigener Prozess) und
  das Ergebnis abwarten. RISC-V-32 hat keine 64-Bit-Atomics.
- Vor jedem Commit alle Tests laufen lassen (64 Bit: kern, kolibri, desktop;
  i686: kern, kolibri) – erst bei grün committen. Geänderte Erwartungsbilder
  ansehen, dann mit `ERWARTET_NEU=1` erneuern.
- Öffentlich auf GitHub (`origin` = TheUnkn0wnC0DE/ESP32-OS): Commits nur
  mit der Identität aus der lokalen `.git/config` (TheUnkn0wnC0DE, noreply)
  und in UTC (`TZ=UTC git commit …`), damit keine Zeitzone sichtbar wird.
  Keine echten Namen, Pfade mit Benutzernamen, WLAN-Daten in Dateien.
- `main` hat keine alte Geschichte (eigener Anfangscommit). Neue Stände von
  `esp32-os` kommen als eigener Commit darauf:
  `git commit-tree <esp32-os-Commit>^{tree} -p main -m …`, dann
  `git branch -f main <neu>`. Pushen nur nach OK des Nutzers.
- Der alte Remote heisst `pudding-panik` – dorthin nie pushen.
