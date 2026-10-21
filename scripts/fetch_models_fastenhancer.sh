#!/bin/sh
# FastEnhancer 48 kHz spec2spec ONNX models (MIT, https://github.com/aask1357/fastenhancer, release onnx-48khz-v1).
set -eu
cd "$(dirname "$0")/.."
mkdir -p models/fastenhancer
for m in t b s m l; do
  f=models/fastenhancer/fastenhancer_${m}_spec.onnx
  [ -f "$f" ] || curl -fL --retry 3 -o "$f" "https://github.com/aask1357/fastenhancer/releases/download/onnx-48khz-v1/fastenhancer_${m}_spec.onnx"
done
(cd models/fastenhancer && if [ -f SHA256SUMS ]; then shasum -a 256 -c SHA256SUMS; else shasum -a 256 *.onnx | tee SHA256SUMS; fi)
