#!/usr/bin/env bash
# Assemble one universal, statically linked JackinUsageMenuBar.app from the
# static XCFramework path. No dylib / framework / XCFramework is embedded.
# Requires: JACKIN_APP_VERSION, JACKIN_APP_BUILD (numeric). Ad-hoc signs after assembly.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/native/dist/JackinUsageMenuBar.app"
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

echo "==> XCFramework (static universal)"
bash "$ROOT/scripts/build-usage-xcframework.sh"
if [[ ! -d "$XCFRAMEWORK" ]]; then
  echo "error: missing $XCFRAMEWORK" >&2
  exit 1
fi

cd "$ROOT/native"

build_slice() {
  local arch="$1"
  local out_dir="$2"
  echo "==> swift build ($arch)"
  # SwiftPM resolves the binaryTarget XCFramework; link is static — no dylib in product.
  swift build -c release --product JackinUsageMenuBar --arch "$arch" \
    -Xswiftc -target -Xswiftc "${arch}-apple-macosx14.0"
  local bin
  bin="$(swift build -c release --show-bin-path --arch "$arch")/JackinUsageMenuBar"
  if [[ ! -f "$bin" ]]; then
    # Fallback path when --arch does not change show-bin-path layout.
    bin="$(swift build -c release --show-bin-path)/JackinUsageMenuBar"
  fi
  if [[ ! -f "$bin" ]]; then
    echo "error: missing Swift product for $arch" >&2
    exit 1
  fi
  mkdir -p "$out_dir"
  cp "$bin" "$out_dir/JackinUsageMenuBar"
  # Confirm slice architecture.
  local got
  got="$(lipo -archs "$out_dir/JackinUsageMenuBar")"
  if ! echo "$got" | grep -qw "$arch"; then
    echo "error: expected $arch in $out_dir/JackinUsageMenuBar, got: $got" >&2
    exit 1
  fi
}

SLICE_ROOT="$ROOT/native/.build/universal-slices"
rm -rf "$SLICE_ROOT"
build_slice arm64 "$SLICE_ROOT/arm64"
build_slice x86_64 "$SLICE_ROOT/x86_64"

rm -rf "$DIST"
mkdir -p "$DIST/Contents/MacOS"
echo "==> lipo universal executable"
lipo -create \
  "$SLICE_ROOT/arm64/JackinUsageMenuBar" \
  "$SLICE_ROOT/x86_64/JackinUsageMenuBar" \
  -output "$DIST/Contents/MacOS/JackinUsageMenuBar"
chmod +x "$DIST/Contents/MacOS/JackinUsageMenuBar"

ARCHS="$(lipo -archs "$DIST/Contents/MacOS/JackinUsageMenuBar")"
echo "  universal archs: $ARCHS"
echo "$ARCHS" | grep -qw arm64
echo "$ARCHS" | grep -qw x86_64

# Plist before any signing.
cat >"$DIST/Contents/Info.plist" <<PLIST
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
if otool -L "$DIST/Contents/MacOS/JackinUsageMenuBar" | grep -E $'^\t' | grep -E 'libjackin_usage_ffi|/Users/|/home/|target/'; then
  echo "error: executable still links absolute or FFI dylib path:" >&2
  otool -L "$DIST/Contents/MacOS/JackinUsageMenuBar" >&2
  exit 1
fi

echo "==> ad-hoc codesign (local/PR shape)"
codesign --force --sign - --timestamp=none "$DIST"

echo "==> app ready: $DIST"
echo "Universal static: no embedded libjackin_usage_ffi.dylib"
echo "Run with: open $DIST"
