#!/usr/bin/env bash
# Build a local LSUIElement menu-bar app linking the cargo release dylib.
# Signing/notarization is operator-gated (see docs).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/native/dist/JackinUsageMenuBar.app"
PROFILE="${PROFILE:-release}"

cd "$ROOT"
cargo build -p jackin-usage-ffi --"$PROFILE"
PROFILE="$PROFILE" bash "$ROOT/scripts/generate-usage-swift-bindings.sh"

cd "$ROOT/native"
swift build -c release --product JackinUsageMenuBar

BIN="$(swift build -c release --show-bin-path)/JackinUsageMenuBar"
rm -rf "$DIST"
mkdir -p "$DIST/Contents/MacOS" "$DIST/Contents/Frameworks"
cp "$BIN" "$DIST/Contents/MacOS/JackinUsageMenuBar"
cp "$ROOT/target/$PROFILE/libjackin_usage_ffi.dylib" "$DIST/Contents/Frameworks/"

cat >"$DIST/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>JackinUsageMenuBar</string>
  <key>CFBundleIdentifier</key>
  <string>com.jackin-project.usage-menu-bar</string>
  <key>CFBundleName</key>
  <string>jackin usage</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSUIElement</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
PLIST

echo "==> app ready: $DIST"
echo "Run with: open $DIST"
echo "Or: DYLD_LIBRARY_PATH=$ROOT/target/$PROFILE $BIN"
