#!/usr/bin/env bash
# Operator-gated Developer ID sign + notarize + staple for JackinUsageMenuBar.app.
# Requires: full Xcode, Developer ID Application identity, notarytool credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-$ROOT/native/dist/JackinUsageMenuBar.app}"
IDENTITY="${DEVELOPER_ID_APPLICATION:-}"
PROFILE="${NOTARY_PROFILE:-jackin-notary}"

if [[ ! -d "$APP" ]]; then
  echo "error: app not found at $APP — run scripts/build-usage-menu-bar-app.sh first" >&2
  exit 1
fi
if [[ -z "$IDENTITY" ]]; then
  echo "error: set DEVELOPER_ID_APPLICATION to your Developer ID Application identity" >&2
  echo "example: export DEVELOPER_ID_APPLICATION='Developer ID Application: Example (TEAMID)'" >&2
  exit 1
fi

echo "==> codesign (hardened runtime)"
codesign --force --deep --options runtime --sign "$IDENTITY" \
  "$APP/Contents/Frameworks/libjackin_usage_ffi.dylib" || true
codesign --force --deep --options runtime --sign "$IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

echo "==> notarize (notarytool profile: $PROFILE)"
ZIP="${APP}.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$APP"
echo "==> stapled: $APP"
