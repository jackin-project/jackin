#!/usr/bin/env bash
# Build a universal static XCFramework for jackin-usage-ffi (macOS arm64 + x86_64).
# Clang module name is exactly jackin_usage_ffiFFI (matches generated UniFFI Swift).
# Assembles the XCFramework directory by hand so Command Line Tools hosts work;
# layout matches xcodebuild -create-xcframework -library output.
# Does not sign or notarize.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/target/xcframework}"
FRAMEWORK_NAME="JackinUsageFFI"
XCFRAMEWORK="$OUT_DIR/$FRAMEWORK_NAME.xcframework"
MODULE_NAME="jackin_usage_ffiFFI"

cd "$ROOT"

echo "==> building staticlibs for apple-darwin targets (macOS 14 floor)"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build -p jackin-usage-ffi --release --target aarch64-apple-darwin
cargo build -p jackin-usage-ffi --release --target x86_64-apple-darwin

ARM_LIB="$ROOT/target/aarch64-apple-darwin/release/libjackin_usage_ffi.a"
X86_LIB="$ROOT/target/x86_64-apple-darwin/release/libjackin_usage_ffi.a"

for lib in "$ARM_LIB" "$X86_LIB"; do
  if [[ ! -f "$lib" ]]; then
    echo "error: missing $lib" >&2
    exit 1
  fi
done

echo "==> generating Swift bindings"
cargo build -p jackin-usage-ffi --release
PROFILE=release OUT_DIR="$ROOT/native/Generated" \
  bash "$ROOT/scripts/generate-usage-swift-bindings.sh"

HEADER="$ROOT/native/Generated/jackin_usage_ffiFFI.h"
if [[ ! -f "$HEADER" ]]; then
  HEADER="$(find "$ROOT/native/Generated" -name '*.h' | head -n1)"
fi
if [[ ! -f "$HEADER" ]]; then
  echo "error: no generated header under native/Generated" >&2
  exit 1
fi

rm -rf "$OUT_DIR"
mkdir -p "$XCFRAMEWORK"

install_slice() {
  local arch="$1"
  local lib="$2"
  local id="macos-${arch}"
  local slice="$XCFRAMEWORK/$id"
  mkdir -p "$slice/Headers"
  cp "$lib" "$slice/libjackin_usage_ffi.a"
  cp "$HEADER" "$slice/Headers/jackin_usage_ffiFFI.h"
  cat >"$slice/Headers/module.modulemap" <<EOF
module ${MODULE_NAME} {
  header "jackin_usage_ffiFFI.h"
  export *
}
EOF
  local archs
  archs="$(lipo -archs "$slice/libjackin_usage_ffi.a")"
  echo "  slice $id: $archs"
  echo "$archs" | grep -qw "$arch" || {
    echo "error: $id library missing $arch (got $archs)" >&2
    exit 1
  }
}

echo "==> assembling static XCFramework (${MODULE_NAME})"
install_slice arm64 "$ARM_LIB"
install_slice x86_64 "$X86_LIB"

cat >"$XCFRAMEWORK/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AvailableLibraries</key>
  <array>
    <dict>
      <key>LibraryIdentifier</key>
      <string>macos-arm64</string>
      <key>LibraryPath</key>
      <string>libjackin_usage_ffi.a</string>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>SupportedArchitectures</key>
      <array>
        <string>arm64</string>
      </array>
      <key>SupportedPlatform</key>
      <string>macos</string>
    </dict>
    <dict>
      <key>LibraryIdentifier</key>
      <string>macos-x86_64</string>
      <key>LibraryPath</key>
      <string>libjackin_usage_ffi.a</string>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>SupportedArchitectures</key>
      <array>
        <string>x86_64</string>
      </array>
      <key>SupportedPlatform</key>
      <string>macos</string>
    </dict>
  </array>
  <key>CFBundlePackageType</key>
  <string>XFWK</string>
  <key>XCFrameworkFormatVersion</key>
  <string>1.0</string>
</dict>
</plist>
PLIST

if command -v plutil >/dev/null 2>&1; then
  plutil -lint "$XCFRAMEWORK/Info.plist" >/dev/null
fi

LIBS=()
while IFS= read -r -d '' lib; do
  LIBS+=("$lib")
done < <(find "$XCFRAMEWORK" -type f -name 'libjackin_usage_ffi.a' -print0)
if [[ ${#LIBS[@]} -lt 2 ]]; then
  echo "error: expected arm64 and x86_64 static libraries inside XCFramework, found ${#LIBS[@]}" >&2
  exit 1
fi

echo "==> XCFramework ready: $XCFRAMEWORK"
ls -la "$XCFRAMEWORK"
