# Stabilitätsrunde am Board (SB/SC/SD, Gesamtauftrag 24.11.)
#
# Baut ESP32-OS mit dem Autopilot aus stabil_autopilot.txt (erzeugt vom
# PC-Test desktop/tests/stabil.rs mit STABIL_AUTOPILOT_NEU=1: alle Programme
# über das Startmenü öffnen, Titelleiste ziehen, im Browser Inhalt
# antippen, über das Kreuz schliessen, Menü oft auf/zu – 6 Runden, 38 min),
# schreibt es aufs Board, schneidet das Log mit (Fotos alle 10 min),
# zählt Abstürze und schreibt danach wieder die normale Firmware (ohne
# Autopilot) aufs Board. Bedient wird nur Harmloses (keine Knöpfe, die
# Daten ändern); Einstellungen und WLAN bleiben.
#
#   powershell -File esp32\werkzeuge\board\stabil_lauf.ps1 -Ziel C:\pfad\sb1
param(
    [Parameter(Mandatory)][string]$Ziel,
    [string]$Port = 'COM3',
    [int]$Minuten = 40
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
$env:ESP32OS_AUTOPILOT = (Get-Content "$PSScriptRoot\stabil_autopilot.txt" -Raw).Trim()
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
$zeilen = @("Stabilitätsrunde $(Get-Date -Format s), $Minuten min, Port $Port")
$summe = 0
foreach ($k in $muster.Keys) {
    $n = @($log | Select-String -Pattern $muster[$k]).Count
    # der erste Boot-ROM-Start ist der Reset zu Beginn
    if ($k -eq 'Neustart (Boot-ROM)') { $n = [Math]::Max(0, $n - 1) }
    $zeilen += ('{0,-30} {1}' -f $k, $n)
    $summe += $n
}
$zeilen += "Summe: $summe"
$zeilen | Set-Content "$Ziel\auswertung.txt" -Encoding UTF8
$zeilen

# Board wieder mit der normalen Firmware (ohne Autopilot)
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
if ($summe -ne 0) { exit 2 }
"FERTIG: 0 Abstürze"
