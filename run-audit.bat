@echo off
chcp 65001 >nul
cd /d "%~dp0"

set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

echo ============================================================
echo  ENYO - AUDIT : pilote le vrai jeu via une sequence scriptee
echo  (menu, parametres, partie, spectateur) et sauve un PNG par
echo  etape. Aucune fenetre ne s'ouvre.
echo ============================================================
"%CARGO%" run --release -q -p ui -- --audit --out out\audit
if errorlevel 1 ( pause & exit /b 1 )

echo.
echo ============================================================
echo  Meme audit en resolution PLEIN ECRAN (verifie la mise en page)
echo ============================================================
"%CARGO%" run --release -q -p ui -- --audit --fullscreen --out out\audit-plein-ecran

echo.
echo Captures dans : out\audit\  et  out\audit-plein-ecran\
echo (change la partie auditee avec --seed N)
pause