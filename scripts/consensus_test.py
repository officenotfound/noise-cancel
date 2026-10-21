#!/usr/bin/env python3
"""Offline test: mute the main model's output when a cheap second model (fe-b) says nobody is speaking."""
import subprocess, sys
from pathlib import Path
import numpy as np
from pystoi import stoi
from scipy.io import wavfile

ROOT = Path(__file__).resolve().parent.parent
QM, SET, WORK = ROOT / "target/release/qm", ROOT / "testdata/set", ROOT / "testdata/engine"
SR, LEARN = 48_000, 10
CATS = ["rain", "rain_steel", "dog_bark", "car_horn", "yelling_far", "worst_case"]
rd = lambda p: wavfile.read(p)[1].astype(np.float64) / 32768

def run(model, src, dst):
    subprocess.run([str(QM), "--models-dir", str(ROOT / "models"), "process", "--preset", "low-latency", "--model", model, "--no-gate", str(src), str(dst)],
                   capture_output=True, check=True)

def consensus(main, det, thresh_db, atten_db, hold_ms):
    n = min(len(main), len(det)) // 480 * 480
    lvl = 10 * np.log10((det[:n].reshape(-1, 480) ** 2).mean(1) + 1e-12)
    open_ = np.zeros(len(lvl), bool); hold = 0
    for i, l in enumerate(lvl):
        if l > thresh_db: hold = int(hold_ms / 10)
        open_[i] = hold > 0
        hold = max(0, hold - 1)
    target = np.where(open_, 1.0, 10 ** (-atten_db / 20))
    g = np.empty(n); cur = 1.0
    a_att, a_rel = 1 - np.exp(-1 / (0.005 * SR)), 1 - np.exp(-1 / (0.02 * SR))
    tgt = np.repeat(target, 480)
    for i in range(n):
        cur += (a_att if tgt[i] < cur else a_rel) * (tgt[i] - cur)
        g[i] = cur
    return main[:n] * g

def si_sdr(ref, est):
    n = min(len(ref), len(est)); ref, est = ref[:n], est[:n]
    a = (est @ ref) / (ref @ ref + 1e-12); t = a * ref
    return 10 * np.log10((t @ t + 1e-12) / ((est - t) @ (est - t) + 1e-12))

main_model, det_model = sys.argv[1], sys.argv[2]
configs = [("none", None), ("det>-50 h300 a30", (-50, 30, 300)), ("det>-55 h300 a30", (-55, 30, 300)), ("det>-45 h300 a40", (-45, 40, 300)), ("det>-50 h200 a20", (-50, 20, 200))]
print(f"main {main_model}, detector {det_model}")
res = {name: {"leak": [], "mix": []} for name, _ in configs}
for cat in CATS:
    src = WORK / f"{cat}.noise_only.wav"
    run(main_model, src, WORK / "m.wav"); run(det_model, src, WORK / "d.wav")
    a, m, d = rd(src), rd(WORK / "m.wav"), rd(WORK / "d.wav")
    i = LEARN * SR + SR // 2
    for name, c in configs:
        o = m if c is None else consensus(m, d, *c)
        res[name]["leak"].append(10 * np.log10(((a[i:] ** 2).sum() + 1e-12) / ((o[i:len(a)] ** 2).sum() + 1e-12)))
    nz = SET / f"{cat}_snr0.wav"
    run(main_model, nz, WORK / "m.wav"); run(det_model, nz, WORK / "d.wav")
    clean, m, d = rd(SET / f"{cat}.clean.wav"), rd(WORK / "m.wav"), rd(WORK / "d.wav")
    for name, c in configs:
        o = m if c is None else consensus(m, d, *c)
        n = min(len(o), len(clean))
        res[name]["mix"].append((stoi(clean[:n], o[:n], SR), si_sdr(clean[:n], o[:n]) - si_sdr(clean[:n], rd(nz)[:n])))
run(main_model, SET / "rain.clean.wav", WORK / "m.wav"); run(det_model, SET / "rain.clean.wav", WORK / "d.wav")
clean, m, d = rd(SET / "rain.clean.wav"), rd(WORK / "m.wav"), rd(WORK / "d.wav")
print(f"{'consensus':<20} {'voiceSTOI':>9} {'mixSTOI':>8} {'mixGain':>8} | noise-only attenuation dB: " + " ".join(f"{c[:6]:>7}" for c in CATS))
for name, c in configs:
    o = m if c is None else consensus(m, d, *c); n = min(len(o), len(clean))
    mix = np.array(res[name]["mix"])
    print(f"{name:<20} {stoi(clean[:n], o[:n], SR):9.3f} {mix[:,0].mean():8.3f} {mix[:,1].mean():+8.1f} | " + " ".join(f"{x:7.1f}" for x in res[name]["leak"]), flush=True)
