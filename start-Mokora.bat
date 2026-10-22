@echo off
title Mokora Launcher

cd /d "%~dp0"

echo ==============================
echo       Starting Mokora...
echo ==============================

echo [0/2] Checking for Mokora still running...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\stop-running.ps1" -Root "%~dp0."
if errorlevel 1 (
    pause
    exit /b 1
)

echo [1/2] Starting frontend in background...
cd frontend
rem npm is a batch file, which `start` would run in a "cmd /K" that never
rem exits and keeps this window open; "cmd /c" ends with the dev server.
rem No input either: a Ctrl+Break would leave it waiting at "Terminate
rem batch job (Y/N)?".
start "" /B "%ComSpec%" /d /c "npm run dev" <nul >nul 2>&1
cd ..

echo Waiting for frontend...
timeout /t 2 /nobreak >nul

echo [2/2] Starting Mokora...
rem Mokora runs on a cmd command line rather than in this batch, so a Ctrl+C
rem never meets the "Terminate batch job (Y/N)?" prompt, which stops a batch
rem before it cleans up. `start /B` leaves that cmd ignoring Ctrl+C; Mokora
rem alone answers it, by closing every account (see src/platform.rs). Once
rem Mokora is gone -- with the last game browser, on Ctrl+C or with this
rem window -- the dev server is stopped and the window closes; a build error
rem or a crash waits for a key, to be read.
start "" /B "%ComSpec%" /d /s /c "cargo run & (if errorlevel 1 pause) & powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\stop-running.ps1" -Root "%~dp0." -DevServer"
exit /b 0
