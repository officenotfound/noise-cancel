#!/bin/sh
# Voice models for the voice-print gate (all free):
#   silero_vad_16k_sequence.onnx              MIT (sequence variant: the main model has an If-branch that tract cannot analyse)           https://github.com/snakers4/silero-vad
#   wespeaker_en_voxceleb_resnet34.onnx     CC-BY-4.0     https://github.com/wenet-e2e/wespeaker (ONNX from k2-fsa/sherpa-onnx releases)
#   wespeaker_en_voxceleb_CAM++.onnx        CC-BY-4.0     same
# CC-BY-4.0 requires credit: the app's About box must name WeSpeaker and its authors.
set -eu
sha() { if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$@"; else sha256sum "$@"; fi; }
cd "$(dirname "$0")/.."
mkdir -p models/voice
get() { [ -f "models/voice/$1" ] || curl -fL --retry 3 -o "models/voice/$1" "$2"; }
get silero_vad_16k_sequence.onnx https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad_16k_sequence.onnx
SH=https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models
get wespeaker_en_voxceleb_resnet34.onnx "$SH/wespeaker_en_voxceleb_resnet34.onnx"
get wespeaker_en_voxceleb_CAM++.onnx "$SH/wespeaker_en_voxceleb_CAM++.onnx"
(cd models/voice && if [ -f SHA256SUMS ]; then sha -c SHA256SUMS; else sha *.onnx | tee SHA256SUMS; fi)
