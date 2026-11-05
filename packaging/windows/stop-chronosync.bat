@echo off
taskkill /F /IM chronosync.exe >nul 2>&1
if errorlevel 1 (echo ChronoSync was not running. This does not kill unrelated programs.) else (echo ChronoSync stopped.)
echo If a non-ChronoSync PID owns the port, use: taskkill /F /PID ^<PID^>
echo Run this file as Administrator if the old instance was elevated.
pause
