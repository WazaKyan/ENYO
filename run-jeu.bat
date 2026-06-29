@echo off
chcp 65001 >nul
cd /d "%~dp0"

set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

echo ============================================================
echo  ENYO - compilation du jeu (la 1re fois : quelques minutes)
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
echo  Lancement. Commandes : WASD / fleches = se deplacer,
echo  molette = zoom, Echap = quitter.
echo ============================================================
"%CARGO%" run --release -q -p ui -- --player 0
