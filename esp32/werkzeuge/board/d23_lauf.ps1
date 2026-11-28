# D2/D3 am Board.
# Lauf 1 (D2, nur mit -OggDatei): Entwicklungs-Build legt die Ogg-Datei nach
#   ESP32-OS/Musik_Test (eigener Testordner) und startet den Musikplayer
#   damit; Nachweis im Log („spielt Test.ogg“, ausgegebene Frames), Foto.
# Lauf 2 (D3): App-Store startet beim Booten; per Autopilot Doom entfernen
#   (zweimal tippen), aus dem neuesten GitHub-Release wieder installieren
#   (HTTPS, SHA-256), PasswordGen entfernen und wieder installieren – am Ende
#   ist alles wie vorher. Fotos. Danach normale Firmware.
#
#   powershell -File esp32\werkzeuge\board\d23_lauf.ps1 -Ziel C:\pfad\d23 [-OggDatei C:\pfad\x.ogg]
param(
    [Parameter(Mandatory)][string]$Ziel,
    [string]$OggDatei = '',
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

function Lauf([string]$Name, [int]$Sek) {
    $z = "$Ziel\$Name"
    New-Item -ItemType Directory -Force $z | Out-Null
    # englische Oberfläche (die Fotos dienen auch als Pressefotos; nur im
    # Build, die Einstellung auf dem Board bleibt)
    $env:ESP32OS_SPRACHE = 'en'
    & powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port -Anhalten *> "$z\bau.log"
    $rc = $LASTEXITCODE
    Umgebung-Leeren
    if ($rc -ne 0) { Write-Host "BAU FEHLER $rc (siehe $z\bau.log)"; exit 1 }
    cmd /c "python `"$esp32\firmware\bildschirmfoto.py`" $Sek `"$z`" $Port --reset > `"$z\board_log.txt`" 2>&1"
    & python "$esp32\firmware\werkzeuge\log_schwaerzen.py" "$z\board_log.txt" | Out-Null
    $log = Get-Content "$z\board_log.txt" -Encoding UTF8
    $abst = @($log | Select-String -Pattern 'Guru Meditation|abort\(\) was called|PANIK:|abgestürzt|Speicher voll').Count
    $neu = [Math]::Max(0, @($log | Select-String -Pattern '^rst:0x').Count - 1)
    Write-Host "$Name - Abstürze $abst, Neustarts $neu"
    if ($abst -or $neu) { $script:fehler++ }
}

$fehler = 0
if ($OggDatei) {
    Umgebung-Leeren
    $env:ESP32OS_MUSIK_TEST = '1'
    $env:ESP32OS_OGG_TEST = $OggDatei
    $env:ESP32OS_START = 'musik /sd0/1/ESP32-OS/Musik_Test/Test.ogg'
    $env:KONSOLE_BILDER = '16'
    Lauf 'd2' 50
    $log = Get-Content "$Ziel\d2\board_log.txt" -Encoding UTF8
    $spielt = @($log | Select-String -Pattern 'Musik: spielt Test\.ogg').Count
    $frames = @($log | Select-String -Pattern 'Test\.ogg bei .*ausgegeben [1-9]').Count
    "D2: spielt $spielt, Fortschritt $frames"
    if ($spielt -lt 1 -or $frames -lt 1) { $fehler++ }
}

Umgebung-Leeren
$env:ESP32OS_START = 'appstore'
# Knöpfe wie desktop/tests/appstore.rs (knopf_mitte): Zeile 0 Doom, 4 PasswordGen
$env:ESP32OS_AUTOPILOT = '25000:494:77:1,25120:494:77:0,40000:494:77:1,40120:494:77:0,60000:494:77:1,60120:494:77:0,160000:494:237:1,160120:494:237:0,200000:494:237:1,200120:494:237:0'
$env:KONSOLE_BILDER = '30,55,80,170,205,255'
Lauf 'd3' 290
$log = Get-Content "$Ziel\d3\board_log.txt" -Encoding UTF8
$entfernt = @($log | Select-String -Pattern 'APP-STORE: doomapp entfernt').Count
$installiert = @($log | Select-String -Pattern 'APP-STORE: doomapp\.bin installiert und geprüft').Count
"D3: entfernt $entfernt, installiert $installiert"
if ($entfernt -ne 1 -or $installiert -ne 1) { $fehler++ }

# Board wieder mit der normalen Firmware (ohne Autopilot)
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
if ($fehler) { "NICHT BESTANDEN ($fehler)"; exit 2 }
"FERTIG"
