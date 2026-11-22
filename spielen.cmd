@echo off
rem Startet Pudding-Panik. Baut das Spiel bei Bedarf (Rust liegt in %%USERPROFILE%%\.cargo\bin).
setlocal
cd /d "%~dp0"
if not exist "target\release\pudding.exe" (
    echo Pudding-Panik wird gebaut ...
    "%USERPROFILE%\.cargo\bin\cargo.exe" build --release -p pudding || goto :error
)
start "" "target\release\pudding.exe" %*
exit /b 0

:error
echo Build fehlgeschlagen. Ist Rust installiert? (https://rustup.rs)
pause
exit /b 1
