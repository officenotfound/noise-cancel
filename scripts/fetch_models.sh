#!/bin/sh
# Fetch pretrained denoiser models into models/ and verify SHA-256 (pinned in models/SHA256SUMS).
set -eu
cd "$(dirname "$0")/.."
BASE=https://github.com/Rikorose/DeepFilterNet/raw/main/models
for f in DeepFilterNet3_onnx.tar.gz DeepFilterNet3_ll_onnx.tar.gz; do
  [ -f "models/$f" ] || curl -fL --retry 3 -o "models/$f" "$BASE/$f"
done
if [ -f models/SHA256SUMS ]; then (cd models && shasum -a 256 -c SHA256SUMS); else echo "no SHA256SUMS yet"; fi
