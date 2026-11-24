# ESP32-OS, C2 (W2): eigene C6-Firmware per Kabel auf den WLAN-Chip ESP32-C6
# schreiben – nur mit Sicherung. Das erste Mal geht es nur per USB-UART-Adapter
# direkt am C6 (die alte C6-Firmware 0.0.0 kann kein Update über SDIO);
# Anschluss und Download-Modus: STATUS.md, „Anleitung: C6-Firmware
# aktualisieren“.
#
#   1. Sichern:  c6_flashen.ps1 -Port COM7 -Sichern -Ziel C:\pfad\c6
#      (liest den ganzen Flash zweimal, vergleicht SHA-256; bricht sonst ab)
#   2. Schreiben: c6_flashen.ps1 -Port COM7 -Schreiben -Ziel C:\pfad\c6
#      (nur, wenn im Ziel eine geprüfte Sicherung liegt; Firmware aus
#      werkzeuge\c6_bauen.ps1, Bauordner C:\eo\c6)
#   3. Rückweg:  c6_flashen.ps1 -Port COM7 -Zurueck -Ziel C:\pfad\c6
param(
    [Parameter(Mandatory)][string]$Port,
    [Parameter(Mandatory)][string]$Ziel,
    [switch]$Sichern,
    [switch]$Schreiben,
    [switch]$Zurueck,
    [string]$Groesse = '0x400000'
)
$py = Join-Path $env:USERPROFILE '.espressif\python_env\idf5.5_py3.12_env\Scripts\python.exe'
$bau = 'C:\eo\c6'
New-Item -ItemType Directory -Force $Ziel | Out-Null
$sicherung = Join-Path $Ziel 'c6_sicherung.bin'

function Esptool { & $py -m esptool --chip esp32c6 -p $Port @args; if ($LASTEXITCODE -ne 0) { throw "esptool $($args -join " ") - Fehler $LASTEXITCODE" } }

if ($Sichern) {
    Esptool flash_id
    Esptool read_flash 0 $Groesse "$Ziel\c6_lesen1.bin"
    Esptool read_flash 0 $Groesse "$Ziel\c6_lesen2.bin"
    $a = (Get-FileHash "$Ziel\c6_lesen1.bin" -Algorithm SHA256).Hash
    $b = (Get-FileHash "$Ziel\c6_lesen2.bin" -Algorithm SHA256).Hash
    if ($a -ne $b) { "SICHERUNG UNGLEICH ($a / $b) – nichts geschrieben"; exit 1 }
    Move-Item -Force "$Ziel\c6_lesen1.bin" $sicherung
    Remove-Item "$Ziel\c6_lesen2.bin"
    "$a" | Set-Content "$sicherung.sha256"
    "SICHERUNG OK: $sicherung ($a)"
}
if ($Schreiben) {
    if (-not (Test-Path "$sicherung.sha256")) { "KEINE GEPRÜFTE SICHERUNG in $Ziel – zuerst -Sichern"; exit 1 }
    if ((Get-FileHash $sicherung -Algorithm SHA256).Hash -ne (Get-Content "$sicherung.sha256").Trim()) { "SICHERUNG BESCHÄDIGT"; exit 1 }
    Push-Location $bau
    try { Esptool -b 460800 --before default_reset --after hard_reset write_flash '@flash_args' } finally { Pop-Location }
    "GESCHRIEBEN – Board normal starten; im Log muss coprocessor=3.1.0 stehen"
}
if ($Zurueck) {
    if (-not (Test-Path $sicherung)) { "KEINE SICHERUNG in $Ziel"; exit 1 }
    Esptool -b 460800 write_flash 0 $sicherung
    "SICHERUNG ZURÜCKGESCHRIEBEN"
}
