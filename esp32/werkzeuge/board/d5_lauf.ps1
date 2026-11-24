# D5 am Board: Absturzschutz. Startet drei Mal absturz_test (Schreiben auf 0,
# Lesen von 4, unerlaubter Befehl). Erwartet: drei Mal „Hardware-Fehler in
# Programm … nur dieses Programm wird beendet“, kein Neustart, der Desktop
# meldet die Abstürze (Foto nach 30 s). Danach normale Firmware.
#
#   powershell -File esp32\werkzeuge\board\d5_lauf.ps1 -Ziel C:\pfad\d5
param(
    [Parameter(Mandatory)][string]$Ziel,
    [string]$Port = 'COM3'
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
# drei Programme mit absichtlichem Hardware-Fehler gleich nach dem Start
$env:ESP32OS_START = 'absturz_test;absturz_test lesen;absturz_test befehl'
$env:KONSOLE_BILDER = '30'
$sek = 60
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
$zeilen = @("D5 Absturzschutz $(Get-Date -Format s), Port $Port")
$summe = 0
foreach ($k in $muster.Keys) {
    $n = @($log | Select-String -Pattern $muster[$k]).Count
    # der erste Boot-ROM-Start ist der Reset zu Beginn
    if ($k -eq 'Neustart (Boot-ROM)') { $n = [Math]::Max(0, $n - 1) }
    $zeilen += ('{0,-30} {1}' -f $k, $n)
    $summe += $n
}
$abgefangen = @($log | Select-String -Pattern 'Hardware-Fehler in Programm').Count
$zeilen += ('{0,-30} {1}' -f 'abgefangen (erwartet 3)', $abgefangen)
$zeilen += "Summe der Neustart-Gründe: $summe"
$zeilen | Set-Content "$Ziel\auswertung.txt" -Encoding UTF8
$zeilen

# Board wieder mit der normalen Firmware (ohne Autopilot)
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
# „Programm abgestürzt“ zählt hier mit (erwartet); Fehler sind Neustarts
$neustart = @($log | Select-String -Pattern 'Guru Meditation|^rst:0x').Count - 1
if ($abgefangen -ne 3 -or $neustart -ne 0) { "NICHT BESTANDEN: abgefangen $abgefangen, Neustarts $neustart"; exit 2 }
"FERTIG: 3 Fehler abgefangen, kein Neustart"
