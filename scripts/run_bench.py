#!/usr/bin/env python3
"""Run qm-bench over the Phase 0 test set for every model; write results/results.json, results/RESULTS.md
and listening copies in results/listen/. Run via .venv/bin/python scripts/run_bench.py [--atten-lim N]."""
from __future__ import annotations

import argparse, json, shutil, subprocess
from pathlib import Path

import numpy as np
from pystoi import stoi
from scipy.io import wavfile

ROOT = Path(__file__).resolve().parent.parent
SET = ROOT / "testdata/set"
RES = ROOT / "results"
BENCH = ROOT / "target/release/qm-bench"
MODELS = ["rnnoise", "dfn3", "dfn3-ll", "fe-t", "fe-b", "fe-s", "fe-m", "fe-l"]
LISTEN_SNR = 0


def rd(p) -> np.ndarray:
    _, x = wavfile.read(p)
    return x.astype(np.float64) / 32768


def pause_suppression_db(clean, noisy, enh) -> float:
    """Noise-only frames (reference silent): how many dB quieter the output is than the input."""
    n = min(len(clean), len(noisy), len(enh)) // 480 * 480
    c, y, e = (v[:n].reshape(-1, 480) for v in (clean, noisy, enh))
    # Only frames where the target is silent AND noise is actually present (> -80 dBFS); digital silence has nothing to remove.
    quiet = ((c ** 2).mean(1) < (c ** 2).mean(1).max() * 1e-5) & ((y ** 2).mean(1) > 1e-8)
    if quiet.sum() < 10:
        return float("nan")
    return float(10 * np.log10(((y[quiet] ** 2).sum() + 1e-12) / ((e[quiet] ** 2).sum() + 1e-12)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--atten-lim", type=float, default=None)
    ap.add_argument("--tag", default="")
    args = ap.parse_args()
    out_dir = RES / f"audio{args.tag}"
    listen = RES / f"listen{args.tag}"
    shutil.rmtree(out_dir, ignore_errors=True)
    shutil.rmtree(listen, ignore_errors=True)

    rows = []
    for m in json.loads((SET / "manifest.json").read_text()):
        clean, noisy = rd(SET / m["reference"]), rd(SET / m["noisy"])
        nz = noisy - clean
        snr_meas = 20 * np.log10(np.sqrt((clean ** 2).mean()) / np.sqrt((nz ** 2).mean()))
        for model in MODELS:
            wav = out_dir / model / m["noisy"]
            wav.parent.mkdir(parents=True, exist_ok=True)
            cmd = [str(BENCH), "--model", model, "--in", str(SET / m["noisy"]), "--ref", str(SET / m["reference"]),
                   "--out", str(wav), "--models-dir", str(ROOT / "models"), "--json"]
            if args.atten_lim is not None and model.startswith("dfn"):
                cmd += ["--atten-lim", str(args.atten_lim)]
            r = json.loads(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)
            enh = rd(wav)
            n = min(len(clean), len(enh))
            r.update(cli=model, category=m["category"], snr_db=m["snr_db"], synthetic_noise=m["synthetic_noise"],
                     stoi_in=float(stoi(clean[:n], noisy[:n], 48000)), stoi_out=float(stoi(clean[:n], enh[:n], 48000)),
                     pause_suppression_db=pause_suppression_db(clean, noisy, enh), snr_measured_db=float(snr_meas))
            rows.append(r)
            print(f"{m['category']:12s} {m['snr_db']:>3} {model:8s} SI-SDR {r['si_sdr_gain_db']:+5.1f}  STOI {r['stoi_in']:.2f}->{r['stoi_out']:.2f}  pause {r['pause_suppression_db']:5.1f} dB")
            if m["snr_db"] == LISTEN_SNR:
                d = listen / f"{m['category']}_snr{LISTEN_SNR}"
                d.mkdir(parents=True, exist_ok=True)
                shutil.copy(wav, d / f"{model}.wav")
                shutil.copy(SET / m["noisy"], d / "0_noisy.wav")
                shutil.copy(SET / m["reference"], d / "0_clean_reference.wav")
    RES.mkdir(exist_ok=True)
    (RES / f"results{args.tag}.json").write_text(json.dumps(rows, indent=1))
    (RES / f"RESULTS{args.tag}.md").write_text(render(rows, args))


def render(rows, args) -> str:
    cats = list(dict.fromkeys(r["category"] for r in rows))
    def mean(rs, k):
        vals = [r[k] for r in rs if r[k] == r[k]]
        return float(np.mean(vals)) if vals else float("nan")
    L = [f"# Phase 0 results{' (' + args.tag + ')' if args.tag else ''}", "",
         f"Machine: Apple M1 Pro (arm64), qm-bench release build. attenuation limit: {args.atten_lim or 'default 100 dB'}. "
         "Averages are over SNR -5/0/5/10 dB. SI-SDR gain and STOI are against the clean reference; "
         "*pause suppression* = how many dB quieter the output is than the input in frames where the target speaker is silent "
         "(higher = noise/others removed between words).", ""]
    for title, key, fmt in [("SI-SDR gain (dB, higher is better)", "si_sdr_gain_db", "{:+.1f}"),
                            ("STOI after processing (0-1, higher is better; input shown first)", "stoi_out", "{:.2f}"),
                            ("Pause suppression (dB, higher is better)", "pause_suppression_db", "{:.1f}")]:
        L += [f"## {title}", "", "| category | " + (" input |" if key == "stoi_out" else "") + " | ".join(MODELS) + " |",
              "|---|" + ("---|" if key == "stoi_out" else "") + "---|" * len(MODELS)]
        for c in cats:
            cell = []
            for m in MODELS:
                rs = [r for r in rows if r["category"] == c and r["cli"] == m]
                v = mean(rs, key)
                cell.append("n/a" if v != v else fmt.format(v))
            inp = [f"{mean([r for r in rows if r['category'] == c and r['cli'] == MODELS[0]], 'stoi_in'):.2f}"] if key == "stoi_out" else []
            syn = " *(synthetic noise)*" if any(r["synthetic_noise"] for r in rows if r["category"] == c) else ""
            L.append(f"| {c}{syn} | " + " | ".join(inp + cell) + " |")
        L.append("")
    L += ["## Speed and delay (this machine; NOT the target i5-8250U)", "",
          "| model | hop | algorithmic delay | measured delay | frame mean | frame p99 | frame max | RTF (= share of one core) | frames over hop |", "|---|---|---|---|---|---|---|---|---|"]
    for m in MODELS:
        rs = [r for r in rows if r["cli"] == m]
        L.append(f"| {m} | {rs[0]['hop_ms']:.1f} ms | {rs[0]['algorithmic_delay_ms']:.1f} ms | {rs[0]['measured_delay_ms']:.1f} ms | {mean(rs, 'frame_ms_mean'):.2f} ms | "
                 f"{mean(rs, 'frame_ms_p99'):.2f} ms | {max(r['frame_ms_max'] for r in rs):.2f} ms | {100 * mean(rs, 'rtf'):.1f}% | {sum(r['frames_over_hop'] for r in rs)} |")
    L.append("")
    return "\n".join(L)


if __name__ == "__main__":
    main()
