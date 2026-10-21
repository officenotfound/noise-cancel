#!/usr/bin/env python3
"""Build the Phase 0 test set: clean speech + owner-specific noises mixed at several SNRs.

Run via .venv/bin/python scripts/make_testset.py. Needs testdata/raw (scripts/fetch_testdata.sh), ffmpeg,
and macOS `say` (only for the distant non-English yelling; skipped if unavailable).

Output (testdata/set/):  <category>_snr<N>.wav (noisy), <category>.clean.wav (reference), manifest.json
All files are 48 kHz mono 16-bit, 20 s. SNR is measured on *active* frames only, so sparse noises
(barks, horns, yells) are as loud as requested while they are sounding, not averaged over silence.

Synthetic parts (flagged in the manifest): "rain_steel" (rain + modal resonances of a metal panel) and
"yelling_far" (TTS voices, boosted + clipped for vocal effort, then reverb + low-pass for distance).
Replace both with real recordings from the owner when available.
"""
from __future__ import annotations

import json, random, subprocess, sys, tempfile
from pathlib import Path

import numpy as np
from scipy import signal
from scipy.io import wavfile

SR = 48_000
DUR_S = 20
N = SR * DUR_S
SNRS = [-5, 0, 5, 10]
ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "testdata/raw"
OUT = ROOT / "testdata/set"


def load(path) -> np.ndarray:
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-f", "f32le", "-ac", "1", "-ar", str(SR), "-"],
                         capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64)


def active_rms(x: np.ndarray, floor_db=-40.0) -> float:
    """RMS over 20 ms frames within `floor_db` of the loudest frame."""
    f = x[: len(x) // 960 * 960].reshape(-1, 960)
    e = (f ** 2).mean(axis=1)
    keep = e > e.max() * 10 ** (floor_db / 10)
    return float(np.sqrt(e[keep].mean())) if keep.any() else 1e-9


def fit(x: np.ndarray, n=N, rng=None) -> np.ndarray:
    """Loop with a short crossfade (or trim) to exactly n samples."""
    if len(x) >= n:
        return x[:n].copy()
    xf = 2400
    out = x.copy()
    while len(out) < n:
        w = np.linspace(0, 1, xf)
        out = np.concatenate([out[:-xf], out[-xf:] * (1 - w) + x[:xf] * w, x[xf:]])
    return out[:n]


def speech_for(idx: int) -> np.ndarray:
    """~20 s of read speech from one LibriSpeech speaker, active-RMS normalised to -24 dBFS."""
    spk_dirs = sorted(p for p in (RAW / "librispeech").rglob("*") if p.is_dir() and p.parent.name == "dev-clean-2")
    files = sorted(spk_dirs[idx % len(spk_dirs)].rglob("*.flac"))
    parts, total = [], 0
    for f in files:
        x = load(f)
        parts += [x, np.zeros(int(0.35 * SR))]
        total += len(x) + int(0.35 * SR)
        if total >= N:
            break
    s = np.concatenate(parts)[:N]
    return s * (10 ** (-24 / 20) / active_rms(s))


def clips(target_name: str):
    meta = [l.split(",") for l in (RAW / "esc50/esc50.csv").read_text().splitlines()[1:]]
    return [RAW / "esc50" / m[0] for m in meta if m[3] == target_name and (RAW / "esc50" / m[0]).exists()]


def rain(rng) -> np.ndarray:
    fs = clips("rain")
    return fit(np.concatenate([load(f) for f in rng.sample(fs, 5)]))


def rain_steel(rng) -> np.ndarray:
    """Approximation of rain on a metal roof: rain bed + dense random drop impacts ringing through panel modes."""
    bed = signal.sosfilt(signal.butter(2, 800, "hp", fs=SR, output="sos"), rain(rng))
    imp = np.zeros(N)
    hits = np.array(sorted(rng.sample(range(N), int(N / SR * 500))))  # ~500 drops/s
    imp[hits] = np.random.default_rng(rng.randrange(1 << 30)).exponential(1.0, len(hits))
    ring = np.zeros(N)
    for f0, q, g in [(380, 25, 1.0), (870, 30, 0.9), (1500, 35, 0.8), (2400, 40, 0.8), (3600, 40, 0.6), (5200, 40, 0.5)]:
        b, a = signal.iirpeak(f0, q, fs=SR)
        ring += g * signal.lfilter(b, a, imp)
    ring *= active_rms(bed) / active_rms(ring)
    return 0.45 * bed + ring


def sparse_events(clip_names, rng, per_clip_gap_s=(0.8, 2.5)) -> np.ndarray:
    """Place real ESC-50 events (digital-silence padding trimmed) at random gaps."""
    out = np.zeros(N)
    pos = int(rng.uniform(0.3, 1.5) * SR)
    pool = clips(clip_names)
    while pos < N:
        x = load(rng.choice(pool))
        keep = np.abs(x) > 1e-4
        if keep.any():
            idx = np.flatnonzero(keep)
            x = x[idx[0]: idx[-1] + 1]
        x = x[: int(rng.uniform(0.6, 2.2) * SR)]
        end = min(N, pos + len(x))
        out[pos:end] += x[: end - pos]
        pos = end + int(rng.uniform(*per_clip_gap_s) * SR)
    return out


VOICES = [("Milena", "ru", "Эй, ты! Иди сюда немедленно! Я сказал, иди сюда! Ты что, не слышишь?"),
          ("Paulina", "es", "¡Oye, tú! ¡Ven aquí ahora mismo! ¡Te dije que vengas, no me oyes!"),
          ("Majed", "ar", "يا أنت! تعال إلى هنا حالا! قلت لك تعال، ألا تسمعني؟")]


def tts(voice, text) -> np.ndarray | None:
    with tempfile.TemporaryDirectory() as d:
        p = Path(d) / "t.aiff"
        r = subprocess.run(["say", "-v", voice, "-r", "230", "-o", str(p), text], capture_output=True)
        return load(p) if r.returncode == 0 and p.exists() else None


def yelling_far(rng) -> np.ndarray:
    """Distant shouting through a wall/window: vocal-effort boost + clipping, then room reverb + low-pass."""
    rir = load(next((RAW / "rir").glob("*.wav")))
    rir = rir[np.argmax(np.abs(rir)):][: SR // 2]
    out = np.zeros(N)
    pos = int(rng.uniform(1, 3) * SR)
    while pos < N - SR:
        voice, _, text = rng.choice(VOICES)
        x = tts(voice, text)
        if x is None:
            continue
        b, a = signal.iirpeak(2200, 0.7, fs=SR)  # shout spectrum: more energy 1-4 kHz
        x = x + 1.5 * signal.lfilter(b, a, x)
        x = np.tanh(3 * x / (np.abs(x).max() + 1e-9))
        x = signal.fftconvolve(x, rir)[: len(x) + len(rir)]
        x = signal.sosfilt(signal.butter(2, 3500, "lp", fs=SR, output="sos"), x)
        end = min(N, pos + len(x))
        out[pos:end] += x[: end - pos]
        pos = end + int(rng.uniform(2, 5) * SR)
    return out


def babble(rng, idx) -> np.ndarray:
    return sum(speech_for(idx + k) for k in (3, 5, 7)) * 0.7


def mix(speech, noise, snr_db):
    noise = noise * (active_rms(speech) / active_rms(noise)) * 10 ** (-snr_db / 20)
    noisy = speech + noise
    g = min(1.0, 0.89 / max(np.abs(noisy).max(), np.abs(speech).max()))
    return speech * g, noisy * g


def write(path, x):
    wavfile.write(path, SR, (np.clip(x, -1, 1) * 32767).astype(np.int16))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    have_say = subprocess.run(["which", "say"], capture_output=True).returncode == 0
    cats = {
        "rain": (lambda r: rain(r), False),
        "rain_steel": (lambda r: rain_steel(r), True),
        "dog_bark": (lambda r: sparse_events("dog", r), False),
        "car_horn": (lambda r: sparse_events("car_horn", r, (0.5, 2.0)), False),
        "yelling_far": (lambda r: yelling_far(r), True),
        "babble": (lambda r: babble(r, 1), False),
        "worst_case": (None, True),  # rain_steel + bark + horn + yelling, all at once
    }
    manifest = []
    for i, (cat, (fn, synthetic)) in enumerate(cats.items()):
        if "yelling" in cat and not have_say:
            print("skip", cat, "(macOS `say` not available)"); continue
        rng = random.Random(1000 + i)
        speech = speech_for(i * 2)
        if cat == "worst_case":
            noise = (rain_steel(rng) / active_rms(rain_steel(random.Random(1))) * 0.6
                     + sparse_events("dog", rng) * 0.6 + sparse_events("car_horn", rng) * 0.6
                     + (yelling_far(rng) * 0.5 if have_say else 0))
        else:
            noise = fn(rng)
        for snr in SNRS:
            clean, noisy = mix(speech, noise, snr)
            write(OUT / f"{cat}.clean.wav", clean)
            write(OUT / f"{cat}_snr{snr}.wav", noisy)
            manifest.append({"category": cat, "snr_db": snr, "noisy": f"{cat}_snr{snr}.wav",
                             "reference": f"{cat}.clean.wav", "synthetic_noise": synthetic})
        print("built", cat)
    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=1))
    print(len(manifest), "files in", OUT)


if __name__ == "__main__":
    sys.exit(main())
