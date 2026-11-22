@echo off
rem Optionaler Build speziell fuer DIESE CPU (target-cpu=native) nach target\native.
rem Der normale Build (spielen.cmd) laeuft auf jeder x86-64-CPU und waehlt
rem AVX2-Pfade ohnehin zur Laufzeit; native bringt hier nur noch ~1-2 %.
setlocal
cd /d "%~dp0"
set RUSTFLAGS=-C target-cpu=native
"%USERPROFILE%\.cargo\bin\cargo.exe" build --release -p pudding --target-dir target\native || goto :error
echo Fertig: target\native\release\pudding.exe
exit /b 0

:error
echo Build fehlgeschlagen.
exit /b 1
