# Pressefotos am Board: je Szene ein Entwicklungs-Build, der das Programm
# beim Start (bzw. nach dem WLAN-Verbinden) öffnet, Fotos aus dem
# Framebuffer (bildschirmfoto.py) nach <Ziel>\<szene>\. Auswahl und Prüfung
# auf private Daten (WLAN-Name, IP, Namen) danach von Hand, die gewählten
# Bilder kommen nach docs/press/. Nur eigene Testdateien (Musik_Test,
# PDF_Test); das Wetter zeigt einen festen Ort, ohne ihn zu speichern.
# Oberfläche englisch (ESP32OS_SPRACHE, nur im Build). Am Ende normale
# Firmware (esp32os, bei Spielen auch doomapp/quakeapp).
#
#   powershell -File esp32\werkzeuge\board\presse_lauf.ps1 -Ziel C:\pfad\presse [-Szenen startmenue,browser]
param(
    [Parameter(Mandatory)][string]$Ziel,
    [string[]]$Szenen = @('startmenue', 'browser', 'dateimanager', 'pdf', 'wetter', 'taskmanager', 'musik', 'doom', 'quake'),
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

function Bauen([string]$Nur, [string]$Log) {
    & powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur $Nur -Port $Port -Anhalten *> $Log
    if ($LASTEXITCODE -ne 0) { Umgebung-Leeren; Write-Host "BAU FEHLER $Nur (siehe $Log)"; exit 1 }
}

# Szene: Umgebung (Name=Wert), Fotozeiten, Laufzeit; Spiel = doomapp/quakeapp
$alle = [ordered]@{
    'desktop'      = @{ Umg = @{}; Bilder = '20'; Sek = 45 }
    'startmenue'   = @{ Umg = @{ ESP32OS_AUTOPILOT = '20000:40:340:1,20120:40:340:0' }; Bilder = '24'; Sek = 50 }
    'browser'      = @{ Umg = @{ ESP32OS_START_NACH_WLAN = 'browser https://en.wikipedia.org/wiki/ESP32' }; Bilder = '60,80'; Sek = 110 }
    'browser_hn'   = @{ Umg = @{ ESP32OS_START_NACH_WLAN = 'browser https://news.ycombinator.com/' }; Bilder = '45,70'; Sek = 100 }
    'browser_wiki' = @{ Umg = @{ ESP32OS_START_NACH_WLAN = 'browser https://en.m.wikipedia.org/wiki/ESP32' }; Bilder = '60,90,120'; Sek = 150 }
    'dateimanager' = @{ Umg = @{ ESP32OS_START = 'dateimanager /sys' }; Bilder = '20'; Sek = 45 }
    'pdf'          = @{ Umg = @{ ESP32OS_PDF_TEST = '1'; ESP32OS_START = 'pdfbetrachter /sd0/1/ESP32-OS/PDF_Test/quickstart.pdf' }; Bilder = '25'; Sek = 50 }
    'wetter'       = @{ Umg = @{ ESP32OS_START_NACH_WLAN = 'wetter zeigen:46.948:7.4474:Bern' }; Bilder = '45,60'; Sek = 90 }
    'taskmanager'  = @{ Umg = @{ ESP32OS_START = 'info;matrix;passwordgen;taskmanager' }; Bilder = '25'; Sek = 50 }
    'musik'        = @{ Umg = @{ ESP32OS_MUSIK_TEST = '1'; ESP32OS_START = 'musik /sd0/1/ESP32-OS/Musik_Test/Test tone.mp3' }; Bilder = '5'; Sek = 40 }
    'appstore'     = @{ Umg = @{ ESP32OS_START = 'appstore' }; Bilder = '15'; Sek = 40 }
    'ogg'          = @{ Umg = @{ ESP32OS_MUSIK_TEST = '1'; ESP32OS_OGG_TEST = $env:PRESSE_OGG; ESP32OS_START = 'musik /sd0/1/ESP32-OS/Musik_Test/Test.ogg' }; Bilder = '1,2'; Sek = 35 }
    'doom'         = @{ Umg = @{ KONSOLE_START = 'doom' }; Spiel = 'doomapp'; Bilder = '35,50,65'; Sek = 90 }
    'quake'        = @{ Umg = @{ KONSOLE_START = 'quake' }; Spiel = 'quakeapp'; Bilder = '40,60,80'; Sek = 110 }
}

$spiele = @()
# mit -File kommt die Liste als ein Text („a,b,c“)
$Szenen = @($Szenen | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
foreach ($s in $Szenen) {
    $sz = $alle[$s]
    if (-not $sz) { Write-Host "Unbekannte Szene $s"; exit 1 }
    $z = "$Ziel\$s"
    New-Item -ItemType Directory -Force $z | Out-Null
    Umgebung-Leeren
    if ($sz.Spiel) {
        # Spielprogramm mit Fotozeiten, ESP32-OS startet direkt ins Spiel
        $env:KONSOLE_BILDER = $sz.Bilder
        Bauen $sz.Spiel "$z\bau_spiel.log"
        Umgebung-Leeren
        $spiele += $sz.Spiel
    } else {
        $env:KONSOLE_BILDER = $sz.Bilder
    }
    foreach ($k in $sz.Umg.Keys) { Set-Item "Env:$k" $sz.Umg[$k] }
    # Oberfläche englisch (nur in diesem Build, die Einstellung bleibt)
    $env:ESP32OS_SPRACHE = 'en'
    Bauen 'esp32os' "$z\bau.log"
    Umgebung-Leeren
    cmd /c "python `"$esp32\firmware\bildschirmfoto.py`" $($sz.Sek) `"$z`" $Port --reset --name $s > `"$z\board_log.txt`" 2>&1"
    & python "$esp32\firmware\werkzeuge\log_schwaerzen.py" "$z\board_log.txt" | Out-Null
    $log = Get-Content "$z\board_log.txt" -Encoding UTF8
    $abst = @($log | Select-String -Pattern 'Guru Meditation|abort\(\) was called|PANIK:|Speicher voll').Count
    $bilder = @(Get-ChildItem "$z\$s*.png" -ErrorAction SilentlyContinue).Count
    Write-Host "$s - Bilder $bilder, Abstuerze $abst"
}

# normale Firmware zurück
Umgebung-Leeren
foreach ($sp in ($spiele | Select-Object -Unique)) {
    & powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur $sp -Port $Port *> "$Ziel\bau_normal_$sp.log"
    if ($LASTEXITCODE -ne 0) { Write-Host "NORMALES $sp NICHT GESCHRIEBEN (siehe $Ziel\bau_normal_$sp.log)"; exit 1 }
}
& powershell -NoProfile -File "$esp32\firmware\bauen_flashen.ps1" -Nur esp32os -Port $Port *> "$Ziel\bau_normal.log"
if ($LASTEXITCODE -ne 0) { Write-Host "NORMALE FIRMWARE NICHT GESCHRIEBEN (siehe $Ziel\bau_normal.log)"; exit 1 }
Write-Host "FERTIG"
