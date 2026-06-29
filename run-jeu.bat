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
echo  Lancement. Commandes :
echo    WASD / fleches  = se deplacer        molette = zoom
echo    ESPACE          = fin de tour (le monde evolue)
echo    F               = outil Fonder   (puis clic sur une case)
echo    E               = outil Essaimer (2 clics : source puis cible)
echo    N               = aucun outil     clic = inspecter une case
echo    Echap           = quitter
echo  (les infos s'affichent dans la barre de titre de la fenetre)
echo ============================================================
"%CARGO%" run --release -q -p ui -- --player 0
