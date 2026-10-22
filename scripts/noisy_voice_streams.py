#!/usr/bin/env python3
"""Mix noise into the voice-gate test streams and run them through the denoiser.

usage: noisy_voice_streams.py DIR [SNR_DB]   (DIR holds s<N>.wav from `eval_gate --dump DIR`; writes p<N>.wav)
Noise rotates through the engine noise_only files; SNR is measured on the non-silent samples.
"""
import glob, os, subprocess, sys
import numpy as np
from scipy.io import wavfile
from scipy.signal import resample_poly

d = sys.argv[1]
snr = float(sys.argv[2]) if len(sys.argv) > 2 else 5.0
preset = os.environ.get("PRESET", "auto")
noises = []
for f in sorted(glob.glob("testdata/engine/*.noise_only.wav")):
    r, x = wavfile.read(f)
    noises.append((os.path.basename(f).split(".")[0], x.astype(np.float32) / 32768))
rng = np.random.default_rng(1)
files = sorted(glob.glob(f"{d}/s*.wav"), key=lambda p: int(os.path.basename(p)[1:-4]))
for i, f in enumerate(files):
    idx = os.path.basename(f)[1:-4]
    _, x = wavfile.read(f)
    x48 = resample_poly(x, 3, 1).astype(np.float32)
    name, nz = noises[i % len(noises)]
    start = rng.integers(0, len(nz))
    n = np.resize(np.roll(nz, -start), len(x48))
    act = np.abs(x48) > 1e-4
    ps = np.mean(x48[act] ** 2)
    pn = np.mean(n ** 2)
    n = n * np.sqrt(ps / (pn * 10 ** (snr / 10)))
    mix = np.clip(x48 + n, -1, 1)
    wavfile.write(f"{d}/m{idx}.wav", 48000, (mix * 32767).astype(np.int16))
    subprocess.run(["./target/release/qm", "process", "--preset", preset, f"{d}/m{idx}.wav", f"{d}/o{idx}.wav"], check=True, capture_output=True)
    sr, o = wavfile.read(f"{d}/o{idx}.wav")
    o = o.astype(np.float32) / (32768 if o.dtype == np.int16 else 1)
    wavfile.write(f"{d}/p{idx}.wav", 16000, resample_poly(o, 1, 3).astype(np.float32))
    wavfile.write(f"{d}/r{idx}.wav", 16000, resample_poly(mix, 1, 3).astype(np.float32))
    print(idx, name, flush=True)
