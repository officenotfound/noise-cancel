#!/usr/bin/env python3
"""Run 16 kHz wavs (e*.wav) through the denoiser, no added noise; writes pe*.wav.  usage: denoise_16k.py DIR"""
import glob, os, subprocess, sys
import numpy as np
from scipy.io import wavfile
from scipy.signal import resample_poly
d = sys.argv[1]
for f in glob.glob(f"{d}/e*.wav"):
    i = os.path.basename(f)[1:-4]
    _, x = wavfile.read(f)
    wavfile.write(f"{d}/em{i}.wav", 48000, (np.clip(resample_poly(x, 3, 1), -1, 1) * 32767).astype(np.int16))
    subprocess.run(["./target/release/qm", "process", "--preset", os.environ.get("PRESET", "auto"), f"{d}/em{i}.wav", f"{d}/eo{i}.wav"], check=True, capture_output=True)
    _, o = wavfile.read(f"{d}/eo{i}.wav")
    o = o.astype(np.float32) / (32768 if o.dtype == np.int16 else 1)
    wavfile.write(f"{d}/pe{i}.wav", 16000, resample_poly(o, 1, 3).astype(np.float32))
