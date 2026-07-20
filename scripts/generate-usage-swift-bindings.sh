#!/usr/bin/env bash
# Generate UniFFI Swift bindings from the built jackin_usage_ffi library.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/native/Generated}"
PROFILE="${PROFILE:-release}"
LIB="$ROOT/target/$PROFILE/libjackin_usage_ffi.dylib"

cd "$ROOT"

echo "==> building jackin-usage-ffi ($PROFILE)"
cargo build -p jackin-usage-ffi --"$PROFILE"

if [[ ! -f "$LIB" ]]; then
  echo "error: expected library at $LIB" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

echo "==> generating Swift bindings into $OUT_DIR"
if ! command -v uniffi-bindgen >/dev/null 2>&1; then
  echo "==> installing uniffi-bindgen 0.32.0"
  cargo install uniffi-bindgen --version 0.32.0 --locked
fi
uniffi-bindgen generate --library "$LIB" --language swift --out-dir "$OUT_DIR"

SOURCES_SWIFT="$ROOT/native/Sources/JackinUsageBridge"
mkdir -p "$SOURCES_SWIFT"
if [[ -f "$OUT_DIR/jackin_usage_ffi.swift" ]]; then
  cp "$OUT_DIR/jackin_usage_ffi.swift" "$SOURCES_SWIFT/jackin_usage_ffi.swift"
fi
if [[ -f "$OUT_DIR/jackin_usage_ffiFFI.modulemap" && ! -f "$OUT_DIR/module.modulemap" ]]; then
  cat >"$OUT_DIR/module.modulemap" <<'EOF'
module jackin_usage_ffiFFI {
    header "jackin_usage_ffiFFI.h"
    export *
}
EOF
fi

echo "==> generated:"
ls -la "$OUT_DIR"
