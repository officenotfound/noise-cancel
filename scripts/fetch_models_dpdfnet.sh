#!/bin/sh
# DPDFNet 48 kHz streaming ONNX models (code Apache-2.0: https://github.com/ceva-ip/DPDFNet;
# the Hugging Face model card states no licence for the weights, so confirm before redistributing).
set -eu
cd "$(dirname "$0")/.."
mkdir -p models/dpdfnet
for m in dpdfnet2_48khz_hr dpdfnet8_48khz_hr; do
  f=models/dpdfnet/$m.onnx
  [ -f "$f" ] || curl -fL --retry 3 -o "$f" "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/$m.onnx"
done
(cd models/dpdfnet && if [ -f SHA256SUMS ]; then shasum -a 256 -c SHA256SUMS; else shasum -a 256 *.onnx | tee SHA256SUMS; fi)
