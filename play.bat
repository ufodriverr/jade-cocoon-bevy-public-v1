@echo off
setlocal
cd /d "%~dp0"
if exist "%~dp0jc-game.exe" (
    "%~dp0jc-game.exe" %*
) else (
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    cargo run --locked --release -p jc-game -- %*
)
if errorlevel 1 pause
