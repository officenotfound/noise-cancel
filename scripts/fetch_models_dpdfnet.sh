#!/bin/sh
# DPDFNet 48 kHz streaming ONNX models (code Apache-2.0: https://github.com/ceva-ip/DPDFNet;
# the Hugging Face model card states no licence for the weights, so confirm before redistributing).
set -eu
# shasum is not in every environment (Git Bash on Windows CI has sha256sum)
sha() { if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$@"; else sha256sum "$@"; fi; }
cd "$(dirname "$0")/.."
mkdir -p models/dpdfnet
for m in dpdfnet2_48khz_hr dpdfnet8_48khz_hr; do
  f=models/dpdfnet/$m.onnx
  [ -f "$f" ] || curl -fL --retry 3 -o "$f" "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/$m.onnx"
done
(cd models/dpdfnet && if [ -f SHA256SUMS ]; then sha -c SHA256SUMS; else sha *.onnx | tee SHA256SUMS; fi)
