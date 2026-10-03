#!/bin/sh
# Build "Buu's Fury.app" (unsigned, ad-hoc signed for Apple silicon) into engine/dist/.
#   engine/package/mac-app.sh            release build + bundle
#   SKIP_BUILD=1 engine/package/mac-app.sh   bundle the existing target/release/buus-fury
# The app ships no game data: on first launch it asks for your own Buu's Fury (USA) ROM and extracts it into
# ~/Library/Application Support/wf-buu/ (mods: .../wf-buu/content/mods/, also reachable from the launcher's Mods page).
set -e
ENGINE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ENGINE"
NAME="Buu's Fury"
APP="dist/$NAME.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

if [ -z "$SKIP_BUILD" ]; then
  cargo build --release -p wf-native
fi
BIN=target/release/buus-fury
[ -x "$BIN" ] || { echo "missing $BIN" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/buus-fury"

# icon (generic art drawn by make_icon.py) -> .icns
ICONSET="$(mktemp -d)/icon.iconset"
mkdir -p "$ICONSET"
python3 package/make_icon.py "$ICONSET/base.png" >/dev/null
for s in 16 32 128 256 512; do
  sips -z $s $s "$ICONSET/base.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d "$ICONSET/base.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
rm "$ICONSET/base.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>dev.wf-buu.buus-fury</string>
  <key>CFBundleExecutable</key><string>buus-fury</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.action-games</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSEnvironment</key><dict><key>WF_WINDOW</key><string>1</string></dict>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>Game Boy Advance ROM</string>
    <key>CFBundleTypeExtensions</key><array><string>gba</string></array>
    <key>CFBundleTypeRole</key><string>Viewer</string>
  </dict></array>
</dict>
</plist>
EOF

# Apple silicon refuses unsigned binaries: an ad-hoc signature (no identity) is enough to run locally
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || echo "warning: codesign failed (the app may not start on Apple silicon)" >&2

echo "built $ENGINE/$APP"
echo "first open: right-click the app > Open (or: xattr -dr com.apple.quarantine \"$APP\")"
