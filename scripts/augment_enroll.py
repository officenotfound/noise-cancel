#!/usr/bin/env python3
"""Noise-augmented enrollment copies: e<T>.wav + each noise at 5-15 dB, denoised -> ea<T>_<k>.wav.  usage: augment_enroll.py DIR"""
import glob, os, subprocess, sys
import numpy as np
from scipy.io import wavfile
from scipy.signal import resample_poly
d = sys.argv[1]
noises = [(os.path.basename(f).split(".")[0], wavfile.read(f)[1].astype(np.float32) / 32768) for f in sorted(glob.glob("testdata/engine/*.noise_only.wav"))]
rng = np.random.default_rng(7)
for f in glob.glob(f"{d}/e[0-9]*.wav"):
    t = os.path.basename(f)[1:-4]
    x = resample_poly(wavfile.read(f)[1], 3, 1).astype(np.float32)
    for k, (name, nz) in enumerate(noises):
        n = np.resize(np.roll(nz, -int(rng.integers(0, len(nz)))), len(x))
        snr = rng.uniform(5, 15)
        n = n * np.sqrt(np.mean(x ** 2) / (np.mean(n ** 2) * 10 ** (snr / 10)))
        wavfile.write(f"{d}/am.wav", 48000, (np.clip(x + n, -1, 1) * 32767).astype(np.int16))
        subprocess.run(["./target/release/qm", "process", "--preset", "auto", f"{d}/am.wav", f"{d}/ao.wav"], check=True, capture_output=True)
        _, o = wavfile.read(f"{d}/ao.wav")
        o = o.astype(np.float32) / (32768 if o.dtype == np.int16 else 1)
        wavfile.write(f"{d}/ea{t}_{k}.wav", 16000, resample_poly(o, 1, 3).astype(np.float32))
