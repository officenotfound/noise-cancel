#!/bin/sh
# Assemble dist/QuietMic: the Windows app, CLI tools, models and a README. Run after a release build + model fetch.
set -eu
cd "$(dirname "$0")/.."
D=dist/QuietMic
rm -rf "$D" && mkdir -p "$D/models"
for b in QuietMic qm qm-bench; do cp "target/release/$b.exe" "$D/"; done
cp models/*.tar.gz "$D/models/"
cp -R models/fastenhancer models/dpdfnet "$D/models/"
[ -f target/release/QuietMic-Uninstall.exe ] && cp target/release/QuietMic-Uninstall.exe "$D/Uninstall.exe"
cp docs/WINDOWS_README.txt "$D/README.txt"
cp scripts/windows/*.bat "$D/"
echo "packaged $D ($(du -sh "$D" | cut -f1))"
