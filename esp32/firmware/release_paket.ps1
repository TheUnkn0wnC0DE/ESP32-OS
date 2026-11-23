# Release-Paket (O8) lokal bauen – veröffentlicht nichts.
#
#   powershell -File esp32\firmware\release_paket.ps1 -Version 1.0
#
# 1. Baut ESP32-OS, Doom und Quake mit `bauen_flashen.ps1 -Release -NurBauen`
#    (lehnt WLAN-Zugangsdaten, Entwicklungs-Schalter und Spieldaten ab und
#    prüft die fertigen Abbilder darauf, siehe dort).
# 2. Legt unter `release\v<Version>\` (nicht im Repo, .gitignore) ab:
#    - esp32os.bin, doomapp.bin, quakeapp.bin (App-Abbilder),
#    - bootloader.bin (ESP-IDF, mit Rollback), partition-table.bin,
#      otadata-leer.bin (8 KB 0xFF: Start aus Platz A nach dem Aufspielen),
#    - esp32os.bin.sha256 (für Updates über WLAN, U1) und SHA256SUMS,
#    - manifest.json für den Web-Flasher (docs/flasher/, Offsets wie
#      partitions.csv).
# 3. Prüft, dass die eingebaute Version (desktop/src/shell.rs, VERSION) mit
#    -Version beginnt – sonst meldete das Update über WLAN eine falsche
#    Version.
param(
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+(\.\d+)?$')][string]$Version
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

$shell = Get-Content -Raw -Encoding UTF8 (Join-Path $repo 'esp32\desktop\src\shell.rs')
if ($shell -notmatch 'pub const VERSION: &str = "([^"]+)"') { throw 'VERSION in shell.rs nicht gefunden' }
$eingebaut = $Matches[1]
if ($eingebaut -ne $Version -and -not $eingebaut.StartsWith("$Version ")) {
    throw "Eingebaute Version '$eingebaut' passt nicht zu -Version $Version (shell.rs VERSION vor dem Release setzen)"
}

& powershell -NoProfile -File (Join-Path $PSScriptRoot 'bauen_flashen.ps1') -Release -NurBauen
if ($LASTEXITCODE -ne 0) { throw 'Release-Bau fehlgeschlagen' }

$ziel = Join-Path $repo "release\v$Version"
if (Test-Path $ziel) { Remove-Item -Recurse -Force $ziel }
New-Item -ItemType Directory -Force $ziel | Out-Null
foreach ($n in 'esp32os', 'doomapp', 'quakeapp') {
    Copy-Item "C:\eo\$n.bin" (Join-Path $ziel "$n.bin") -Force
}
$boot = Get-ChildItem C:\eo\riscv32imafc-esp-espidf\release\build\esp-idf-sys -Filter bootloader.bin -Recurse |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $boot) { throw 'bootloader.bin nicht gefunden' }
Copy-Item $boot.FullName (Join-Path $ziel 'bootloader.bin') -Force
espflash partition-table (Join-Path $PSScriptRoot 'partitions.csv') --to-binary --output (Join-Path $ziel 'partition-table.bin')
if ($LASTEXITCODE -ne 0) { throw 'Partitionstabelle fehlgeschlagen' }
[IO.File]::WriteAllBytes((Join-Path $ziel 'otadata-leer.bin'), [byte[]](,0xFF * 8192))

# Prüfsummen
$summen = foreach ($d in Get-ChildItem $ziel -File | Sort-Object Name) {
    '{0}  {1}' -f (Get-FileHash $d.FullName -Algorithm SHA256).Hash.ToLower(), $d.Name
}
[IO.File]::WriteAllText((Join-Path $ziel 'SHA256SUMS'), (($summen -join "`n") + "`n"))
$app = ($summen | Where-Object { $_ -like '*  esp32os.bin' })
[IO.File]::WriteAllText((Join-Path $ziel 'esp32os.bin.sha256'), "$app`n")

# Manifest für ESP Web Tools (docs/flasher/index.html): Offsets wie partitions.csv,
# Bootloader des ESP32-P4 bei 0x2000, Partitionstabelle bei 0x8000
$teile = @(
    @{ path = 'bootloader.bin'; offset = 0x2000 },
    @{ path = 'partition-table.bin'; offset = 0x8000 },
    @{ path = 'otadata-leer.bin'; offset = 0x10000 },
    @{ path = 'esp32os.bin'; offset = 0x20000 },
    @{ path = 'doomapp.bin'; offset = 0xC00000 },
    @{ path = 'quakeapp.bin'; offset = 0xE00000 }
)
$manifest = [ordered]@{
    name = 'ESP32-OS'
    version = $Version
    new_install_prompt_erase = $true
    builds = @([ordered]@{ chipFamily = 'ESP32-P4'; parts = $teile })
}
[IO.File]::WriteAllText((Join-Path $ziel 'manifest.json'), ($manifest | ConvertTo-Json -Depth 5))
Get-ChildItem $ziel | Format-Table Name, Length -AutoSize | Out-String | Write-Host
Write-Host "Release-Paket fertig: $ziel (nichts veröffentlicht)"
