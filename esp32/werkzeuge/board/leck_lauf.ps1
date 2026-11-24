# M4 Lecksuche am Board: Heap-Spur (CONFIG_HEAP_TRACING_STANDALONE, nur
# vorübergehend – sdkconfig.defaults wird byte-genau wiederhergestellt) mit
# dem Autopiloten der Stabilitätsrunde. Spur ab 60 s, Ausgabe nach 885 s
# (Ende der 2. Runde, alle Programme zu): noch belegte interne Blöcke nach
# Inhalt gruppiert (bericht.txt). Danach normale Firmware.
#
#   powershell -File esp32\werkzeuge\board\leck_lauf.ps1 -Ziel C:\pfad\leck
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
        '', '# M4 Lecksuche (vorübergehend, leck_lauf.ps1)',
        'CONFIG_HEAP_TRACING_STANDALONE=y')
    Umgebung-Leeren
    $env:ESP32OS_HEAP_TRACE = '1'
    $env:ESP32OS_HEAPSPUR_ENDE = '885'
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

cmd /c "python `"$esp32\firmware\bildschirmfoto.py`" 960 `"$Ziel`" $Port --reset > `"$Ziel\board_log.txt`" 2>&1"
& python "$esp32\firmware\werkzeuge\log_schwaerzen.py" "$Ziel\board_log.txt"
Select-String -Path "$Ziel\board_log.txt" -Pattern 'HEAPSPUR|SPEICHER t=' -Encoding UTF8 |
    ForEach-Object { $_.Line } | Set-Content "$Ziel\bericht.txt" -Encoding UTF8

# Board wieder mit der normalen Firmware
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
"FERTIG: Bericht in $Ziel\bericht.txt"
