@echo off
rem Saved in the Traditional Chinese code page (950), the console's own: cmd
rem misreads a UTF-8 batch file (chcp 65001) after a multi-line block and runs
rem half a line of Chinese as a command. Some cp950 characters end in a
rem byte cmd reads as | or ^ (U+6703, the full-width closing bracket),
rem so the echo lines avoid them.
setlocal
title 訓練 Mora 危險度
rem Adds a danger head to Mora: for every tile, how likely each opponent would
rem win on it. The model itself is frozen, so its play stays exactly as it is;
rem step 4 checks that (the parity test) before putting the new file in
rem weights\, and keeps the old one in out\.
rem Double-click to run; close the window to stop, double-click again to go on
rem from the last finished epoch. MORA_QUICK=1 runs it tiny and installs nothing.

cd /d "%~dp0.."
set "PY=%~dp0venv\Scripts\python.exe"
set "DATA=out\p3v2d"
set "OUT=out\mora3p-danger.safetensors"
set "GAMES="
set "EPOCHS=3"
if "%MORA_QUICK%"=="1" (
  set "DATA=out\p3v2d-quick"
  set "OUT=out\mora3p-danger-quick.safetensors"
  set "GAMES=300"
  set "EPOCHS=1"
)

echo ============================================================
echo  訓練 Mora 危險度
echo  Mora 原本的出牌完全不變，只是多學「每張牌的放銃機率」。
echo  訓練紀錄：%OUT%.log
echo  中途關掉視窗沒關係，再雙擊一次就從上一輪接著訓練。
echo ============================================================
echo.

echo [1/4] 編譯抽取程式
cargo build --release --features extract --bin extract
if errorlevel 1 goto fail
echo.

echo [2/4] 產生訓練資料與危險度標註，三麻全部約 6 分鐘、約 55 GB
if exist "%DATA%\meta.json" (
  echo   已經產生過，略過。
) else (
  target\release\extract.exe dataset\p3 3 "%DATA%" %GAMES% --features v2 --danger
  if errorlevel 1 goto fail
)
echo.

echo [3/4] 訓練危險度 %EPOCHS% 輪
if exist "%OUT%" (
  echo   已經訓練完成，略過。要重新訓練請先刪掉 %OUT% 和 %OUT%.ckpt
) else (
  "%PY%" train\train_danger.py --data "%DATA%" --base weights\mora3p.safetensors --out "%OUT%" --epochs %EPOCHS% --resume
  if errorlevel 1 goto fail
)
echo.

if "%MORA_QUICK%"=="1" goto done
echo [4/4] 換上新的 Mora，並確認出牌和原本一模一樣
if not exist "out\mora3p-before-danger.safetensors" copy /y "weights\mora3p.safetensors" "out\mora3p-before-danger.safetensors" >nul
copy /y "%OUT%" "weights\mora3p.safetensors" >nul
cargo test --release --test parity
if errorlevel 1 (
  echo   出牌和原本不一樣，已經還原成原本的 Mora。
  copy /y "out\mora3p-before-danger.safetensors" "weights\mora3p.safetensors" >nul
  goto fail
)
echo.

:done
echo ============================================================
echo  全部完成！上面 val_auc 越接近 1 越準，mean_pred 越接近 real 越可信。
echo  重新雙擊 start-Mokora.bat 就能在對局畫面看到「危險度 - 模型」。
echo  原本的 Mora 備份在 out\mora3p-before-danger.safetensors
echo ============================================================
pause
exit /b 0

:fail
echo.
echo  發生錯誤，訓練停止。請把上面的訊息給 Claude 看。
pause
exit /b 1
