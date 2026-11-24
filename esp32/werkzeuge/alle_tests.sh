#!/bin/bash
# Alle Testsuiten vor einem Commit: 64 Bit kern, kolibri, desktop, netsurf;
# i686 (32 Bit wie das Board) kern, kolibri, netsurf und die 32-Bit-Tests des
# Desktops. Eine Ergebniszeile je Suite, am Ende FEHLER=0 oder 1.
#   bash esp32/werkzeuge/alle_tests.sh
cd "$(dirname "$0")/.." || exit 1
fehler=0
lauf() {
  name="$1"; shift
  aus=$("$@" 2>&1)
  r=$?
  ergebnis=$(echo "$aus" | grep -E "^test result" | awk '{p+=$4; f+=$6} END {print p" ok, "f" fehlgeschlagen"}')
  echo "$name: Rückgabe $r, $ergebnis"
  if [ $r -ne 0 ]; then
    fehler=1
    echo "$aus" | grep -E "^error|panicked|FAILED|failures:" | head -20
  fi
}
lauf "64 kern" cargo test --release -p kern
lauf "64 kolibri" cargo test --release -p kolibri
lauf "64 desktop" cargo test --release -p desktop
lauf "i686 kern" cargo test --release -p kern --target i686-pc-windows-msvc
lauf "i686 kolibri" cargo test --release -p kolibri --target i686-pc-windows-msvc
lauf "i686 desktop (C-Programme, Startmenü, Stabilität)" cargo test --release -p desktop --target i686-pc-windows-msvc --test c_programme --test thashview --test menue_start --test stabil --test browser_menue --test konsole_touch --test standby_touch --test ersteinrichtung_kreuz --test fps_lauf --test fenster_raender
cd netsurf || exit 1
lauf "64 netsurf" cargo test --release
lauf "i686 netsurf" cargo test --release --target i686-pc-windows-msvc
echo "FEHLER=$fehler"
