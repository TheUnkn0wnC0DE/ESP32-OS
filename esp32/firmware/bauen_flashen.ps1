# Baut die Firmware für das Waveshare ESP32-P4-WIFI6-Touch-LCD-5 und flasht sie.
#
# Drei Programme (eigene App-Partitionen, siehe partitions.csv, seit B0):
# „esp32os“ (ESP32-OS mit Konsole, Pudding-Panik, Würfelwelt; OTA-Platz
# esp32os_a, Updates gehen nach esp32os_b), „doomapp“ und „quakeapp“.
# Standard: alle drei bauen und aufspielen. „factory“ gilt weiter als
# anderer Name für „esp32os“.
#
#   powershell -File esp32\firmware\bauen_flashen.ps1                 # alle Programme
#   powershell -File esp32\firmware\bauen_flashen.ps1 -Nur esp32os    # nur eines (esp32os/doomapp/quakeapp)
#   powershell -File esp32\firmware\bauen_flashen.ps1 -Bin hwtest     # Hardware-Test
#   powershell -File esp32\firmware\bauen_flashen.ps1 -NurBauen
#   powershell -File esp32\firmware\bauen_flashen.ps1 -Monitor 60     # 60 s Log mitschreiben
#   powershell -File esp32\firmware\bauen_flashen.ps1 -Daten abbild.bin # FAT-Abbild „daten“ aufspielen
#     (Abbild aus werkzeuge\daten_abbild.py; enthält u. a. DOOM1.WAD). Die
#     Quake-PAK liegt auf der SD-Karte unter ESP32-OS\quake\pak0.pak.
#   powershell -File esp32\firmware\bauen_flashen.ps1 -Wlan           # mit Testnetz (W0)
#     Liest SSID=… und PASSWORT=… aus wlan.local im Repo-Wurzelordner (in
#     .gitignore). Die Werte gehen nur als Build-Variablen in die Firmware,
#     dieses Skript gibt sie nie aus.
#   powershell -File esp32\firmware\bauen_flashen.ps1 -Release -NurBauen  # Release-Firmware (O8)
#     Release-Firmware darf NIE WLAN-Zugangsdaten enthalten. -Release prüft das
#     dreifach und bricht sonst ab:
#     1. -Release zusammen mit -Wlan wird abgelehnt.
#     2. Ist vor dem Start irgendeine Umgebungsvariable ESP32OS_WLAN_* gesetzt
#        (Sitzung, Benutzer oder System), bricht das Skript ab, statt sie still
#        zu entfernen – die Firmware liest SSID/PASSWORT per option_env! beim
#        Übersetzen, ESP32OS_WLAN_VERGESSEN gehört auch nicht in ein Release.
#     3. Nach dem Bau wird jedes erzeugte Abbild (.elf und .bin) Byte für Byte
#        nach SSID und Passwort aus wlan.local durchsucht (falls die Datei
#        existiert); ein Treffer löscht die Abbilder und bricht ab. Ausgegeben
#        werden nie die Werte, nur das Ergebnis.
#     Cargo baut neu, wenn sich eine per option_env! gelesene Variable ändert;
#     Punkt 3 fängt trotzdem jeden Rest aus einem früheren -Wlan-Bau ab.
#
# Windows-Besonderheiten (alle hier erledigt):
# * esp-idf-sys verlangt kurze Pfade (Ausgabeordner ≤ 88 Zeichen) → Build-Ordner C:\eo.
# * Liegt der Build-Ordner außerhalb des Projekts, findet esp-idf-sys die
#   Einstellungen in Cargo.toml nur über CARGO_WORKSPACE_DIR.
# * Ein "%" im Projektpfad bricht Batchdateien von ESP-IDF → das Projekt
#   wird über ein virtuelles Laufwerk (subst O:) angesprochen.
# * Komponenten aus der Registry (esp_hosted) haben sehr lange Pfade →
#   Zwischenspeicher C:\ec (IDF_COMPONENT_CACHE_PATH), esp_hosted selbst als
#   gekürzte Kopie unter C:\ec (werkzeuge\komponenten_holen.py).
# * bindgen braucht libclang (winget install LLVM.LLVM).
# Werkzeuge: rustup (Toolchain aus rust-toolchain.toml), cargo install ldproxy espflash.
param(
    [ValidateSet('pudding-p4', 'hwtest')][string]$Bin = 'pudding-p4',
    [string]$Port = 'COM3',
    [ValidateSet('alle', 'esp32os', 'factory', 'doomapp', 'quakeapp')][string]$Nur = 'alle',
    [switch]$NurBauen,
    [string]$Daten = '',
    [switch]$Wlan,
    # Release-Firmware: ohne WLAN-Zugangsdaten, mit Prüfung (siehe Kopf)
    [switch]$Release,
    # Board nach dem Aufspielen nicht starten (erster Start erst beim
    # Mitschnitt, z. B. bildschirmfoto.py --reset)
    [switch]$Anhalten,
    [int]$Monitor = 0
)
$danach = @()
if ($Anhalten) { $danach = @('--after', 'no-reset') }
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# Release (O8): nie mit WLAN-Zugangsdaten (Prüfungen 1 und 2, siehe Kopf)
if ($Release) {
    if ($Wlan) { throw 'Release: -Wlan ist verboten (Release-Firmware ohne WLAN-Zugangsdaten)' }
    $gesetzt = @()
    foreach ($ziel in 'Process', 'User', 'Machine') {
        $alle = [Environment]::GetEnvironmentVariables($ziel)
        foreach ($k in $alle.Keys) {
            if ($k -like 'ESP32OS_WLAN*') { $gesetzt += "$k ($ziel)" }
        }
    }
    if ($gesetzt.Count -gt 0) {
        throw ('Release: WLAN-Umgebungsvariablen gesetzt, bitte entfernen: ' + ($gesetzt -join ', '))
    }
}
# Prüfung 3: Abbild enthält weder SSID noch Passwort aus wlan.local. Gibt nur
# das Ergebnis aus, nie die Werte.
function Test-ReleaseAbbild([string]$pfad) {
    $datei = Join-Path $repo 'wlan.local'
    if (-not (Test-Path $datei)) {
        Write-Host "Release-Pruefung $(Split-Path -Leaf $pfad): keine wlan.local, Byte-Suche entfaellt"
        return
    }
    $latin1 = [Text.Encoding]::GetEncoding(28591)
    $inhalt = $latin1.GetString([IO.File]::ReadAllBytes($pfad))
    foreach ($z in Get-Content -Encoding UTF8 $datei) {
        if ($z -match '^\s*(SSID|PASSWORT)\s*=\s*(.+?)\s*$') {
            $art = $Matches[1]
            $wert = $latin1.GetString([Text.Encoding]::UTF8.GetBytes($Matches[2]))
            $Matches = $null
            if ($inhalt.IndexOf($wert, [StringComparison]::Ordinal) -ge 0) {
                $wert = $null
                throw "Release: $art aus wlan.local steht im Abbild $(Split-Path -Leaf $pfad) - Abbild verworfen"
            }
            $wert = $null
        }
    }
    Write-Host "Release-Pruefung $(Split-Path -Leaf $pfad): keine WLAN-Zugangsdaten"
}
if (-not (Test-Path 'O:\esp32\firmware\Cargo.toml')) { subst O: $repo }

$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
    [Environment]::GetEnvironmentVariable('Path', 'User') + ';C:\Program Files\LLVM\bin'
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:CARGO_TARGET_DIR = 'C:\eo'
$env:CARGO_WORKSPACE_DIR = 'O:\esp32\firmware'
# Zwischenspeicher des Komponenten-Managers mit kurzem Pfad: esp_hosted
# enthält Beispiele mit sehr tiefen Ordnern (Windows-Grenze 260 Zeichen).
$env:IDF_COMPONENT_CACHE_PATH = 'C:\ec'
# Testnetz (W0): nur aus der lokalen Datei wlan.local, nie ausgeben
Remove-Item Env:ESP32OS_WLAN_SSID, Env:ESP32OS_WLAN_PASSWORT -ErrorAction SilentlyContinue
if ($Wlan) {
    $datei = Join-Path $repo 'wlan.local'
    if (-not (Test-Path $datei)) { throw 'wlan.local fehlt (Repo-Wurzel, Zeilen SSID=... und PASSWORT=...)' }
    foreach ($z in Get-Content -Encoding UTF8 $datei) {
        if ($z -match '^\s*SSID\s*=\s*(.+?)\s*$') { $env:ESP32OS_WLAN_SSID = $Matches[1] }
        if ($z -match '^\s*PASSWORT\s*=\s*(.+?)\s*$') { $env:ESP32OS_WLAN_PASSWORT = $Matches[1] }
    }
    $Matches = $null
    if (-not $env:ESP32OS_WLAN_SSID -or -not $env:ESP32OS_WLAN_PASSWORT) { throw 'wlan.local: SSID oder PASSWORT fehlt' }
}

# Programm → Cargo-Merkmal und Adresse der App-Partition.
$programme = @(
    @{ Name = 'esp32os';  Merkmal = '';      Adresse = '0x20000' },
    @{ Name = 'doomapp';  Merkmal = 'doom';  Adresse = '0xC00000' },
    @{ Name = 'quakeapp'; Merkmal = 'quake'; Adresse = '0xE00000' }
)
if ($Nur -eq 'factory') { $Nur = 'esp32os' }
if ($Bin -ne 'pudding-p4') { $programme = @($programme[0]) }
elseif ($Nur -ne 'alle') { $programme = @($programme | Where-Object { $_.Name -eq $Nur }) }

Push-Location O:\esp32\firmware
try {
    # Gekürzte lokale Kopien von Registry-Komponenten (esp_hosted, siehe
    # komponenten\esp32os\idf_component.yml): feste Version, SHA-256 geprüft.
    python werkzeuge\komponenten_holen.py
    if ($LASTEXITCODE -ne 0) { throw 'Komponenten holen fehlgeschlagen' }
    # esp-idf-sys bemerkt Änderungen am C-Code der Komponenten (Doom, Quake)
    # nicht: dann sdkconfig.defaults anfassen – ESP-IDF baut neu (nur Geändertes).
    $stempel = 'C:\eo\doom_c.stamp'
    # (auch die angepasste esp_hosted-Kopie unter C:\ec, siehe komponenten_holen.py)
    $neuste = Get-ChildItem O:\esp32\doom\doomgeneric, O:\esp32\quake\quakegeneric, O:\esp32\firmware\komponenten, O:\esp32\kolibri\kolibri_c, O:\esp32\kolibri\c, O:\esp32\kolibri\include, C:\ec\v3.1.0 -File -Recurse | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not (Test-Path $stempel) -or $neuste.LastWriteTime -gt (Get-Item $stempel).LastWriteTime) {
        (Get-Item O:\esp32\firmware\sdkconfig.defaults).LastWriteTime = Get-Date
        New-Item -ItemType File -Force $stempel | Out-Null
    }
    $elf = "C:\eo\riscv32imafc-esp-espidf\release\$Bin"
    foreach ($p in $programme) {
        if ($p.Merkmal) {
            cargo build --release --bin $Bin --features $p.Merkmal
        } else {
            cargo build --release --bin $Bin
        }
        if ($LASTEXITCODE -ne 0) { throw "Build $($p.Name) fehlgeschlagen" }
        Copy-Item $elf "C:\eo\$($p.Name).elf" -Force
        espflash save-image --chip esp32p4 --flash-size 32mb "C:\eo\$($p.Name).elf" "C:\eo\$($p.Name).bin"
        if ($LASTEXITCODE -ne 0) { throw "Abbild $($p.Name) fehlgeschlagen" }
        if ($Release) {
            try {
                Test-ReleaseAbbild "C:\eo\$($p.Name).elf"
                Test-ReleaseAbbild "C:\eo\$($p.Name).bin"
            } catch {
                Remove-Item "C:\eo\$($p.Name).elf", "C:\eo\$($p.Name).bin" -Force -ErrorAction SilentlyContinue
                throw
            }
        }
    }
    if ($NurBauen) { return }
    # Bootloader von ESP-IDF (mit Rollback, CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE)
    $boot = Get-ChildItem C:\eo\riscv32imafc-esp-espidf\release\build\esp-idf-sys -Filter bootloader.bin -Recurse | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    foreach ($p in $programme) {
        if ($p.Name -eq 'esp32os') {
            # Mit Bootloader und Partitionstabelle; otadata löschen → Start in esp32os_a.
            espflash flash --port $Port --baud 921600 --flash-size 32mb --bootloader $boot.FullName --partition-table partitions.csv --target-app-partition esp32os_a --erase-parts otadata --non-interactive @danach "C:\eo\esp32os.elf"
        } else {
            espflash write-bin --port $Port --baud 921600 --non-interactive @danach $p.Adresse "C:\eo\$($p.Name).bin"
        }
        if ($LASTEXITCODE -ne 0) { throw "Aufspielen $($p.Name) fehlgeschlagen (Board an `"USB TO UART`"? Sonst BOOT halten + RESET)" }
    }
    if ($Daten) {
        # Partition „daten“ (FAT mit Verschleißausgleich, siehe partitions.csv)
        espflash write-bin --port $Port --baud 921600 --non-interactive 0x1000000 $Daten
        if ($LASTEXITCODE -ne 0) { throw 'Daten-Abbild aufspielen fehlgeschlagen' }
    }
    if ($Monitor -gt 0) {
        $log = Join-Path $env:TEMP "esp32_$Bin.log"
        $p = Start-Process espflash -ArgumentList @('monitor', '--port', $Port, '--non-interactive') `
            -RedirectStandardOutput $log -PassThru -NoNewWindow
        if (-not $p.WaitForExit($Monitor * 1000)) { Stop-Process $p -Force }
        Get-Content $log
    }
} finally {
    Pop-Location
}
