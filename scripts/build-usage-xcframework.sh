#!/usr/bin/env bash
# Build a universal XCFramework for jackin-usage-ffi (macOS aarch64 + x86_64).
# Does not sign or notarize — those steps need operator Developer ID credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/target/xcframework}"
FRAMEWORK_NAME="JackinUsageFFI"
XCFRAMEWORK="$OUT_DIR/$FRAMEWORK_NAME.xcframework"

cd "$ROOT"

echo "==> building staticlibs for apple-darwin targets"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
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
MODULEMAP="$ROOT/native/Generated/jackin_usage_ffiFFI.modulemap"
if [[ ! -f "$HEADER" ]]; then
  HEADER="$(find "$ROOT/native/Generated" -name '*.h' | head -n1)"
fi
if [[ ! -f "$HEADER" ]]; then
  echo "error: no generated header under native/Generated" >&2
  exit 1
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/macos-universal"

FRAMEWORK="$OUT_DIR/macos-universal/$FRAMEWORK_NAME.framework"
mkdir -p "$FRAMEWORK/Headers" "$FRAMEWORK/Modules"
lipo -create "$ARM_LIB" "$X86_LIB" -output "$FRAMEWORK/$FRAMEWORK_NAME"
cp "$HEADER" "$FRAMEWORK/Headers/"
if [[ -f "$MODULEMAP" ]]; then
  cp "$MODULEMAP" "$FRAMEWORK/Modules/module.modulemap"
else
  cat >"$FRAMEWORK/Modules/module.modulemap" <<EOF
framework module $FRAMEWORK_NAME {
  umbrella header "$(basename "$HEADER")"
  export *
  module * { export * }
}
EOF
fi

echo "==> creating XCFramework"
xcodebuild -create-xcframework \
  -framework "$FRAMEWORK" \
  -output "$XCFRAMEWORK"

echo "==> XCFramework ready: $XCFRAMEWORK"
ls -la "$XCFRAMEWORK"
