#!/usr/bin/env python3
"""Grid over residual-gate settings for one model: leak vs voice damage. Uses files made by eval_presets.py."""
import subprocess, sys
from pathlib import Path
import numpy as np
from pystoi import stoi
from scipy.io import wavfile

ROOT = Path(__file__).resolve().parent.parent
QM, SET, WORK = ROOT / "target/release/qm", ROOT / "testdata/set", ROOT / "testdata/engine"
SR, LEARN = 48_000, 10
CATS = ["rain", "rain_steel", "dog_bark", "car_horn", "yelling_far", "worst_case"]
model = sys.argv[1] if len(sys.argv) > 1 else "dfn3-ll"
rd = lambda p: wavfile.read(p)[1].astype(np.float64) / 32768

def run(src, dst, gate_args):
    subprocess.run([str(QM), "--models-dir", str(ROOT / "models"), "process", "--preset", "low-latency", "--model", model, str(src), str(dst)] + gate_args,
                   capture_output=True, check=True)

configs = [("off", ["--no-gate"]),
           ("t20 r4 m45 h120 (current)", []),
           ("t30 r2 m15 h250", ["--gate-threshold", "30", "--gate-ratio", "2", "--gate-max", "15", "--gate-hold", "250"]),
           ("t30 r3 m25 h300", ["--gate-threshold", "30", "--gate-ratio", "3", "--gate-max", "25", "--gate-hold", "300"]),
           ("t40 r3 m30 h300", ["--gate-threshold", "40", "--gate-ratio", "3", "--gate-max", "30", "--gate-hold", "300"]),
           ("t25 r2 m12 h400", ["--gate-threshold", "25", "--gate-ratio", "2", "--gate-max", "12", "--gate-hold", "400"])]
print(f"model {model}")
print(f"{'gate':<28} {'voiceSTOI':>9} {'mixSTOI':>8} | noise-only attenuation dB: " + " ".join(f"{c[:6]:>7}" for c in CATS))
for name, args in configs:
    leak = []
    for cat in CATS:
        src = WORK / f"{cat}.noise_only.wav"
        run(src, WORK / "g.wav", args)
        a, b = rd(src), rd(WORK / "g.wav")
        i = LEARN * SR + SR // 2
        leak.append(10 * np.log10(((a[i:] ** 2).sum() + 1e-12) / ((b[i:len(a)] ** 2).sum() + 1e-12)))
    clean = rd(SET / "rain.clean.wav")
    run(SET / "rain.clean.wav", WORK / "g.wav", args)
    o = rd(WORK / "g.wav"); n = min(len(o), len(clean))
    vs = stoi(clean[:n], o[:n], SR)
    ms = []
    for cat in ("rain_steel", "dog_bark", "worst_case"):
        run(SET / f"{cat}_snr0.wav", WORK / "g.wav", args)
        o = rd(WORK / "g.wav"); c = rd(SET / f"{cat}.clean.wav"); n = min(len(o), len(c))
        ms.append(stoi(c[:n], o[:n], SR))
    print(f"{name:<28} {vs:9.3f} {np.mean(ms):8.3f} | " + " ".join(f"{x:7.1f}" for x in leak), flush=True)
