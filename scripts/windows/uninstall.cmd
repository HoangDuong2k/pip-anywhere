@echo off
rem Double-click to run uninstall.ps1 (no administrator rights needed).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1" %*
pause
