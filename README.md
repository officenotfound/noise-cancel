<div align="center">

```
  ___        _      _   __  __ _
 / _ \ _   _(_) ___| |_|  \/  (_) ___
| | | | | | | |/ _ \ __| |\/| | |/ __|
| |_| | |_| | |  __/ |_| |  | | | (__
 \__\_\\__,_|_|\___|\__|_|  |_|_|\___|

   *  A I   N O I S E   C A N C E L L E R  *
   free forever // open source // no keygen needed
```

</div>

```
 .-------------------------------------------------------------------.
 | QuietMic - Noise Canceller                                [_][X]  |
 |-------------------------------------------------------------------|
 | File  Options  Help                                               |
 | .-- Status ----------------------------------------------------. |
 | |  (*) ON                                                       | |
 | |  dfn3-ll | est. delay 41 ms | CPU 14%                         | |
 | |  Mic [########----------]   Out [###-------------]            | |
 | '---------------------------------------------------------------' |
 | .-- Preset ----------------------------------------------------. |
 | |  ( ) Low latency   (o) Balanced   ( ) Max noise removal       | |
 | '---------------------------------------------------------------' |
 '-------------------------------------------------------------------'
```

QuietMic cleans up your microphone **for every app at once**. It listens to your real mic, strips out the noise with
an on-device AI model, and sends the clean sound to a virtual microphone that Zoom, Teams, softphones and the rest can
use. Think of it as a free, open-source stand-in for Krisp.

Built for the noises that actually ruin calls: **rain on a steel roof, barking dogs, honking, a guy shouting outside**.
It runs on ordinary CPUs (an 8th-gen i5 is the target), with no GPU and no account.

```
 mic --> [ AI denoiser ] --> [ gentle gate ] --> [ limiter ] --> virtual cable --> Zoom / Teams / softphone
          DFN3-LL, FastEnhancer, DPDFNet, RNNoise              (VB-Cable / BlackHole)
```

## Status: v0.1, early

- Written on a Mac and checked there; the Windows build is cross-compiled and has had only light testing.
- No open model blocks every noise yet, so QuietMic ships several and picks the best one your PC can run in real time
  (see [BUILD_SPEC.md](BUILD_SPEC.md) section 13 for the measured numbers).
- The installers are not code-signed, so Windows SmartScreen and macOS Gatekeeper will warn you.
- Bug reports and benchmark results from other PCs are very welcome.

## Install

**Windows 10 / 11 (x64)**

1. Download `QuietMic-Setup.exe` from the [Releases](../../releases) page (check it against `SHA256SUMS.txt`).
2. Run it. It installs just for you (no administrator rights) and can fetch the free
   [VB-Cable](https://vb-audio.com/Cable/) virtual microphone driver for you; that part asks Windows for permission.
3. Start QuietMic and pick **CABLE Output** as the microphone in Zoom, Teams or your softphone.

```
 [ INSTALL ]   a scene-style installer with a chiptune (mute button included).
 uninstall:    Settings > Installed apps > QuietMic, or Uninstall.exe in the install folder (sadder music).
```

**macOS (Apple silicon)**: open `QuietMic-macOS.dmg`, drag the app to Applications, install
[BlackHole 2ch](https://existential.audio/blackhole/) and choose it as the microphone in your call app. See
[docs/MACOS_README.txt](docs/MACOS_README.txt).

## Presets

| Preset | What you get |
|---|---|
| Auto / Low latency | the fastest model that suits your PC, about 40 ms end to end |
| Balanced | better noise removal for a little more delay |
| Max noise removal | the strongest models plus an extra "noise guard" |
| Max voice separation | tuned to keep your voice and drop everything else |
| Light | for very old or busy PCs |

The first start measures your PC and picks the heaviest model it can run safely. If the PC falls behind later, QuietMic
steps down to a lighter model by itself and steps back up when it can.

## Privacy

```
 +--------------------------------------------------------------+
 |  no account   no telemetry   no logs   no network by default  |
 +--------------------------------------------------------------+
```

- Audio is processed on your machine and never leaves it.
- **No logs.** Nothing is written to disk about what you do. (Start with `--log` if you want a troubleshooting log.)
- **No network unless you ask.** Update checks are **off by default**. If you switch them on (installer tick-box or
  *Options > Check for updates online*), QuietMic asks GitHub for the newest release, downloads the installer, checks its
  SHA-256 against the release's `SHA256SUMS.txt`, and installs it on the next start. Switch it off and the downloads
  are deleted.
- The test recorder keeps one take on your disk until you record a new one.

## Build from source

You need [Rust](https://rustup.rs). The models are downloaded separately.

```sh
sh scripts/fetch_models.sh && sh scripts/fetch_models_fastenhancer.sh && sh scripts/fetch_models_dpdfnet.sh
cargo build --release -p qm-app          # the app
cargo run --release -p qm-app -- --demo  # the window with simulated data, no audio
sh scripts/package_macos.sh              # dist/QuietMic.app and .dmg (on a Mac)
```

Windows from a Mac: `cargo xwin build --release --target x86_64-pc-windows-msvc -p qm-app` (needs
[cargo-xwin](https://github.com/rust-cross/cargo-xwin); see [docs/DEV_NOTES.md](docs/DEV_NOTES.md)).

## Layout

```
crates/qm-dsp        denoiser wrappers: DeepFilterNet3 / DFN3-LL (tract), RNNoise, FastEnhancer, DPDFNet
crates/qm-engine     presets, gate, noise guard, limiter, governor (the "step down when slow" logic)
crates/qm-audio      WASAPI (Windows) and CoreAudio (macOS) capture/playback, drift-compensating resampler
crates/qm-app        the retro window, tray / menu bar, recorder, settings, updater
crates/qm-installer  the scene-style installer and uninstaller
crates/qm-cli        `qm`: process a file, list presets, measure this PC
crates/qm-bench      benchmark harness behind the numbers in results/
crates/qm-voice      research: speaker-gate experiments (not part of the app, see docs/RESEARCH.md)
```

## Credits and licences

QuietMic stands on other people's work. Please check each project's licence before redistributing the models.

- [DeepFilterNet](https://github.com/Rikorose/DeepFilterNet) (MIT / Apache-2.0) and [tract](https://github.com/sonos/tract) (MIT / Apache-2.0)
- [RNNoise](https://gitlab.xiph.org/xiph/rnnoise) / [nnnoiseless](https://github.com/jneem/nnnoiseless) (BSD-3-Clause)
- [FastEnhancer](https://github.com/aask1357/fastenhancer) and [DPDFNet](https://github.com/ceva-ip/DPDFNet): see their repositories for code and weight licences
- Pixel Operator font by Jayvee Enaguas (CC0), see `assets/fonts/README.md`
- [VB-Audio Virtual Cable](https://vb-audio.com/Cable/) is donationware by VB-Audio; QuietMic never bundles it, it only downloads it on request
- [BlackHole](https://existential.audio/blackhole/) on macOS
- [WeSpeaker](https://github.com/wenet-e2e/wespeaker) models (CC-BY-4.0) and Silero VAD (MIT) are used only by the `qm-voice` research crate

```
 greetz to the open source crew: DeepFilterNet, RNNoise, FastEnhancer, DPDFNet, Rust and egui.
 turn your speakers up, click install, and take your next call in peace.
```
