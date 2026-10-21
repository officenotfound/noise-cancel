@echo off
rem Records 6 seconds from the default microphone through the engine (no virtual cable, no sound output).
rem Results: %APPDATA%\QuietMic\selftest.txt plus selftest_input.wav / selftest_output.wav. Speak or play noise while it runs.
cd /d "%~dp0"
QuietMic.exe --selftest 6 --preset low-latency --model fe-s
timeout /t 9 >nul
type "%APPDATA%\QuietMic\selftest.txt"
pause
