#!/bin/sh
# Build dist/QuietMic.app and dist/QuietMic-macOS.dmg (Apple Silicon, or whatever `cargo` builds for this Mac).
# Needs: cargo, the models (scripts/fetch_models*.sh), and the macOS tools sips / iconutil / codesign / hdiutil.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p qm-app
APP=dist/QuietMic.app
rm -rf "$APP" dist/dmg dist/QuietMic-macOS.dmg dist/AppIcon.iconset
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/models"
cp target/release/QuietMic "$APP/Contents/MacOS/QuietMic"
cp models/*.tar.gz "$APP/Contents/Resources/models/"
cp -R models/fastenhancer models/dpdfnet "$APP/Contents/Resources/models/"
rm -f "$APP"/Contents/Resources/models/*/SHA256SUMS "$APP"/Contents/Resources/models/SHA256SUMS

# icon
python3 scripts/make_icon.py dist/icon-1024.png
mkdir -p dist/AppIcon.iconset
for s in 16 32 128 256 512; do
  sips -z $s $s dist/icon-1024.png --out dist/AppIcon.iconset/icon_${s}x${s}.png >/dev/null
  sips -z $((s*2)) $((s*2)) dist/icon-1024.png --out dist/AppIcon.iconset/icon_${s}x${s}@2x.png >/dev/null
done
iconutil -c icns dist/AppIcon.iconset -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf dist/AppIcon.iconset dist/icon-1024.png

VERSION=$(grep -m1 '^version' crates/qm-app/Cargo.toml | cut -d'"' -f2)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>QuietMic</string>
  <key>CFBundleDisplayName</key><string>QuietMic</string>
  <key>CFBundleIdentifier</key><string>com.quietmic.app</string>
  <key>CFBundleExecutable</key><string>QuietMic</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSMicrophoneUsageDescription</key><string>QuietMic listens to your microphone so it can remove background noise before other apps hear you.</string>
</dict></plist>
PLIST

# ad-hoc signature so macOS can attach the microphone permission to this app
codesign --force --deep --sign - "$APP"

# disk image with an Applications shortcut and a short readme
mkdir -p dist/dmg
cp -R "$APP" dist/dmg/
ln -s /Applications dist/dmg/Applications
cp docs/MACOS_README.txt dist/dmg/README.txt
hdiutil create -volname "QuietMic" -srcfolder dist/dmg -ov -format UDZO dist/QuietMic-macOS.dmg >/dev/null
rm -rf dist/dmg
echo "built $APP ($(du -sh "$APP" | cut -f1)) and dist/QuietMic-macOS.dmg ($(du -sh dist/QuietMic-macOS.dmg | cut -f1))"
