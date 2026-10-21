# Build Spec: System-Wide AI Noise Cancellation for Windows

> **Audience:** an AI coding agent (or developer) picking this up cold. Everything you need to start is here.
> **Owner's goal:** replace a paid Krisp subscription with a free, self-built app that removes background noise from the microphone for **every** Windows app, with a strong focus on **softphones**.
> **Working name:** `QuietMic` (rename freely).

---

## 1. Requirements

### Hard requirements
| # | Requirement | Target |
|---|---|---|
| R1 | Works in **all** Windows apps (softphones, Zoom, Meet, Teams, Discord, browsers) | Via a virtual microphone (VB-Audio Virtual Cable, see §3) |
| R2 | **Very low latency**. Primary use is live phone calls on softphones | **≤ 30 ms** added end-to-end (mic in → virtual mic out), hard ceiling 40 ms |
| R3 | Runs on **low-end PCs** | Minimum spec: **Intel 8th-gen i5 (e.g. i5-8250U, 4C/8T, 15 W laptop), 16 GB RAM, Windows 11, no dedicated GPU** |
| R4 | No GPU dependency | CPU-only inference. No CUDA, no RTX Voice / NVIDIA Broadcast |
| R5 | No audio glitches (dropouts, crackles) under normal desktop load | 0 buffer underruns in a 60-minute call on the minimum spec |
| R6 | Free and open-source dependencies only | Licenses must allow free personal use and redistribution (MIT / Apache-2.0 / BSD) |
| R7 | All processing local | No audio or voice data leaves the PC |

### Noise profile (what the owner actually faces, ordered by frequency)
1. **Loud rain on a steel roof**: dense, broadband, sustained, impulsive texture.
2. **Dogs barking**: sudden, loud, harmonic, short bursts.
3. **Loud car horns / honking**: sudden, very loud, strongly **tonal/harmonic**. Risk: models can mistake horn harmonics for voiced speech.
4. **A man yelling outside in another language**: **distant (far-field) human speech**, reverberant, coming through walls/windows. Generic denoisers deliberately *keep* speech, so this needs extra handling (see §5.3).
5. **Nearby people talking**: *rare*. Lower priority.

Tests, tuning and acceptance criteria must be built around these five categories, **especially #2–#4**, which are sudden and loud.

---

## 2. Architecture

```
┌───────────── QuietMic.exe (single Rust binary, system tray) ─────────────┐
│                                                                          │
│  [WASAPI capture]──►[lock-free ring]──►[DSP thread]──►[lock-free ring]──►[WASAPI render]
│   real mic, 48 kHz     (rtrb)            │                                  to "CABLE Input"
│   event-driven,                          │                                  (VB-Cable)
│   10 ms periods                          ▼
│                           1. Resample to 48 kHz mono f32 (if needed)
│                           2. DeepFilterNet3 low-latency (or RNNoise fallback)
│                           3. Distant-voice / residual gate  (§5.3)
│                           4. (Phase 2) Voiceprint gate        (§6)
│                           5. Output limiter (-1 dBFS ceiling)
│                                                                          │
│  [UI thread: tray icon + settings window]  ◄── atomics / channel ──►  DSP │
└──────────────────────────────────────────────────────────────────────────┘

Softphone / Zoom / etc. select microphone:  "CABLE Output (VB-Audio Virtual Cable)"
```

### Threads
- **Capture thread**: WASAPI shared mode, event-driven, smallest period the device allows (target 10 ms; use `IAudioClient3` for lower periods where supported). Registered with MMCSS as **"Pro Audio"** (`AvSetMmThreadCharacteristicsW`).
- **DSP thread**: MMCSS "Pro Audio". Pulls 10 ms (480-sample) frames, processes them, and pushes them out. **No allocations, locks, logging, or syscalls in the hot path.** Preallocate everything.
- **Render thread**: WASAPI shared mode, event-driven, writes to the VB-Cable "CABLE Input" render endpoint. MMCSS "Pro Audio".
- **UI thread**: tray, settings window, device notifications. Talks to DSP only via atomics (params) and a lock-free SPSC channel (meters/stats).

You may merge capture+DSP+render into one thread driven by the capture event if measurements show lower latency/jitter. Measure both.

### Latency budget (must be measured and shown in the UI)
| Stage | Budget |
|---|---|
| Capture period | 10 ms (lower if IAudioClient3 allows) |
| Model algorithmic delay | ≤ 10–20 ms (DFN3 low-latency, see §5.1) |
| Ring buffer slack | ≤ 1 frame, adaptive |
| Render period + VB-Cable | ~7–10 ms (VB-Cable internal buffer set to minimum stable value) |
| **Total added** | **≤ 30 ms target** |

### Clock drift
The real mic and VB-Cable run on different clocks. Implement **adaptive drift compensation**: monitor the output ring fill level and apply tiny resampling-ratio corrections (±0.01–0.1%) with a high-quality resampler (`rubato`) so the buffer never grows (latency creep) or empties (glitches). Never correct by dropping or duplicating samples.

---

## 3. Virtual microphone: VB-Audio Virtual Cable

- The owner has approved installing **VB-Audio Virtual Cable** (free donationware): https://vb-audio.com/Cable/
- Do **not** write a custom kernel audio driver. Driver signing costs too much and isn't worth it for this project.
- On startup, detect whether "CABLE Input" exists. If missing, show a friendly dialog with a link to the download page and short install steps (install as admin, reboot).
- Document the latency tuning: in the VB-Cable control panel, lower the internal buffer ("Max Latency") as far as is stable. Note the stable value in the README after testing on the minimum spec.
- **Do not** redistribute VB-Cable inside the installer (licence). Link to it.

---

## 4. Tech stack

| Concern | Choice | Notes |
|---|---|---|
| Language | **Rust** (stable) | DeepFilterNet's runtime is Rust-native; single static `.exe` |
| Audio I/O | `wasapi` crate (or `windows` crate directly) | Need event-driven mode, IAudioClient3, MMCSS, device notifications. `cpal` is acceptable for a first prototype but likely too abstracted for the latency target |
| Primary denoiser | **DeepFilterNet 3**: https://github.com/Rikorose/DeepFilterNet (`libDF`, crate `deep_filter`, runs ONNX via `tract`) | MIT/Apache-2.0. Use the **low-latency model variant** (check `models/` in the repo for the `_ll` ONNX bundle) |
| Fallback denoiser | **RNNoise** via `nnnoiseless` (pure Rust port) | BSD. ~10 ms delay, ~1% CPU. "Ultra-light" mode |
| Resampling | `rubato` | For non-48 kHz mics and drift compensation |
| Ring buffers | `rtrb` | Lock-free SPSC |
| VAD | Silero VAD (ONNX, MIT) via `tract` or `ort` | Used by the gate (§5.3) and voiceprint (§6) |
| Speaker embedding (Phase 2) | WeSpeaker ResNet34 or SpeechBrain ECAPA-TDNN exported to ONNX | Apache-2.0. Pick the one that runs fastest on CPU |
| Tray icon / menu | `tray-icon` + `muda` | |
| Settings window | `egui` / `eframe` | Lightweight, no web runtime |
| Global hotkey | `global-hotkey` | Mute toggle |
| Autostart | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` | Opt-in checkbox |
| Config | TOML in `%APPDATA%\QuietMic\config.toml` via `serde` + `toml` | |
| Installer | Inno Setup (or `cargo-wix`) | Phase 3 |
| Build | Cross-compile from macOS with `cargo-xwin` (target `x86_64-pc-windows-msvc`) **and** GitHub Actions `windows-latest` CI | The dev machine is a Mac. Final testing happens on the owner's Windows PC |

**Build flags:** release profile with `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`. Keep the default x86-64 baseline target. Don't set `target-cpu=native` or x86-64-v3 globally. 8th-gen i5 has AVX2, but the `.exe` should also run on older or odd CPUs. Where SIMD matters, use runtime CPU feature detection.

---

## 5. Denoising pipeline details

### 5.1 DeepFilterNet3 low-latency (default "Quality" mode)
- 48 kHz mono, 10 ms hop (480 samples).
- Use the **low-latency / no-lookahead** model or config. Standard DFN3 has ~40 ms algorithmic delay, which **breaks R2**. Verify the real algorithmic delay of the chosen model and record it in the README.
- Expose DFN's **attenuation limit** (`atten_lim_db`) as the **"Strength"** slider: e.g. 6 dB (light) … 100 dB (max). A lower limit keeps a little natural room tone and reduces "underwater" artifacts.
- Run inference **single-threaded** (tract) to avoid thread-pool wakeup jitter.
- If any frame's processing time exceeds 70% of the hop (7 ms) for N consecutive frames, auto-switch to RNNoise and show a tray warning. Switch back when the system recovers.

### 5.2 RNNoise ("Ultra-light" mode)
- `nnnoiseless`, 480-sample frames at 48 kHz.
- For very weak machines or battery saver.

### 5.3 Handling the owner's specific noises
Generic models get most of the way. Add these targeted measures and **evaluate each one** with the test set (§8):

1. **Input headroom (all categories, critical for barks/horns):** clipped noise is much harder to remove. On first run, check the mic level. If the peaks clip, tell the user to lower the mic level and **turn off Windows "Microphone Boost" and any OEM "voice enhancement"/AGC** (Realtek, Dolby, Waves MaxxVoice are common on laptops). Show a clip indicator in the UI.
2. **Barks and horns (sudden, loud):** a no-lookahead model can let the first 10–20 ms of an onset through. Mitigation: a **transient guard** after the model. When the input energy spikes by more than X dB within one frame **and** VAD says "not user speech", apply fast gain reduction (attack ≤ 1 ms, release ~50–100 ms). Tune X so it never chops the user's own plosives.
3. **Horn harmonics mistaken for speech:** test specifically. If DFN keeps horn tones, the transient guard plus the residual gate (below) should catch them. Log results.
4. **Distant yelling (far-field speech): "Distant voice reduction"**, a toggle that is **on by default**:
   - The user wears a headset or sits close to the mic, so their voice is much louder and drier than speech from outside.
   - After denoising, run Silero VAD and a **level-based downward expander**. Speech below a calibrated level threshold (set during first-run calibration from the user's normal speaking level) is attenuated 15–30 dB.
   - This costs almost nothing and requires no voiceprint. It is the **main defence** against the guy yelling outside, and it works even before Phase 2.
5. **Rain on steel roof:** DFN handles dense broadband noise well. Mainly verify there's no "musical noise" or pumping during long rain. The strength slider controls the trade-off.

### 5.4 Output stage
- Soft limiter at −1 dBFS (protects softphone codecs from clipping).
- Optional make-up gain (0 to +12 dB) for quiet mics.
- Optional "Monitor" toggle: play processed audio to the user's headphones so they can hear themselves. Off by default. Uses a separate render stream and never adds latency to the main path.

---

## 6. Phase 2: In-app voiceprint ("Only my voice")

Lower priority than Phase 1, since nearby talkers are rare. The distant-voice gate (§5.3.4) covers most of the yelling case.

### Enrollment (recorded **inside the app**)
- Settings → "Set up my voice". A wizard shows a short script (~30 s of varied sentences) for the user to read aloud.
- Live level meter. Reject and ask for a redo if the input clips, is too quiet, or the background noise is too high (measure SNR with VAD on/off segments).
- Run the denoiser **before** computing the embedding. Average the embeddings over several VAD-voiced windows.
- Store the embedding locally in `%APPDATA%\QuietMic\voiceprints\<device-id>.bin`. Support one profile per input device. Add a "Delete my voiceprint" button.
- A "Test it" button records 10 s while someone else talks, then plays back before and after.

### Runtime gate
- Every ~200–400 ms of VAD-voiced audio, compute an embedding on a sliding window (run on a **low-priority worker thread**, never on the DSP thread).
- If the cosine similarity to the voiceprint is below the threshold, attenuate voiced segments (smoothed, ~100 ms ramps). The DSP thread only reads the resulting gain value via an atomic. **This must add zero latency** to the audio path; the decision lags slightly, which is acceptable.
- Known limitation, to be stated in the UI: it can't separate the user's voice from another voice **at the same instant**. True overlapping-speech isolation (target speaker extraction) is out of scope unless the owner asks for it later.

---

## 7. UI / UX

**Tray icon:** green = active, grey = bypass, red = error (e.g. VB-Cable missing, mic disconnected).

**Tray menu:**
- Enabled / Bypass toggle
- Mode: Quality (DFN3-LL) · Ultra-light (RNNoise)
- Strength: Light / Medium / Strong / Max
- Distant voice reduction ✓
- Only my voice ✓ (Phase 2, greyed out until enrolled)
- Input device ▸ (list)
- Open settings… · Quit

**Settings window (egui):**
- Input device and output device (default "CABLE Input")
- Live meters: input level (with clip indicator), output level, **noise removed (dB)**
- Live stats: **added latency (ms, measured)**, **CPU % of one core**, worst-case frame time, underrun count
- Strength slider, make-up gain, monitor toggle
- Mute hotkey (default `Ctrl+Alt+M`) with an on-screen toast
- Start with Windows ✓
- First-run wizard: check VB-Cable, pick mic, set level/headroom, calibrate speaking level (for §5.3.4), show a softphone setup guide

**Device robustness:**
- Register `IMMNotificationClient`. Handle the mic being unplugged, the default device changing, and Bluetooth headset switching between hands-free and stereo profiles. Reopen streams automatically within about 1 s with no crash.
- Handle the PC sleeping and waking.

---

## 8. Phase 0: Benchmark & listening harness (build this FIRST)

Before any UI, build a CLI `qm-bench`, which can run on macOS:

```
qm-bench --model dfn3-ll|dfn3|rnnoise --in noisy.wav --out clean.wav [--atten-lim 30]
```
It must report:
- Real-time factor, mean and **p99/max per-frame processing time**
- Algorithmic latency (from the model config) plus measured end-to-end delay (cross-correlate the clean reference with the output)
- If a clean reference exists: SI-SDR and PESQ/STOI (a crate or a small Python script is fine for *offline* metrics). Optionally DNSMOS (ONNX).

### Test set (`testdata/`, generated by a script; do not commit large audio)
- **Clean speech:** a few minutes of free read speech (e.g. LibriSpeech or VCTK, check licences).
- **Noise:** use the free **Microsoft DNS Challenge noise set** (CC-BY-4.0) and/or **Freesound CC0** clips, picked to match §1:
  rain on metal/tin roof, heavy rain, dog barks, car horns/traffic honking, distant shouting/yelling (ideally non-English), café/office chatter.
- Mix at SNRs of **−5, 0, 5, 10 dB**. Include **impulsive mixes** where barks and horns come in at high level over speech.
- Simulate far-field yelling: apply a reverb impulse response plus low-pass filtering before mixing.

### Phase 0 deliverable
1. A table comparing DFN3-LL, DFN3 and RNNoise on every noise category: metrics, RTF, p99 frame time.
2. Before/after WAV pairs for each category, for the owner to listen to.
3. A **CPU estimate for the i5-8250U**. Benchmark on the Mac, then validate on the owner's PC, or on any 8th-gen i5 / CI Windows runner, with turbo disabled to simulate battery throttling.
4. A recommendation for which model is the default.

Get the owner's sign-off on the listening results before building Phase 1.

---

## 9. Softphone compatibility

- Softphones (MicroSIP, Zoiper, 3CX, Bria, RingCentral, Teams, Webex, Zoom Phone) usually run at 8 or 16 kHz with their own echo cancellation (AEC), noise suppression (NS) and AGC.
- QuietMic outputs **48 kHz**. Windows resamples to whatever the softphone opens. Make sure "CABLE Output" is set to 48 kHz in Windows Sound settings, and document this.
- **Keep the softphone's AEC on.** Recommend turning the softphone's own NS **off or low** to avoid double processing ("underwater" sound). Turn AGC off if the voice pumps.
- Strongly recommend a **headset**: it improves AEC and makes the distant-voice gate far more effective.
- Write `docs/SOFTPHONE_SETUP.md` with per-app steps for the common ones above: where the mic selector and the NS/AEC toggles are.

---

## 10. Repository layout (suggested)

```
noise-cancel/
├── BUILD_SPEC.md              ← this file
├── README.md
├── Cargo.toml                 (workspace)
├── crates/
│   ├── qm-dsp/                denoiser wrappers, gates, limiter, resampler (platform-independent, unit-tested)
│   ├── qm-audio-win/          WASAPI capture/render, MMCSS, device notifications (cfg(windows))
│   ├── qm-voiceprint/         VAD + speaker embedding + enrollment (Phase 2)
│   ├── qm-bench/              CLI benchmark harness (Phase 0)
│   └── qm-app/                tray + egui settings + config; produces QuietMic.exe
├── models/                    downloaded by script; not committed (or Git LFS)
├── scripts/
│   ├── fetch_models.(sh|ps1)
│   └── make_testset.py
├── testdata/                  generated, gitignored
├── docs/SOFTPHONE_SETUP.md
├── installer/                 Inno Setup script (Phase 3)
└── .github/workflows/build.yml  windows-latest: build, test, upload QuietMic.exe artifact
```

`qm-dsp` must stay platform-independent so it can be developed and tested on macOS.

---

## 11. Phases & acceptance criteria

### Phase 0: Benchmark (see §8)
- [ ] `qm-bench` works with all three models
- [ ] Results table and WAV pairs delivered; owner signs off on the default model

### Phase 1: Working app
- [ ] QuietMic.exe runs from the tray. Selecting "CABLE Output" in any app gives the processed voice
- [ ] Measured added latency ≤ 30 ms, shown live in the UI
- [ ] On the minimum spec (8th-gen i5): Quality mode uses ≤ 15% of one core on average, p99 frame time ≤ 5 ms, and **0 underruns in a 60-min test call**
- [ ] Auto-fallback to RNNoise under CPU starvation works without an audible glitch
- [ ] Transient guard + distant-voice reduction implemented and tuned on the test set; no clipping of the user's own speech onsets
- [ ] Survives unplugging/replugging the mic, changing the default device, and sleep/wake
- [ ] Mute hotkey, strength, mode switch, start-with-Windows all work
- [ ] VB-Cable missing → clear guidance dialog
- [ ] Tested in at least one softphone (e.g. MicroSIP) and Zoom/Meet in a browser

### Phase 2: "Only my voice"
- [ ] In-app enrollment wizard with quality checks
- [ ] Gate attenuates other speakers when the user is silent; no added audio-path latency; ≤ 3% extra CPU on the minimum spec
- [ ] Voiceprint stored locally only; delete button works

### Phase 3: Polish
- [ ] Installer (Inno Setup), presets (e.g. "Rain", "Street", "Max"), update check (GitHub Releases, opt-in), `SOFTPHONE_SETUP.md`

---

## 12. Rules for the implementing agent

- **Measure, don't assume.** Every latency/CPU claim must come from `qm-bench` output or the in-app stats.
- **Real-time safety first:** no allocation, locking, I/O or logging on the audio/DSP threads. Use `assert_no_alloc` in debug builds to enforce this.
- Keep the DSP crate pure and unit-tested (frame sizes, resampler drift, limiter, gate state machines).
- Don't commit model files or test audio; fetch them with scripts and pin model versions plus SHA-256 hashes.
- The dev machine is **macOS**; the target is **Windows 11 x64**. Cross-compile with `cargo-xwin` and rely on GitHub Actions to produce the Windows `.exe` for the owner to test.
- Verify the facts in this spec that are marked "check"/"verify" (e.g. the exact DFN low-latency model name and its real delay) against the upstream repos before relying on them.
- When something is a trade-off the owner should decide (quality vs latency vs CPU), present the measured numbers and ask.

---

## 13. Phase 0 findings (measured; supersedes assumptions above where they conflict)

Full tables: `results/RESULTS.md`. Reproduce: `scripts/fetch_models.sh`, `scripts/fetch_testdata.sh`, `.venv/bin/python scripts/make_testset.py`, `cargo build --release -p qm-bench`, `.venv/bin/python scripts/run_bench.py`. Numbers below are from an **Apple M1 Pro, not the i5-8250U target**.

### Integration facts (save yourself the debugging)
- **crates.io `deep_filter` 0.2.5 has no model runtime.** Use the git repo: `deep_filter = { git = ".../Rikorose/DeepFilterNet", rev = "d375b2d8…", features = ["tract"] }` (crate lib name `df`, module `df::tract`).
- **Pin tract to exactly 0.21.4** (`=0.21.4` on tract-core/onnx/pulse/hir). 0.21.19 (what cargo picks) breaks libDF's `symbol_table` calls.
- **Use the CLI thresholds** `with_thresholds(-15, 35, 35)`. `RuntimeParams::default()` uses (-10, 30, 20) and cost standard DFN3 ~13 dB SI-SDR (6.9 dB vs 20.0 dB on the upstream sample). Our wrapper now matches upstream's `deep-filter` output exactly.
- Model files (`models/*.tar.gz`, hashes in `models/SHA256SUMS`) load directly with `DfParams::new(path)`. Both are 48 kHz, FFT 960 / hop 480 (10 ms).
- Real algorithmic delay = `(fft - hop) + lookahead * hop`: **DFN3-LL 10 ms** (lookahead 0), **standard DFN3 30 ms** (lookahead 2), RNNoise 10 ms. Cross-correlation measured exactly these values. Standard DFN3 therefore does **not** fit the ≤30 ms total budget; the spec's earlier "~40 ms" figure was approximate.
- DFN3-LL is a **bigger network** (36 MB, hidden 512 × 3 GRU layers) than standard DFN3 (8 MB, hidden 256), so it costs about 2× the CPU.
- `cargo check --target x86_64-pc-windows-msvc` fails on macOS (tract-linalg's build needs `lib.exe`). Build Windows binaries on GitHub Actions (`.github/workflows/build.yml`) or with `cargo-xwin` (downloads the Microsoft SDK; licence acceptance needed).

### Quality (averages over SNR -5/0/5/10 dB; SI-SDR gain in dB)
| category | RNNoise | DFN3 | **DFN3-LL** |
|---|---|---|---|
| rain | +5.6 | +9.2 | **+9.5** |
| rain on steel (synthetic) | +6.7 | +10.1 | **+11.1** |
| dog barks | -0.2 | +10.5 | **+11.4** |
| car horns | +5.7 | +11.4 | **+12.7** |
| far-field yelling (synthetic TTS) | +0.2 | +1.8 | +1.4 |
| babble (nearby talkers) | -0.8 | **-7.6** | -1.1 |
| everything at once | +3.1 | +7.3 | **+10.8** |

### What this means for the design
1. **Default = DFN3-LL.** Best quality on every owner-relevant noise, and the only DFN variant that fits the delay budget (10 ms).
2. **RNNoise is a poor fallback for this owner's noise.** It does nothing for dog barks and lets steel-roof rain through between words (15 dB vs 38-48 dB pause suppression). Keep it only as an emergency mode for CPU starvation, and tell the user quality drops.
3. **No model removes distant yelling** (+1-2 dB; only 7-10 dB quieter in pauses). The level-based distant-voice gate (§5.3.4) is therefore **required, not optional**. Prototype and tune it first in Phase 1.
4. **Nearby babble is handled badly by all models** (standard DFN3 actually damages the target voice). Acceptable, since the owner says this is rare; Phase 2's voiceprint gate is the answer.
5. **CPU is the main risk.** On the M1 Pro: DFN3-LL 1.7 ms per 10 ms frame (17% of a core, p99 1.9 ms, but one 12 ms spike in the whole run), DFN3 0.9 ms (9%), RNNoise 0.04 ms (0.4%). The i5-8250U is expected to be roughly 2-3× slower per core, i.e. DFN3-LL ≈ 35-50% of a core and 3.5-5 ms per frame (**an estimate, not a measurement**). Frame time spikes under thermal throttling could then exceed the 10 ms hop. Measure with `scripts/make_bench_pack.sh` + `scripts/bench_windows.ps1` on the real PC before building Phase 1. If DFN3-LL is too slow there, the options are: INT8 quantisation of the ONNX models, a 20 ms hop, or a smaller custom-distilled model.
6. The one 12 ms frame spike on a quiet desktop means the real-time path needs a jitter buffer of ≥ 1 extra frame or the §5.1 auto-fallback logic, as already specified.

### Test-set caveats
- Rain on a steel roof and the far-field yelling are **synthetic approximations** (flagged in `testdata/set/manifest.json`). Real recordings from the owner will be more reliable: re-run `run_bench.py` after dropping them in.
- Measured SNR of the synthetic-noise sets can be ~2 dB off nominal (rain_steel, worst_case). The table averages are still comparable between models.
- The "pause suppression" metric is `n/a` for dog barks because barks rarely fall into speech pauses.
- ESC-50 clips are CC BY-NC 3.0 and LibriSpeech is CC BY 4.0: fine for personal benchmarking, do not redistribute `testdata/`.
