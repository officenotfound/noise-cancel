QuietMic for macOS (early test build)
=====================================

What it does
  Removes background noise (rain, barking, horns, fans, distant voices) from your microphone for EVERY app, by
  sending the cleaned sound to a virtual microphone called BlackHole.

Set-up (once)
  1. Install the free BlackHole 2ch virtual audio driver:  https://existential.audio/blackhole/
     (or in Terminal: brew install blackhole-2ch). You may need to restart.
  2. Drag QuietMic to Applications and open it. The first time, macOS may say it is from an unidentified developer:
     right-click the app, choose Open, then Open again. Allow microphone access when asked.
  3. A coloured circle appears in the menu bar (green = working, grey = bypass, orange = starting, red = problem).
     Click it to open the window or use the menu.
  4. In Zoom / Teams / your softphone, choose "BlackHole 2ch" as the MICROPHONE.
  5. Use headphones so the cleaned voice you play back is not picked up again.

The window
  A Windows-95-style window: status lamp, level meters, presets, microphone drop-down, speed readouts and a small
  test recorder (record circle, play/pause, Cleaned / Original). Record a few seconds with noise playing, then press
  play to hear what other people would hear. A new recording replaces the old one.
  The close button hides the window; the app keeps running in the menu bar. Quit from the menu bar icon or Cmd+Q.

Troubleshooting
  - Menu bar icon > "Open log..." shows what went wrong.
  - If "Not running: BlackHole is not installed" appears, install BlackHole (step 1) and QuietMic will start itself.
  - No sound in the other app? Make sure that app's microphone is BlackHole 2ch, not your built-in microphone.
