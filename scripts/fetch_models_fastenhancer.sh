#!/bin/sh
# FastEnhancer 48 kHz spec2spec ONNX models (MIT, https://github.com/aask1357/fastenhancer, release onnx-48khz-v1).
set -eu
# shasum is not in every environment (Git Bash on Windows CI has sha256sum)
sha() { if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$@"; else sha256sum "$@"; fi; }
cd "$(dirname "$0")/.."
mkdir -p models/fastenhancer
for m in t b s m l; do
  f=models/fastenhancer/fastenhancer_${m}_spec.onnx
  [ -f "$f" ] || curl -fL --retry 3 -o "$f" "https://github.com/aask1357/fastenhancer/releases/download/onnx-48khz-v1/fastenhancer_${m}_spec.onnx"
done
(cd models/fastenhancer && if [ -f SHA256SUMS ]; then sha -c SHA256SUMS; else sha *.onnx | tee SHA256SUMS; fi)
