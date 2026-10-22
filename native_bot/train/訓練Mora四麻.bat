@echo off
rem Pure ASCII on purpose: cmd misreads a UTF-8 batch file after a multi-line
rem block, and this file has no text that needs another code page.
setlocal
title Train Mora 4p
rem Mora 4p (mora-4p): the yonma model on the V2 features, the same recipe as
rem the sanma Mora (the .bat for Mora 3p) at the size Mokora 4p had (--conv 192
rem --blocks 12 --fc 768). The arena opponent is the original base 4p (Mokora
rem was removed).
rem Double-click to run; close the window to stop, double-click again to go on
rem from the last finished epoch. Everything stays in native_bot\out\.
rem MORA_QUICK=1 runs the same steps tiny (for checking this script).
rem The V2 set for 4 players is large (well over 100 GB): check the free space.
rem It also carries the danger labels (about 1 percent more), so the danger
rem script for Mora 4p reuses it instead of making a second copy.

cd /d "%~dp0.."
set "PY=%~dp0venv\Scripts\python.exe"
set "DATA=out\p4v2d"
set "OUT=out\mora4p.safetensors"
set "GAMES="
set "EPOCHS=5"
set "SIZE=--conv 192 --blocks 12 --fc 768"
rem 6 shards at a time: the 4-player V2 shards are 2.4 GB each and a group
rem of 8 ran out of memory on a 64 GB machine.
set "SHARDS=--stream-shards 6"
set "WALLS=1000"
if "%MORA_QUICK%"=="1" (
  set "DATA=out\p4v2d-quick"
  set "OUT=out\mora4p-quick.safetensors"
  set "GAMES=300"
  set "EPOCHS=2"
  set "SIZE=--conv 32 --blocks 2 --fc 64"
  set "SHARDS="
  set "WALLS=10"
)
set "ARENA_LOG=%OUT:.safetensors=-arena.txt%"

echo ============================================================
echo  Train Mora 4p  (yonma, V2 features)
echo  Training log : %OUT%.log
echo  Arena result : %ARENA_LOG%
echo  Close this window to stop. Run it again to continue.
echo ============================================================
echo.

echo [1/4] Building the extract and arena tools
cargo build --release --features extract --bin extract --bin arena
if errorlevel 1 goto fail
echo.

echo [2/4] Making the V2 training set from the 4-player logs (long)
if exist "%DATA%\meta.json" (
  echo   Already made, skipped.
) else (
  target\release\extract.exe dataset\p4 4 "%DATA%" %GAMES% --features v2 --danger
  if errorlevel 1 goto fail
)
echo.

echo [3/4] Training %EPOCHS% epochs (the progress bar shows time left)
if exist "%OUT%" (
  echo   Already trained, skipped. To train again, delete %OUT% and %OUT%.ckpt
) else (
  "%PY%" train\train.py --data "%DATA%" --out "%OUT%" --epochs %EPOCHS% --batch 2048 --lr 1.5e-3 %SIZE% %SHARDS% --resume
  if errorlevel 1 goto fail
)
echo.

echo [4/4] Arena against the original base 4p: east and south, %WALLS% walls each
if exist "%ARENA_LOG%" del "%ARENA_LOG%"
for %%M in (east south) do (
  powershell -NoProfile -Command "& '.\target\release\arena.exe' '%OUT%' 'weightsase4p.safetensors' %WALLS% %%M | Tee-Object -FilePath '%ARENA_LOG%' -Append; exit $LASTEXITCODE"
  if errorlevel 1 goto fail
)
echo.
echo ============================================================
echo  Done. A is Mora 4p, B is the original base 4p; a lower average rank is better.
echo  Result saved in %ARENA_LOG%
echo ============================================================
pause
exit /b 0

:fail
echo.
echo  Something went wrong and training stopped. Copy the messages above to Claude.
pause
exit /b 1
