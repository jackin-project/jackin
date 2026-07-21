#!/usr/bin/env bash
# Fail-closed validation for JackinUsageMenuBar.app (PR ad-hoc or release mode).
# Env: JACKIN_APP_VERSION, JACKIN_APP_BUILD (required). Optional ZIP path as $2.
# RELEASE_MODE=1 enables Gatekeeper/stapler expectations (Developer ID + notarized).
set -euo pipefail

APP="${1:-}"
ZIP="${2:-}"
RELEASE_MODE="${RELEASE_MODE:-0}"

if [[ -z "$APP" || ! -d "$APP" ]]; then
  echo "usage: $0 <JackinUsageMenuBar.app> [archive.zip]" >&2
  exit 2
fi
if [[ -z "${JACKIN_APP_VERSION:-}" || -z "${JACKIN_APP_BUILD:-}" ]]; then
  echo "error: JACKIN_APP_VERSION and JACKIN_APP_BUILD are required" >&2
  exit 1
fi

BIN="$APP/Contents/MacOS/JackinUsageMenuBar"
PLIST="$APP/Contents/Info.plist"

fail() { echo "error: $*" >&2; exit 1; }

[[ -f "$BIN" ]] || fail "missing executable $BIN"
[[ -f "$PLIST" ]] || fail "missing $PLIST"

# Exact plist fields.
bid="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST")"
[[ "$bid" == "com.jackin-project.usage-menu-bar" ]] || fail "bundle id $bid"
exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$PLIST")"
[[ "$exe" == "JackinUsageMenuBar" ]] || fail "executable name $exe"
ver="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")"
[[ "$ver" == "$JACKIN_APP_VERSION" ]] || fail "version $ver != $JACKIN_APP_VERSION"
build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST")"
[[ "$build" == "$JACKIN_APP_BUILD" ]] || fail "build $build != $JACKIN_APP_BUILD"
min_os="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$PLIST")"
[[ "$min_os" == "14.0" ]] || fail "LSMinimumSystemVersion $min_os"
lsui="$(/usr/libexec/PlistBuddy -c 'Print :LSUIElement' "$PLIST")"
[[ "$lsui" == "true" ]] || fail "LSUIElement must be true"

ARCHS="$(lipo -archs "$BIN")"
echo "$ARCHS" | grep -qw arm64 || fail "missing arm64 (got $ARCHS)"
echo "$ARCHS" | grep -qw x86_64 || fail "missing x86_64 (got $ARCHS)"

# Per-slice min OS when vtool reports it (normalized 14.0; never newer than plist floor).
if command -v vtool >/dev/null 2>&1; then
  for arch in arm64 x86_64; do
    info="$(vtool -arch "$arch" -show-build "$BIN" 2>/dev/null || true)"
    if echo "$info" | grep -qi 'minos'; then
      minos="$(echo "$info" | awk 'tolower($1) ~ /minos/ {print $NF; exit}')"
      if [[ -n "$minos" && "$minos" != "14.0" && "$minos" != "14.0.0" ]]; then
        # Allow older floors; reject newer than 14.0.
        if awk -v m="$minos" 'BEGIN { split(m,a,"."); v=a[1]+a[2]/100; exit !(v>14.0) }'; then
          fail "slice $arch minos $minos newer than 14.0"
        fi
      fi
    fi
  done
fi

if find "$APP" -type f \( -name '*.dylib' -o -name '*.a' \) | grep -q .; then
  fail "app embeds dylib or static archive"
fi
if find "$APP" \( -name '*.framework' -o -name '*.xcframework' \) | grep -q .; then
  fail "app embeds framework or XCFramework"
fi
if otool -L "$BIN" | grep -E $'^\t' | grep -E 'libjackin_usage_ffi|/Users/|/home/|target/'; then
  fail "absolute or FFI dylib linkage remains"
fi

codesign --verify --deep --strict "$APP" || fail "codesign verify failed"

if [[ "$RELEASE_MODE" == "1" ]]; then
  spctl --assess --type execute "$APP" || fail "spctl assess failed"
  xcrun stapler validate "$APP" || fail "stapler validate failed"
fi

if [[ -n "$ZIP" ]]; then
  [[ -f "$ZIP" ]] || fail "zip not found: $ZIP"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  unzip -q "$ZIP" -d "$tmp"
  count="$(find "$tmp" -name 'JackinUsageMenuBar.app' -type d | wc -l | tr -d ' ')"
  [[ "$count" == "1" ]] || fail "archive must contain exactly one JackinUsageMenuBar.app (found $count)"
  nested="$(find "$tmp" -name 'JackinUsageMenuBar.app' -type d | head -1)"
  # Recurse without zip.
  JACKIN_APP_VERSION="$JACKIN_APP_VERSION" JACKIN_APP_BUILD="$JACKIN_APP_BUILD" \
    RELEASE_MODE="$RELEASE_MODE" \
    bash "$0" "$nested"
fi

echo "OK: verified $APP"
