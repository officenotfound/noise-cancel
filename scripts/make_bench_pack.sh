#!/bin/sh
# Assemble bench-pack/ : everything needed to time the denoisers on a Windows PC (e.g. the i5-8250U target).
# Usage: scripts/make_bench_pack.sh path/to/qm-bench.exe   (built by CI or cross-compiled)
set -eu
cd "$(dirname "$0")/.."
EXE=${1:?path to qm-bench.exe}
P=bench-pack
rm -rf "$P" && mkdir -p "$P/models/fastenhancer" "$P/models/dpdfnet" "$P/audio"
cp "$EXE" "$P/qm-bench.exe"
cp models/DeepFilterNet3_onnx.tar.gz models/DeepFilterNet3_ll_onnx.tar.gz "$P/models/"
cp models/fastenhancer/*_spec.onnx "$P/models/fastenhancer/"
cp models/dpdfnet/*.onnx "$P/models/dpdfnet/"
for f in rain_steel_snr0 dog_bark_snr0 car_horn_snr0 worst_case_snr0; do cp "testdata/set/$f.wav" "$P/audio/"; done
cp scripts/bench_windows.ps1 "$P/run.ps1"
(cd "$P" && zip -qr ../bench-pack.zip .)
echo "wrote bench-pack.zip ($(du -h bench-pack.zip | cut -f1))"
