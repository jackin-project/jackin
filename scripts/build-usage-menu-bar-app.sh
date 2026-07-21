#!/usr/bin/env bash
# Assemble one arm64 (Apple Silicon) statically linked JackinDesktop.app
# from the static XCFramework path. No dylib / framework / XCFramework is embedded.
# Requires: JACKIN_APP_VERSION, JACKIN_APP_BUILD (numeric). Ad-hoc signs after assembly.
# Intel/x86_64 is out of scope for now (operator decision 2026-07-22).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/native/dist/JackinDesktop.app"
XCFRAMEWORK="$ROOT/target/xcframework/JackinUsageFFI.xcframework"

if [[ -z "${JACKIN_APP_VERSION:-}" ]]; then
  echo "error: JACKIN_APP_VERSION is required (e.g. 0.6.0)" >&2
  exit 1
fi
if [[ -z "${JACKIN_APP_BUILD:-}" ]]; then
  echo "error: JACKIN_APP_BUILD is required (numeric CFBundleVersion, e.g. 1)" >&2
  exit 1
fi
if ! [[ "$JACKIN_APP_VERSION" =~ ^[0-9]+(\.[0-9]+)*$ ]]; then
  echo "error: JACKIN_APP_VERSION must be numeric dotted (got $JACKIN_APP_VERSION)" >&2
  exit 1
fi
if ! [[ "$JACKIN_APP_BUILD" =~ ^[0-9]+$ ]]; then
  echo "error: JACKIN_APP_BUILD must be numeric (got $JACKIN_APP_BUILD)" >&2
  exit 1
fi

cd "$ROOT"

echo "==> XCFramework (static arm64)"
bash "$ROOT/scripts/build-usage-xcframework.sh"
if [[ ! -d "$XCFRAMEWORK" ]]; then
  echo "error: missing $XCFRAMEWORK" >&2
  exit 1
fi

cd "$ROOT/native"

ARCH=arm64
echo "==> swift build ($ARCH)"
# SwiftPM resolves the binaryTarget XCFramework; link is static — no dylib in product.
swift build -c release --product JackinDesktop --arch "$ARCH" \
  -Xswiftc -target -Xswiftc "${ARCH}-apple-macosx14.0"
BIN_DIR="$(swift build -c release --show-bin-path --arch "$ARCH")"
bin="$BIN_DIR/JackinDesktop"
if [[ ! -f "$bin" ]]; then
  BIN_DIR="$(swift build -c release --show-bin-path)"
  bin="$BIN_DIR/JackinDesktop"
fi
if [[ ! -f "$bin" ]]; then
  echo "error: missing Swift product for $ARCH" >&2
  exit 1
fi

got="$(lipo -archs "$bin")"
if ! echo "$got" | grep -qw "$ARCH"; then
  echo "error: expected $ARCH in $bin, got: $got" >&2
  exit 1
fi
if echo "$got" | grep -qw x86_64; then
  echo "error: unexpected x86_64 slice in arm64-only build: $got" >&2
  exit 1
fi

rm -rf "$DIST"
mkdir -p "$DIST/Contents/MacOS" "$DIST/Contents/Resources"
cp "$bin" "$DIST/Contents/MacOS/JackinDesktop"
chmod +x "$DIST/Contents/MacOS/JackinDesktop"

# SwiftPM resource bundle for Bundle.module (logomark template image).
RESOURCE_BUNDLE=""
for candidate in \
  "$BIN_DIR/JackinDesktop_JackinDesktop.bundle" \
  "$BIN_DIR/JackinDesktop.bundle"; do
  if [[ -d "$candidate" ]]; then
    RESOURCE_BUNDLE="$candidate"
    break
  fi
done
if [[ -z "$RESOURCE_BUNDLE" ]]; then
  # Fall back to a shallow search under the bin dir.
  RESOURCE_BUNDLE="$(find "$BIN_DIR" -maxdepth 2 -type d -name 'JackinDesktop_JackinDesktop.bundle' 2>/dev/null | head -1 || true)"
fi
if [[ -z "$RESOURCE_BUNDLE" || ! -d "$RESOURCE_BUNDLE" ]]; then
  echo "error: missing SwiftPM resource bundle JackinDesktop_JackinDesktop.bundle under $BIN_DIR" >&2
  ls -la "$BIN_DIR" >&2 || true
  exit 1
fi
cp -R "$RESOURCE_BUNDLE" "$DIST/Contents/Resources/"

ARCHS="$(lipo -archs "$DIST/Contents/MacOS/JackinDesktop")"
echo "  executable archs: $ARCHS"
echo "$ARCHS" | grep -qw arm64
if echo "$ARCHS" | grep -qw x86_64; then
  echo "error: final app must be arm64-only (got $ARCHS)" >&2
  exit 1
fi

# Plist before any signing.
cat >"$DIST/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>JackinDesktop</string>
  <key>CFBundleIdentifier</key>
  <string>com.jackin-project.desktop</string>
  <key>CFBundleName</key>
  <string>Jackin Desktop</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${JACKIN_APP_VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${JACKIN_APP_BUILD}</string>
  <key>LSUIElement</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
PLIST

# Fail-closed: no embedded Rust dylib / static archive / framework / XCFramework.
if find "$DIST" -type f \( -name '*.dylib' -o -name '*.a' -o -name '*.framework' -o -name '*.xcframework' \) | grep -q .; then
  echo "error: app must not embed dylib/staticlib/framework/XCFramework:" >&2
  find "$DIST" -type f >&2
  exit 1
fi
# Only dependency lines (tab-indented); ignore the absolute path in the header.
if otool -L "$DIST/Contents/MacOS/JackinDesktop" | grep -E $'^\t' | grep -E 'libjackin_usage_ffi|/Users/|/home/|target/'; then
  echo "error: executable still links absolute or FFI dylib path:" >&2
  otool -L "$DIST/Contents/MacOS/JackinDesktop" >&2
  exit 1
fi

echo "==> ad-hoc codesign (local/PR shape)"
codesign --force --sign - --timestamp=none "$DIST"

echo "==> app ready: $DIST"
echo "Apple Silicon (arm64) static: no embedded libjackin_usage_ffi.dylib"
echo "Run with: open $DIST"
