# BR1/BR2: Seitenliste am Board messen (Entwicklungs-Build ESP32OS_SEITENLISTE,
# firmware/src/seitenliste.rs): je Seite ein frischer Browser, -Sek Sekunden,
# Foto 46 s nach dem Start jeder Seite (bild_<i>.png), danach beendet.
# Auswertung: tabelle.md (br_auswertung.py; Spalten Ergebnis/Grund nach
# Sicht der Fotos ausfüllen). Danach wieder die normale Firmware aufs Board.
#
#   powershell -File esp32\werkzeuge\board\br_lauf.ps1 -Ziel C:\pfad\br1
param(
    [Parameter(Mandatory)][string]$Ziel,
    [int]$Sek = 60,
    [string]$Liste = '',
    [int]$Anzahl = 0,
    [string]$Port = 'COM3',
    # Entwicklung: Heap-Spur bzw. Browser ohne JavaScript
    [switch]$Spur,
    [switch]$OhneJs
)
# Logs als UTF-8 (PowerShell 5 schriebe mit *> UTF-16)
$env:PYTHONIOENCODING = 'utf-8'
# $PSScriptRoot ist in Standardwerten von param() leer (25.11.: 0 Seiten)
if (-not $Liste) { $Liste = "$PSScriptRoot\br_seiten.txt" }
$esp32 = Resolve-Path "$PSScriptRoot\..\.."
New-Item -ItemType Directory -Force $Ziel | Out-Null
$alle = @(Get-Content $Liste | Where-Object { $_ -and -not $_.StartsWith('#') })
if ($Anzahl -gt 0) { $alle = @($alle | Select-Object -First $Anzahl) }
$n = $alle.Count

function Umgebung-Leeren {
    Get-ChildItem Env: | Where-Object { $_.Name -like 'ESP32OS_*' -or $_.Name -like 'KONSOLE_*' } |
        ForEach-Object { Remove-Item "Env:$($_.Name)" }
}

Umgebung-Leeren
$env:ESP32OS_SEITENLISTE = $alle -join ' '
$env:ESP32OS_SEITENLISTE_S = "$Sek"
if ($Spur) { $env:ESP32OS_HEAP_TRACE = '1' }
if ($OhneJs) { $env:ESP32OS_JS_AUS = '1' }
# erste Seite 20 s nach dem Start, Fotozeit ab Schleifenstart ~3 s
$env:KONSOLE_BILDER = (0..($n - 1) | ForEach-Object { 20 + $_ * $Sek + 46 - 3 }) -join ','
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port -Anhalten *> "$Ziel\bau.log"
$rc = $LASTEXITCODE
Umgebung-Leeren
if ($rc -ne 0) { "BAU FEHLER $rc (siehe $Ziel\bau.log)"; exit 1 }
$dauer = 20 + $n * $Sek + $(if ($Spur) { 80 } else { 20 })
cmd /c "python `"$esp32\firmware\bildschirmfoto.py`" $dauer `"$Ziel`" $Port --reset > `"$Ziel\board_log.txt`" 2>&1"
& python "$esp32\firmware\werkzeuge\log_schwaerzen.py" "$Ziel\board_log.txt"
& python -I "$PSScriptRoot\br_auswertung.py" "$Ziel\board_log.txt" | Set-Content "$Ziel\tabelle.md" -Encoding UTF8

# Board wieder mit der normalen Firmware
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
"FERTIG ($n Seiten, Tabelle $Ziel\tabelle.md)"
