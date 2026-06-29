@echo off
chcp 65001 >nul
cd /d "%~dp0"

rem --- Trouve cargo (Rust) ---
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

if not exist out mkdir out

echo ============================================================
echo  ENYO - compilation (la premiere fois : quelques minutes)
echo ============================================================
"%CARGO%" build --release -p harness
if errorlevel 1 (
  echo.
  echo Echec de compilation. Rust est-il installe ? ^(https://rustup.rs^)
  pause
  exit /b 1
)

echo.
echo ============================================================
echo  Generation d'une partie : 8 nations, 150 mois, Directeur
echo ============================================================
"%CARGO%" run --release -q -p harness -- --seed 2026 --nations 8 --turns 150 --director --player 0 --png "out\monde.png" --png-scale 2 --region "out\civilisations.png" --region-scale 14 --tileset "out\tileset.png" --tileset-scale 16 --gif "out\partie.gif"

echo.
echo ============================================================
echo  Images generees dans le dossier "out\" :
echo    - monde.png          (la carte du monde)
echo    - civilisations.png  (zoom sur les nations : villes, frontieres, guerres)
echo    - partie.gif         (la partie qui evolue, en anime)
echo    - tileset.png        (les tuiles pixel-art)
echo  Ouverture...
echo ============================================================
start "" "out\partie.gif"
start "" "out\civilisations.png"
start "" "out\monde.png"
pause
