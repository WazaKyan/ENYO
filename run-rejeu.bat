@echo off
chcp 65001 >nul
cd /d "%~dp0"

set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

if not exist "out\derniere-partie.rec.jsonl" (
  echo Aucune partie enregistree. Lance d'abord run-jeu.bat et joue un peu.
  pause
  exit /b 1
)

echo ============================================================
echo  ENYO - compilation...
echo ============================================================
"%CARGO%" build --release -p ui
if errorlevel 1 ( pause & exit /b 1 )

echo.
echo ============================================================
echo  REJEU de ta derniere partie (deterministe, au bit pres) :
echo    Espace   = tour suivant       x1 / x2 = lecture auto
echo    Pause(0) = stop               WASD / molette = naviguer
echo    clic     = inspecter          Echap = retour menu
echo ============================================================
"%CARGO%" run --release -q -p ui -- --replay out\derniere-partie.rec.jsonl