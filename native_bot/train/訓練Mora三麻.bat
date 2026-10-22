@echo off
rem Saved in the Traditional Chinese code page (950), the console's own: cmd
rem misreads a UTF-8 batch file (chcp 65001) after a multi-line block and runs
rem half a line of Chinese as a command. Some cp950 characters end in a
rem byte cmd reads as | or ^ (U+6703, the full-width closing bracket),
rem so the echo lines avoid them.
setlocal
title 訓練 Mora 三麻
rem Mora (mora-3p): the sanma model on the V2 features.
rem Double-click to run; close the window to stop, double-click again to go on
rem from the last finished epoch. Everything stays in native_bot\out\.
rem MORA_QUICK=1 runs the same steps tiny (for checking this script).

cd /d "%~dp0.."
set "PY=%~dp0venv\Scripts\python.exe"
set "DATA=out\p3v2"
set "OUT=out\mora3p.safetensors"
set "GAMES="
set "EPOCHS=10"
set "SIZE=--conv 160 --blocks 10 --fc 768"
set "WALLS=2000"
if "%MORA_QUICK%"=="1" (
  set "DATA=out\p3v2-quick"
  set "OUT=out\mora3p-quick.safetensors"
  set "GAMES=300"
  set "EPOCHS=2"
  set "SIZE=--conv 32 --blocks 2 --fc 64"
  set "WALLS=10"
)
set "ARENA_LOG=%OUT:.safetensors=-arena.txt%"

echo ============================================================
echo  訓練 Mora 三麻
echo  訓練紀錄：%OUT%.log
echo  對打結果：%ARENA_LOG%
echo  中途關掉視窗沒關係，再雙擊一次就從上一輪接著訓練。
echo ============================================================
echo.

echo [1/4] 編譯抽取與對打程式
cargo build --release --features extract --bin extract --bin arena
if errorlevel 1 goto fail
echo.

echo [2/4] 產生第二版訓練資料，三麻全部約 6 分鐘
if exist "%DATA%\meta.json" (
  echo   已經產生過，略過。
) else (
  target\release\extract.exe dataset\p3 3 "%DATA%" %GAMES% --features v2
  if errorlevel 1 goto fail
)
echo.

echo [3/4] 訓練 %EPOCHS% 輪，每輪約 15 分鐘
if exist "%OUT%" (
  echo   已經訓練完成，略過。要重新訓練請先刪掉 %OUT% 和 %OUT%.ckpt
) else (
  "%PY%" train\train.py --data "%DATA%" --out "%OUT%" --epochs %EPOCHS% --batch 2048 --lr 1.5e-3 %SIZE% --stream-shards 16 --resume
  if errorlevel 1 goto fail
)
echo.

echo [4/4] 跟 shipped Mora 對打驗證：東風戰、半莊戰各 %WALLS% 副牌 x 6 種座位
if exist "%ARENA_LOG%" del "%ARENA_LOG%"
for %%M in (east south) do (
  powershell -NoProfile -Command "& '.\target\release\arena.exe' '%OUT%' 'weights\mora3p.safetensors' %WALLS% %%M | Tee-Object -FilePath '%ARENA_LOG%' -Append; exit $LASTEXITCODE"
  if errorlevel 1 goto fail
)
echo.
echo ============================================================
echo  全部完成！上面 A 是 Mora，B 是 shipped Mora；avg rank 越小越好。
echo  結果也存在 %ARENA_LOG%
echo ============================================================
pause
exit /b 0

:fail
echo.
echo  發生錯誤，訓練停止。請把上面的訊息給 Claude 看。
pause
exit /b 1
