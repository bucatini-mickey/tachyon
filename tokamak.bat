@echo off
rem The Tokamak, from Explorer or a shell:  tokamak.bat [project]
rem With no project it opens the last one (Ctrl+P switches inside).
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
rem Double-clicked: reopen in Windows Terminal, which draws it best.
if not defined WT_SESSION (
    where wt >nul 2>nul && (
        start "" wt -d "%~dp0." cmd /c "%~f0" %*
        exit /b
    )
)
title The Tokamak
cargo run --release --quiet --example tokamak -- %*
if errorlevel 1 pause
