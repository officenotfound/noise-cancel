@echo off
rem Times every model on this PC and writes measure_this_pc.txt next to this file. Send that file back.
cd /d "%~dp0"
echo CPU: %PROCESSOR_IDENTIFIER% > measure_this_pc.txt
echo. >> measure_this_pc.txt
qm.exe --models-dir models calibrate --preset max-noise-removal >> measure_this_pc.txt 2>&1
type measure_this_pc.txt
echo.
echo Saved to %~dp0measure_this_pc.txt
pause
