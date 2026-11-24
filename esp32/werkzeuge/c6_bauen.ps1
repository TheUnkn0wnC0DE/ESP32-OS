# ESP32-OS, C2 (W2): C6-Firmware (esp_hosted 3.1.0 Koprozessor, SDIO) bauen.
# ESP-IDF 5.5.5 aus dem Benutzerordner (~\.espressif), Ziel esp32c6,
# Bauordner C:\eo\c6 (kurzer Pfad, ausserhalb des Repos). Die Umgebung wird
# von Hand gesetzt (export.ps1 verlangt alle Werkzeuge, installiert sind nur
# die für RISC-V – die braucht der C6 auch).
# Ergebnis: C:\eo\c6\esp32os_c6.bin (+ bootloader, Partitionstabelle) und
# die genaue esptool-Zeile in C:\eo\c6\flasher_args.json.
#
#   powershell -File esp32\werkzeuge\c6_bauen.ps1
$ErrorActionPreference = 'Continue'
$projekt = Resolve-Path "$PSScriptRoot\..\c6_firmware"
$esp = Join-Path $env:USERPROFILE '.espressif'
$env:IDF_PATH = Join-Path $esp 'esp-idf\v5.5.5'
$env:IDF_TOOLS_PATH = $esp
$py = Join-Path $esp 'python_env\idf5.5_py3.12_env\Scripts'
$env:IDF_PYTHON_ENV_PATH = Join-Path $esp 'python_env\idf5.5_py3.12_env'
$werkzeuge = @(
    $py,
    (Join-Path $esp 'tools\cmake\3.30.2\bin'),
    (Join-Path $esp 'tools\ninja\1.12.1'),
    (Join-Path $esp 'tools\riscv32-esp-elf\esp-14.2.0_20260121\riscv32-esp-elf\bin'),
    (Join-Path $env:IDF_PATH 'tools')
)
$env:PATH = ($werkzeuge -join ';') + ';' + $env:PATH
$env:ESP_ROM_ELF_DIR = Join-Path $esp 'tools\esp-rom-elfs\20241011'
$idfpy = Join-Path $env:IDF_PATH 'tools\idf.py'
$bau = 'C:\eo\c6'
New-Item -ItemType Directory -Force $bau | Out-Null
if (-not (Test-Path "$bau\CMakeCache.txt")) {
    & "$py\python.exe" $idfpy -C $projekt -B $bau set-target esp32c6
    if ($LASTEXITCODE -ne 0) { "SET-TARGET FEHLER $LASTEXITCODE"; exit 1 }
}
& "$py\python.exe" $idfpy -C $projekt -B $bau build
if ($LASTEXITCODE -ne 0) { "BAU FEHLER $LASTEXITCODE"; exit 1 }
Get-ChildItem $bau -Filter *.bin | Select-Object Name, Length
"FERTIG"
