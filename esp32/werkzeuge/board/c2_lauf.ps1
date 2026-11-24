# C2 (W2) am Board: eigene C6-Firmware vom P4 aus schreiben (esp_hosted-OTA,
# ohne USB-UART-Adapter). Nur mit OK des Nutzers. Baut die C6-Firmware
# (werkzeuge/c6_bauen.ps1), dann ESP32-OS mit ESP32OS_C6_OTA (C6-Bild
# eingebettet; Update einmal je Build, NVS-Merker „c6_ota“). Der C6 schreibt
# in seinen freien App-Platz, die alte Firmware bleibt im anderen; nach dem
# Neustart Probezeit 90 s – meldet sich der P4 nicht, schaltet der C6 selbst
# zurück. Erwartet im Log: „C6-Update: fertig“, danach Versionen gleich
# (3.1.0). Danach normale Firmware auf den P4 (der C6 behält die neue).
# Vorher/Nachher-Messung: werkzeuge/durchsatz.py (ESP32OS_DURCHSATZ-Build).
#
#   powershell -File esp32\werkzeuge\board\c2_lauf.ps1 -Ziel C:\pfad\c2
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
# C6-Firmware frisch bauen, dann ESP32-OS mit eingebettetem C6-Bild
& powershell -NoProfile -File "$esp32\werkzeuge\c6_bauen.ps1" *> "$Ziel\c6_bau.log"
if ($LASTEXITCODE -ne 0) { Umgebung-Leeren; "C6-BAU FEHLER (siehe $Ziel\c6_bau.log)"; exit 1 }
$env:ESP32OS_C6_OTA = '1'
$env:KONSOLE_BILDER = '200'
$sek = 240
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
$zeilen = @("C2 C6-Update vom P4 $(Get-Date -Format s), Port $Port")
$summe = 0
foreach ($k in $muster.Keys) {
    $n = @($log | Select-String -Pattern $muster[$k]).Count
    # der erste Boot-ROM-Start ist der Reset zu Beginn
    if ($k -eq 'Neustart (Boot-ROM)') { $n = [Math]::Max(0, $n - 1) }
    $zeilen += ('{0,-30} {1}' -f $k, $n)
    $summe += $n
}
$fertig = @($log | Select-String -Pattern 'C6-Update: fertig').Count
$gleich = @($log | Select-String -Pattern 'coprocessor=3\.1\.0 \(match\)|Koprozessor esp32c6 3\.1\.0 .* gleich').Count
$zurueck = @($log | Select-String -Pattern 'zurück zur alten Firmware').Count
$zeilen += ('{0,-30} {1}' -f 'Update geschrieben', $fertig)
$zeilen += ('{0,-30} {1}' -f 'Versionen gleich (3.1.0)', $gleich)
$zeilen += "Summe der Absturz-Gründe: $summe"
$zeilen | Set-Content "$Ziel\auswertung.txt" -Encoding UTF8
$zeilen

# Board wieder mit der normalen Firmware (ohne Autopilot)
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
# (Neustarts gehören hier dazu: der P4 startet nach dem Update neu)
if ($fertig -lt 1 -or $gleich -lt 1) { "NICHT BESTANDEN: geschrieben $fertig, gleich $gleich – alte C6-Firmware bleibt bzw. Rückweg (siehe Log)"; exit 2 }
"FERTIG: C6 mit esp_hosted 3.1.0, Versionen gleich"
