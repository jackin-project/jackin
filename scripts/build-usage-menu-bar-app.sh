#!/usr/bin/env bash
# Build a local LSUIElement menu-bar app linking the cargo release dylib.
# Rewrites install names so the .app is relocatable (Frameworks via @rpath).
# Signing/notarization is operator-gated (see docs).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/native/dist/JackinUsageMenuBar.app"
PROFILE="${PROFILE:-release}"
LIB_NAME="libjackin_usage_ffi.dylib"

cd "$ROOT"
cargo build -p jackin-usage-ffi --"$PROFILE"
PROFILE="$PROFILE" bash "$ROOT/scripts/generate-usage-swift-bindings.sh"

cd "$ROOT/native"
swift build -c release --product JackinUsageMenuBar

BIN="$(swift build -c release --show-bin-path)/JackinUsageMenuBar"
rm -rf "$DIST"
mkdir -p "$DIST/Contents/MacOS" "$DIST/Contents/Frameworks"
cp "$BIN" "$DIST/Contents/MacOS/JackinUsageMenuBar"
cp "$ROOT/target/$PROFILE/$LIB_NAME" "$DIST/Contents/Frameworks/$LIB_NAME"

APP_BIN="$DIST/Contents/MacOS/JackinUsageMenuBar"
APP_LIB="$DIST/Contents/Frameworks/$LIB_NAME"

# Self-contained dylib: id + binary dependency use @rpath (not absolute cargo path).
install_name_tool -id "@rpath/$LIB_NAME" "$APP_LIB"

# Binary may link either target/release/lib… or target/release/deps/lib….
OLD_REF="$(otool -L "$APP_BIN" | awk '/libjackin_usage_ffi\.dylib/{print $1; exit}')"
if [[ -z "${OLD_REF:-}" ]]; then
  echo "error: $APP_BIN has no libjackin_usage_ffi.dylib dependency" >&2
  otool -L "$APP_BIN" >&2
  exit 1
fi
install_name_tool -change "$OLD_REF" "@rpath/$LIB_NAME" "$APP_BIN"

# Add rpath if missing.
if ! otool -l "$APP_BIN" | grep -q '@executable_path/../Frameworks'; then
  install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP_BIN"
fi

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

echo "==> linkage check (must be @rpath, not absolute cargo path)"
otool -L "$APP_BIN" | head -5
if otool -L "$APP_BIN" | grep -q 'libjackin_usage_ffi.dylib'; then
  if otool -L "$APP_BIN" | grep 'libjackin_usage_ffi' | grep -q '@rpath/'; then
    echo "OK: binary uses @rpath/$LIB_NAME"
  else
    echo "error: binary still links absolute path:" >&2
    otool -L "$APP_BIN" | grep libjackin_usage_ffi >&2
    exit 1
  fi
fi

echo "==> app ready: $DIST"
echo "Relocatable: copy the .app anywhere; Frameworks embeds $LIB_NAME"
echo "Run with: open $DIST"
