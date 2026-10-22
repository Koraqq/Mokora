@echo off
rem Pure ASCII on purpose, like the Mora 4p training script.
setlocal
title Train Mora 4p danger
rem Adds a danger head to Mora 4p: for every tile, how likely each opponent
rem would win on it. The model itself is frozen, so its play stays exactly as
rem it is. Run the Mora 4p training script first: this one takes its model
rem (out\mora4p.safetensors) and its training set (out\p4v2d), and makes the
rem set itself if it is missing. The result goes to out\ only: Mora 4p is not
rem installed into weights\ yet, so nothing there is touched.
rem Double-click to run; close the window to stop, double-click again to go on
rem from the last finished epoch. MORA_QUICK=1 runs it tiny.

cd /d "%~dp0.."
set "PY=%~dp0venv\Scripts\python.exe"
set "DATA=out\p4v2d"
set "BASE=out\mora4p.safetensors"
set "OUT=out\mora4p-danger.safetensors"
set "GAMES="
set "EPOCHS=3"
rem 6 shards at a time, like the training script (8 ran out of memory there).
set "SHARDS=--stream-shards 6"
if "%MORA_QUICK%"=="1" (
  set "DATA=out\p4v2d-quick"
  set "BASE=out\mora4p-quick.safetensors"
  set "OUT=out\mora4p-danger-quick.safetensors"
  set "GAMES=300"
  set "EPOCHS=1"
  set "SHARDS="
)

echo ============================================================
echo  Train Mora 4p danger
echo  Mora 4p plays exactly as before; it only learns each tile's
echo  deal-in chance (chi and the 2m to 8m tiles included).
echo  Training log : %OUT%.log
echo  Close this window to stop. Run it again to continue.
echo ============================================================
echo.

if not exist "%BASE%" (
  echo  %BASE% is missing. Run the Mora 4p training script first.
  goto fail
)

echo [1/3] Building the extract tool
cargo build --release --features extract --bin extract
if errorlevel 1 goto fail
echo.

echo [2/3] Making the training set with danger labels (long, skipped if made)
if exist "%DATA%\meta.json" (
  echo   Already made, skipped.
) else (
  target\release\extract.exe dataset\p4 4 "%DATA%" %GAMES% --features v2 --danger
  if errorlevel 1 goto fail
)
echo.

echo [3/3] Training the danger head, %EPOCHS% epochs (the progress bar shows time left)
if exist "%OUT%" (
  echo   Already trained, skipped. To train again, delete %OUT% and %OUT%.ckpt
) else (
  "%PY%" train\train_danger.py --data "%DATA%" --base "%BASE%" --out "%OUT%" --epochs %EPOCHS% %SHARDS% --resume
  if errorlevel 1 goto fail
)
echo.
echo ============================================================
echo  Done. The closer val_auc is to 1 and mean_pred is to real, the better.
echo  The model with the danger head is %OUT%
echo  Mora 4p is not part of the game yet; ask Claude to install it.
echo ============================================================
pause
exit /b 0

:fail
echo.
echo  Something went wrong and training stopped. Copy the messages above to Claude.
pause
exit /b 1
