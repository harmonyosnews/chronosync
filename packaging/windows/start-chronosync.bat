@echo off
cd /d "%~dp0"
echo ChronoSync v1.5.0
echo Auto-port is enabled; 8686 will be skipped if it is busy.
echo Keep this window open while ChronoSync is running.
echo Close this window or press Ctrl+C to stop this instance.
echo.
chronosync.exe --open --auto-port %*
echo.
echo ChronoSync process exited.
echo If this was a second launch, the existing instance is still running.
echo Run stop-chronosync.bat to stop all ChronoSync processes.
pause
