QuietMic (early test build)
===========================

What it does
  Removes background noise (rain, barking, horns, fans, distant voices) from your microphone for EVERY app, by
  sending the cleaned sound to a virtual microphone.

Set-up (once)
  1. Install the free VB-Audio Virtual Cable: https://vb-audio.com/Cable/  (run the setup as administrator, reboot).
  2. Double-click QuietMic.exe. A coloured circle appears near the clock (click the ^ arrow if hidden).
       green = working, grey = bypass, orange = starting/measuring, red = problem (hover for the reason)
  3. In Zoom / Teams / your softphone, choose "CABLE Output (VB-Audio Virtual Cable)" as the MICROPHONE.
  4. Use a headset. Keep the softphone's own echo cancellation ON and its noise suppression LOW or OFF.

First start
  The first launch measures how fast this PC is (a few seconds), picks the strongest model it can run safely, and
  remembers it. Right-click the tray icon for presets, microphone choice, "Re-measure this PC" and
  "Performance report...".

Presets (right-click the tray icon > Preset)
  Auto / Low latency   lowest delay first, then noise removal (recommended for calls)
  Balanced             stronger removal, a little more delay
  Max noise removal    strongest (uses the DPDFNet model, adds about 50 ms of delay)
  Max voice separation tighter on other / distant voices
  Light                for old or slow PCs

Please test and report
  - Run measure_this_pc.bat and send back measure_this_pc.txt (speed numbers for this CPU).
  - Run selftest.bat to check the microphone path (no virtual cable needed). It writes selftest.txt and two WAV
    files in %APPDATA%\QuietMic.
  - Tell us: does a barking dog / rain still get through? Is your own voice clipped or robotic? Any crackling?
  - Log file: tray icon > "Open log..." (or %APPDATA%\QuietMic\quietmic.log).

Known limits of this build
  - Switching presets restarts the audio for a moment.
  - Added delay is the estimate shown in the tray (model + about 30 ms of Windows audio buffering).
  - No installer yet; this folder can be anywhere.
