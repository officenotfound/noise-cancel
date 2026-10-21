#!/usr/bin/env python3
"""Evaluate QuietMic presets with the real engine (qm CLI): does noise get through, is the voice damaged, how much delay.

Tests per preset (all at 48 kHz, 480-sample callbacks):
  leak    : 10 s of clean speech (lets the gate learn the voice) then 20 s of NOISE ONLY (noisy - clean, 0 dB SNR level).
            Reports attenuation of the noise part in dB and the share of 10 ms frames still above -60 dBFS.
  damage  : clean speech with no noise through the engine: STOI and SI-SDR vs the original (1.0 / high = untouched).
  mixed   : noisy speech at 0 dB SNR: SI-SDR gain and STOI vs clean.
Run via .venv/bin/python scripts/eval_presets.py ; writes results/PRESETS.md and results/presets.json
"""
from __future__ import annotations

import json, re, subprocess
from pathlib import Path

import numpy as np
from pystoi import stoi
from scipy.io import wavfile

ROOT = Path(__file__).resolve().parent.parent
SET = ROOT / "testdata/set"
WORK = ROOT / "testdata/engine"
RES = ROOT / "results"
QM = ROOT / "target/release/qm"
CATS = ["rain", "rain_steel", "dog_bark", "car_horn", "yelling_far", "worst_case"]
PRESETS = ["low-latency", "balanced", "max-noise-removal", "max-voice-separation", "light"]
SR = 48_000
LEARN_S = 10


def rd(p) -> np.ndarray:
    return wavfile.read(p)[1].astype(np.float64) / 32768


def wr(p, x):
    wavfile.write(p, SR, (np.clip(x, -1, 1) * 32767).astype(np.int16))


def run(preset, src, dst, model=None, gate=True):
    cmd = [str(QM), "--models-dir", str(ROOT / "models"), "process", "--preset", preset, str(src), str(dst)]
    if model:
        cmd += ["--model", model]
    if not gate:
        cmd += ["--no-gate"]
    r = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return float(re.search(r"total delay ([\d.]+) ms", r.stderr).group(1))


def pick(preset) -> str:
    out = subprocess.run([str(QM), "--models-dir", str(ROOT / "models"), "calibrate", "--preset", preset],
                         capture_output=True, text=True, check=True).stdout
    return re.search(r"pick for '[^']+': (\S+)", out).group(1)


def si_sdr(ref, est):
    n = min(len(ref), len(est)); ref, est = ref[:n], est[:n]
    a = (est @ ref) / (ref @ ref + 1e-12); t = a * ref
    return 10 * np.log10((t @ t + 1e-12) / ((est - t) @ (est - t) + 1e-12))


def frames_db(x):
    n = len(x) // 480 * 480
    return 10 * np.log10((x[:n].reshape(-1, 480) ** 2).mean(1) + 1e-12)


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    RES.mkdir(exist_ok=True)
    for cat in CATS:
        clean = rd(SET / f"{cat}.clean.wav")
        noisy = rd(SET / f"{cat}_snr0.wav")
        wr(WORK / f"{cat}.noise_only.wav", np.concatenate([clean[: LEARN_S * SR], (noisy - clean)[: 20 * SR]]))
    rows = []
    variants = [(p, True) for p in PRESETS] + [("low-latency", False)]
    for preset, gate in variants:
        model = pick(preset)
        row = {"preset": preset + ("" if gate else " (gate off)"), "model": model, "leak": {}, "audible": {}, "mixed_gain": {}, "mixed_stoi": {}}
        for cat in CATS:
            nz_in = rd(WORK / f"{cat}.noise_only.wav")
            delay = run(preset, WORK / f"{cat}.noise_only.wav", WORK / "out.wav", model, gate)
            out = rd(WORK / "out.wav")
            a, b = nz_in[LEARN_S * SR + SR // 2:], out[LEARN_S * SR + SR // 2: len(nz_in)]
            row["leak"][cat] = float(10 * np.log10(((a ** 2).sum() + 1e-12) / ((b ** 2).sum() + 1e-12)))
            row["audible"][cat] = float((frames_db(b) > -60).mean() * 100)
            run(preset, SET / f"{cat}_snr0.wav", WORK / "mix.wav", model, gate)
            mix, clean = rd(WORK / "mix.wav"), rd(SET / f"{cat}.clean.wav")
            noisy = rd(SET / f"{cat}_snr0.wav")
            n = min(len(mix), len(clean))
            row["mixed_gain"][cat] = float(si_sdr(clean[:n], mix[:n]) - si_sdr(clean[:n], noisy[:n]))
            row["mixed_stoi"][cat] = float(stoi(clean[:n], mix[:n], SR))
        row["delay_ms"] = delay
        clean = rd(SET / "rain.clean.wav")
        run(preset, SET / "rain.clean.wav", WORK / "cl.wav", model, gate)
        o = rd(WORK / "cl.wav")
        n = min(len(o), len(clean))
        row["clean_stoi"] = float(stoi(clean[:n], o[:n], SR))
        row["clean_sisdr"] = float(si_sdr(clean[:n], o[:n]))
        row["clean_level_change_db"] = float(10 * np.log10((o[:n] ** 2).mean() / (clean[:n] ** 2).mean()))
        rows.append(row)
        print(f"{row['preset']:<34} {model:<8} delay {delay:5.1f} ms | leak dB " + " ".join(f"{row['leak'][c]:5.1f}" for c in CATS)
              + f" | voice STOI {row['clean_stoi']:.3f} level {row['clean_level_change_db']:+.1f} dB", flush=True)
    (RES / "presets.json").write_text(json.dumps(rows, indent=1))
    L = ["# Preset evaluation (qm engine, M1 Pro, 480-sample callbacks)", "",
         "Model = what calibration picks on this machine. **Noise-only attenuation** (dB, higher = less gets through): "
         "20 s of the noise alone at the level it had in the 0 dB mix, after 10 s of the user's voice so the gate has learned their level. "
         "**Audible** = share of 10 ms frames still louder than -60 dBFS. **Voice** = your clean speech through the engine (STOI 1.0 = identical).", "",
         "## Noise-only attenuation (dB)", "", "| preset | model | delay | " + " | ".join(CATS) + " |", "|---|---|---|" + "---|" * len(CATS)]
    for r in rows:
        L.append(f"| {r['preset']} | {r['model']} | {r['delay_ms']:.1f} ms | " + " | ".join(f"{r['leak'][c]:.1f}" for c in CATS) + " |")
    L += ["", "## Share of frames still audible (> -60 dBFS), %", "", "| preset | " + " | ".join(CATS) + " |", "|---|" + "---|" * len(CATS)]
    for r in rows:
        L.append(f"| {r['preset']} | " + " | ".join(f"{r['audible'][c]:.0f}" for c in CATS) + " |")
    L += ["", "## Mixed speech + noise at 0 dB: SI-SDR gain (dB) / STOI", "", "| preset | " + " | ".join(CATS) + " |", "|---|" + "---|" * len(CATS)]
    for r in rows:
        L.append(f"| {r['preset']} | " + " | ".join(f"{r['mixed_gain'][c]:+.1f} / {r['mixed_stoi'][c]:.2f}" for c in CATS) + " |")
    L += ["", "## Your voice with no noise (damage check)", "", "| preset | STOI | SI-SDR dB | level change dB |", "|---|---|---|---|"]
    for r in rows:
        L.append(f"| {r['preset']} | {r['clean_stoi']:.3f} | {r['clean_sisdr']:.1f} | {r['clean_level_change_db']:+.1f} |")
    (RES / "PRESETS.md").write_text("\n".join(L) + "\n")


if __name__ == "__main__":
    main()
