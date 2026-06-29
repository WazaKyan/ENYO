@echo off
chcp 65001 >nul
cd /d "%~dp0"

set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

echo ============================================================
echo  ENYO - mode SPECTATEUR (la 1re fois : quelques minutes)
echo ============================================================
"%CARGO%" build --release -p ui
if errorlevel 1 (
  echo.
  echo Echec de compilation. Rust est-il installe ? ^(https://rustup.rs^)
  pause
  exit /b 1
)

echo.
echo ============================================================
echo  Spectateur : le monde joue tout seul, tu regardes.
echo    0 = pause     1 = vitesse normale     2 = vitesse rapide
echo    WASD/fleches  = se deplacer           molette = zoom
echo    clic          = inspecter une case    Echap = quitter
echo ============================================================
"%CARGO%" run --release -q -p ui -- --spectator --nations 10 --px 8