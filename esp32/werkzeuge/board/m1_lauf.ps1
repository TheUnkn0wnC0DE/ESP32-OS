# M1 am Board: Mess-Build für die Tabelle „wer hält wie viel internen RAM“.
# Schaltet vorübergehend CONFIG_HEAP_TASK_TRACKING und
# CONFIG_FREERTOS_USE_TRACE_FACILITY ein (nie so committen – am Ende stellt
# das Skript sdkconfig.defaults byte-genau wieder her, auch bei Fehlern), baut
# mit dem Autopilot der Stabilitätsrunde (eine Runde: alle Programme,
# Browser, Menü) und Berichten (Tasks mit Stapel-Lage, Heap je Task, interne
# Heap-Bereiche) nach 40 s (Ruhe nach dem Start), 200 s und 430 s (Last).
# Danach normale Firmware.
#
#   powershell -File esp32\werkzeuge\board\m1_lauf.ps1 -Ziel C:\pfad\m1
#   (-NurBauen: nur den Mess-Build bauen, nichts schreiben – Prüfung am PC)
param(
    [Parameter(Mandatory)][string]$Ziel,
    [string]$Port = 'COM3',
    [switch]$NurBauen
)
$env:PYTHONIOENCODING = 'utf-8'
$esp32 = Resolve-Path "$PSScriptRoot\..\.."
$defaults = "$esp32\firmware\sdkconfig.defaults"
New-Item -ItemType Directory -Force $Ziel | Out-Null

function Umgebung-Leeren {
    Get-ChildItem Env: | Where-Object { $_.Name -like 'ESP32OS_*' -or $_.Name -like 'KONSOLE_*' } |
        ForEach-Object { Remove-Item "Env:$($_.Name)" }
}

$rc = 1
# genauer Inhalt vorher (auch nicht committete Änderungen bleiben)
$vorher = [System.IO.File]::ReadAllBytes($defaults)
try {
    Add-Content -Path $defaults -Encoding UTF8 -Value @(
        '', '# M1 Mess-Build (vorübergehend, m1_lauf.ps1)',
        'CONFIG_HEAP_TASK_TRACKING=y', 'CONFIG_FREERTOS_USE_TRACE_FACILITY=y')
    Umgebung-Leeren
    $env:ESP32OS_SPEICHERBERICHT = '40,200,430'
    if (-not $NurBauen) {
        # als Datei (eine Umgebungsvariable wäre zu lang, max. 32767 Zeichen)
$env:ESP32OS_AUTOPILOT_DATEI = (Resolve-Path "$PSScriptRoot\stabil_autopilot.txt").Path
        & powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port -Anhalten *> "$Ziel\bau.log"
    } else {
        & powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -NurBauen *> "$Ziel\bau.log"
    }
    $rc = $LASTEXITCODE
} finally {
    Umgebung-Leeren
    [System.IO.File]::WriteAllBytes($defaults, $vorher)
}
if ($rc -ne 0) { "BAU FEHLER $rc (siehe $Ziel\bau.log)"; exit 1 }
if ($NurBauen) { "MESS-BUILD GEBAUT (sdkconfig.defaults wiederhergestellt)"; exit 0 }

cmd /c "python `"$esp32\firmware\bildschirmfoto.py`" 460 `"$Ziel`" $Port --reset > `"$Ziel\board_log.txt`" 2>&1"
& python "$esp32\firmware\werkzeuge\log_schwaerzen.py" "$Ziel\board_log.txt"
Select-String -Path "$Ziel\board_log.txt" -Pattern 'BERICHT|TASK|TASKS:|HEAPTASK|MAINBLOCK|SPEICHER t=|Heap|heap|WLAN: Transport|Start: intern|Programm-Pool' -Encoding UTF8 |
    ForEach-Object { $_.Line } | Set-Content "$Ziel\bericht.txt" -Encoding UTF8

# Board wieder mit der normalen Firmware
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
"FERTIG: Bericht in $Ziel\bericht.txt"
