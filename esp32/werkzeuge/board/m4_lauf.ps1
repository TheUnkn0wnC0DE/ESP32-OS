# M4 am Board: Dauertest ≥ 60 min (Vorrang Stabilität und Speicher). Autopilot
# aus dauertest_autopilot.txt (PC-Test desktop/tests/stabil.rs mit
# STABIL_AUTOPILOT_NEU=1: 10 Runden à 416 s – alle Programme öffnen,
# bedienen, schliessen, Menü oft auf/zu, Browser mit Seiten über Links,
# Startknopf aus dem Browser). Zählt Abstürze und vergleicht den freien
# internen RAM am Anfang (t=20 s) und am Ende (Speicherwacht-Zeilen).
# Danach normale Firmware.
#
#   powershell -File esp32\werkzeuge\board\m4_lauf.ps1 -Ziel C:\pfad\m4
param(
    [Parameter(Mandatory)][string]$Ziel,
    [string]$Port = 'COM3',
    [int]$Minuten = 75
)
# Logs als UTF-8 (PowerShell 5 schriebe mit *> UTF-16)
$env:PYTHONIOENCODING = 'utf-8'
$esp32 = Resolve-Path "$PSScriptRoot\..\.."
New-Item -ItemType Directory -Force $Ziel | Out-Null

function Umgebung-Leeren {
    Get-ChildItem Env: | Where-Object { $_.Name -like 'ESP32OS_*' -or $_.Name -like 'KONSOLE_*' } |
        ForEach-Object { Remove-Item "Env:$($_.Name)" }
}

Umgebung-Leeren
# als Datei (eine Umgebungsvariable wäre zu lang, max. 32767 Zeichen)
$env:ESP32OS_AUTOPILOT_DATEI = (Resolve-Path "$PSScriptRoot\dauertest_autopilot.txt").Path
$sek = $Minuten * 60
$env:KONSOLE_BILDER = ((1..([int]($Minuten / 10))) | ForEach-Object { $_ * 600 }) -join ','
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port -Anhalten *> "$Ziel\bau.log"
$rc = $LASTEXITCODE
Umgebung-Leeren
if ($rc -ne 0) { "BAU FEHLER $rc (siehe $Ziel\bau.log)"; exit 1 }

cmd /c "python `"$esp32\firmware\bildschirmfoto.py`" $sek `"$Ziel`" $Port --reset > `"$Ziel\board_log.txt`" 2>&1"
& python "$esp32\firmware\werkzeuge\log_schwaerzen.py" "$Ziel\board_log.txt"

# Auswertung: Abstürze und Neustarts zählen
$log = Get-Content "$Ziel\board_log.txt" -Encoding UTF8
$muster = [ordered]@{
    'Guru Meditation (CPU-Fehler)' = 'Guru Meditation'
    'abort()'                     = 'abort\(\) was called'
    'Rust-Panik'                  = 'PANIK:'
    'Programm abgestürzt'         = 'abgestürzt|Absturz von'
    'Speicher voll'               = 'Speicher voll|SpeicherVoll'
    'Neustart (Boot-ROM)'         = '^rst:0x'
}
$zeilen = @("M4 Dauertest $(Get-Date -Format s), $Minuten min, Port $Port")
$summe = 0
foreach ($k in $muster.Keys) {
    $n = @($log | Select-String -Pattern $muster[$k]).Count
    # der erste Boot-ROM-Start ist der Reset zu Beginn
    if ($k -eq 'Neustart (Boot-ROM)') { $n = [Math]::Max(0, $n - 1) }
    $zeilen += ('{0,-30} {1}' -f $k, $n)
    $summe += $n
}
$zeilen += "Summe: $summe"
# Startfehler („Programm nicht gestartet“) und Reste je Runde: kein Absturz,
# aber die Runde lief nicht wie geplant (Dauertest 27.11.)
$startfehler = @($log | Select-String -Pattern 'Start \S+: Fehler').Count
$reste = @($log | Select-String -Pattern 'offen geblieben \[.+\]').Count
$zeilen += ('{0,-30} {1}' -f 'Startfehler', $startfehler)
$zeilen += ('{0,-30} {1}' -f 'Runden mit offenen Resten', $reste)
# Leck: interner RAM bei t=20 s (Desktop, WLAN, vor dem Autopiloten) gegen
# die letzte Zeile (alle Programme zu, Autopilot fertig)
$speicher = @($log | Select-String -Pattern 'SPEICHER t=(\d+)s intern frei (\d+) KB' | ForEach-Object {
    [pscustomobject]@{ t = [int]$_.Matches[0].Groups[1].Value; frei = [int]$_.Matches[0].Groups[2].Value } })
$anfang = $speicher | Where-Object { $_.t -eq 20 } | Select-Object -First 1
$ende = $speicher | Select-Object -Last 1
$tief = ($speicher | Measure-Object -Property frei -Minimum).Minimum
if ($anfang -and $ende) {
    $zeilen += "interner RAM frei: Anfang $($anfang.frei) KB (t=20 s), Ende $($ende.frei) KB (t=$($ende.t) s), tiefster Wert $tief KB, Differenz $($ende.frei - $anfang.frei) KB"
} else {
    $zeilen += "interner RAM: Speicherzeilen fehlen im Log"
}
$zeilen | Set-Content "$Ziel\auswertung.txt" -Encoding UTF8
$zeilen

# Board wieder mit der normalen Firmware (ohne Autopilot)
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
if ($summe -ne 0) { "NICHT BESTANDEN: $summe Absturz-Gründe"; exit 2 }
if (-not ($anfang -and $ende) -or $ende.frei -lt $anfang.frei - 4) { "NICHT BESTANDEN: interner RAM am Ende tiefer als am Anfang (Leck?)"; exit 3 }
if ($startfehler -ne 0 -or $reste -ne 0) { "NICHT BESTANDEN: $startfehler Startfehler, $reste Runden mit offenen Resten"; exit 4 }
"FERTIG: 0 Abstürze, interner RAM am Ende wie am Anfang"
