#!/bin/sh
# Convert the LibriSpeech dev-clean-2 speakers (fetched by fetch_testdata.sh) to 16 kHz mono WAV:
#   testdata/voice16k/<speaker>/<utterance>.wav
set -eu
cd "$(dirname "$0")/.."
OUT=testdata/voice16k
mkdir -p "$OUT"
find testdata/raw/librispeech -name '*.flac' | sort | while read -r f; do
  spk=$(basename "$f" | cut -d- -f1)
  mkdir -p "$OUT/$spk"
  echo "$f $OUT/$spk/$(basename "$f" .flac).wav"
done | xargs -P 8 -n 2 sh -c '[ -f "$1" ] || ffmpeg -v error -i "$0" -ac 1 -ar 16000 "$1"'
echo "speakers: $(ls $OUT | wc -l), files: $(find $OUT -name '*.wav' | wc -l)"
