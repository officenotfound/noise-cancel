#!/bin/sh
# Fetch raw source audio for the Phase 0 test set into testdata/raw (gitignored; do not redistribute).
#   - LibriSpeech "mini" dev-clean-2 (CC BY 4.0, ~126 MB) -> clean read speech
#   - ESC-50 clips (CC BY-NC 3.0, ~0.4 MB each) -> rain, dog bark, car horn noise
set -eu
cd "$(dirname "$0")/.."
RAW=testdata/raw
mkdir -p "$RAW/esc50" "$RAW/librispeech"

if [ ! -d "$RAW/librispeech/LibriSpeech" ]; then
  curl -fL --retry 3 -o "$RAW/librispeech/dev-clean-2.tar.gz" https://www.openslr.org/resources/31/dev-clean-2.tar.gz
  tar xzf "$RAW/librispeech/dev-clean-2.tar.gz" -C "$RAW/librispeech"
  rm "$RAW/librispeech/dev-clean-2.tar.gz"
fi

META="$RAW/esc50/esc50.csv"
[ -f "$META" ] || curl -fsSL -o "$META" https://raw.githubusercontent.com/karolpiczak/ESC-50/master/meta/esc50.csv
# class target ids: 0 dog, 10 rain, 43 car_horn. 12 clips each.
for target in 0 10 43; do
  awk -F, -v t="$target" 'NR>1 && $3==t {print $1}' "$META" | head -12 | while read -r f; do
    [ -f "$RAW/esc50/$f" ] || curl -fsSL --retry 3 -o "$RAW/esc50/$f" "https://raw.githubusercontent.com/karolpiczak/ESC-50/master/audio/$f"
  done
done
echo "done: $(ls "$RAW/esc50"/*.wav | wc -l) ESC-50 clips"

# Simulated room impulse response (from the DeepFilterNet repo, MIT/Apache-2.0) used for far-field voices.
mkdir -p "$RAW/rir"
RIR=rir_sim_1001_w11.7_l2.6_h2.5_rt60_0.7919.wav
[ -f "$RAW/rir/$RIR" ] || curl -fsSL -o "$RAW/rir/$RIR" "https://github.com/Rikorose/DeepFilterNet/raw/main/assets/$RIR"
