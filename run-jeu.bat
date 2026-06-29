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
echo  Lancement... un menu s'ouvre :
echo    Jouer        : developpe ta nation
echo    Spectateur   : regarde le monde tourner tout seul
echo    Parametres   : graine, nations, zoom, plein ecran
echo  Tout se fait a la souris ; les commandes clavier sont
echo  affichees dans le jeu. (Echap = retour menu / quitter)
echo ============================================================
"%CARGO%" run --release -q -p ui