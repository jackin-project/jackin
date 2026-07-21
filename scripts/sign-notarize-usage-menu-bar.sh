#!/usr/bin/env bash
# Developer ID sign + notarize + staple for an already-complete static
# JackinUsageMenuBar.app, then write the post-staple release ZIP.
#
# Local (keychain profile):
#   DEVELOPER_ID_APPLICATION='Developer ID Application: … (TEAMID)' \
#   NOTARY_PROFILE=jackin-notary \
#   JACKIN_APP_VERSION=X.Y.Z JACKIN_APP_BUILD=N \
#   ./scripts/sign-notarize-usage-menu-bar.sh [app] [out.zip]
#
# Direct App Store Connect API key (CI / non-interactive):
#   DEVELOPER_ID_APPLICATION=… \
#   APP_STORE_CONNECT_API_KEY_PATH=/path/AuthKey_XXX.p8 \
#   APP_STORE_CONNECT_KEY_ID=… \
#   APP_STORE_CONNECT_ISSUER_ID=… \   # required for team keys
#   EXPECTED_CERT_SHA256=… \         # optional fail-closed fingerprint
#   EXPECTED_TEAM_ID=… \             # optional fail-closed Team ID
#   ./scripts/sign-notarize-usage-menu-bar.sh [app] [out.zip]
#
# Never prints secret values. Does not use codesign --deep.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-$ROOT/native/dist/JackinUsageMenuBar.app}"
IDENTITY="${DEVELOPER_ID_APPLICATION:-}"
PROFILE="${NOTARY_PROFILE:-}"
VERSION="${JACKIN_APP_VERSION:-}"
BUILD="${JACKIN_APP_BUILD:-}"
OUT_ZIP="${2:-}"
NOTARY_LOG_DIR="${NOTARY_LOG_DIR:-${RUNNER_TEMP:-/tmp}/jackin-notary}"

fail() { echo "error: $*" >&2; exit 1; }

if [[ ! -d "$APP" ]]; then
  fail "app not found at $APP — run scripts/build-usage-menu-bar-app.sh first"
fi
if [[ -z "$IDENTITY" ]]; then
  fail "set DEVELOPER_ID_APPLICATION to the Developer ID Application identity"
fi
if [[ -z "$VERSION" || -z "$BUILD" ]]; then
  fail "JACKIN_APP_VERSION and JACKIN_APP_BUILD are required for the final ZIP name"
fi
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  fail "JACKIN_APP_VERSION must be stable X.Y.Z (got $VERSION)"
fi
if ! [[ "$BUILD" =~ ^[0-9]+$ ]]; then
  fail "JACKIN_APP_BUILD must be numeric (got $BUILD)"
fi

# Reject leftover dylib packaging (static app only).
if find "$APP" -type f \( -name '*.dylib' -o -name '*.a' \) | grep -q .; then
  fail "app still embeds dylib/static archive — refuse to sign"
fi

# Sign the app bundle only (static universal executable; no nested frameworks).
echo "==> codesign (hardened runtime, secure timestamp, no --deep)"
codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

# Optional fail-closed fingerprint / team checks after signing.
if [[ -n "${EXPECTED_CERT_SHA256:-}" ]]; then
  cert_prefix="$(mktemp -d)/codesign-cert"
  codesign -d --extract-certificates="$cert_prefix" "$APP" 2>/dev/null \
    || fail "could not extract signing certificate"
  cert_file="${cert_prefix}0"
  [[ -f "$cert_file" ]] || fail "missing extracted leaf certificate"
  cert_hash="$(shasum -a 256 "$cert_file" | awk '{print tolower($1)}')"
  rm -rf "$(dirname "$cert_prefix")"
  expected="$(printf '%s' "$EXPECTED_CERT_SHA256" | tr '[:upper:]' '[:lower:]' | tr -d ':')"
  [[ "$cert_hash" == "$expected" ]] || fail "certificate SHA-256 mismatch (expected configured fingerprint)"
fi
if [[ -n "${EXPECTED_TEAM_ID:-}" ]]; then
  team="$(codesign -dv --verbose=4 "$APP" 2>&1 | awk -F= '/TeamIdentifier/ {print $2; exit}')"
  [[ "$team" == "$EXPECTED_TEAM_ID" ]] || fail "TeamIdentifier mismatch (got ${team:-empty})"
fi

# Forbidden entitlements (get-task-allow etc.).
ents="$(codesign -d --entitlements :- "$APP" 2>/dev/null || true)"
if printf '%s' "$ents" | grep -q 'get-task-allow'; then
  fail "forbidden entitlement get-task-allow present"
fi

mkdir -p "$NOTARY_LOG_DIR"
SUBMIT_ZIP="$NOTARY_LOG_DIR/submit-JackinUsageMenuBar.zip"
rm -f "$SUBMIT_ZIP"
echo "==> submission zip (disposable)"
ditto -c -k --keepParent "$APP" "$SUBMIT_ZIP"

echo "==> notarytool submit"
NOTARY_JSON="$NOTARY_LOG_DIR/notary-submit.json"
if [[ -n "${APP_STORE_CONNECT_API_KEY_PATH:-}" ]]; then
  [[ -f "$APP_STORE_CONNECT_API_KEY_PATH" ]] || fail "APP_STORE_CONNECT_API_KEY_PATH not a file"
  [[ -n "${APP_STORE_CONNECT_KEY_ID:-}" ]] || fail "APP_STORE_CONNECT_KEY_ID required with API key path"
  [[ -n "${APP_STORE_CONNECT_ISSUER_ID:-}" ]] || fail "APP_STORE_CONNECT_ISSUER_ID required for team API keys"
  xcrun notarytool submit "$SUBMIT_ZIP" \
    --key "$APP_STORE_CONNECT_API_KEY_PATH" \
    --key-id "$APP_STORE_CONNECT_KEY_ID" \
    --issuer "$APP_STORE_CONNECT_ISSUER_ID" \
    --wait --output-format json | tee "$NOTARY_JSON"
elif [[ -n "$PROFILE" ]]; then
  xcrun notarytool submit "$SUBMIT_ZIP" \
    --keychain-profile "$PROFILE" \
    --wait --output-format json | tee "$NOTARY_JSON"
else
  fail "set NOTARY_PROFILE or APP_STORE_CONNECT_API_KEY_PATH + KEY_ID + ISSUER_ID"
fi

status="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("status",""))' "$NOTARY_JSON" 2>/dev/null || true)"
if [[ -z "$status" ]]; then
  # Fallback: last line JSON blob
  status="$(tail -1 "$NOTARY_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))' 2>/dev/null || true)"
fi
[[ "$status" == "Accepted" ]] || fail "notarytool status was '${status:-unknown}', required Accepted (log: $NOTARY_JSON)"

echo "==> staple + validate"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
spctl --assess --type execute --verbose=4 "$APP" || fail "Gatekeeper assessment failed"

echo "==> Plan 001 release-mode verifier"
JACKIN_APP_VERSION="$VERSION" JACKIN_APP_BUILD="$BUILD" RELEASE_MODE=1 \
  bash "$ROOT/scripts/verify-usage-menu-bar-app.sh" "$APP"

if [[ -z "$OUT_ZIP" ]]; then
  OUT_ZIP="$ROOT/native/dist/jackin-usage-menu-bar-${VERSION}-universal-apple-darwin.zip"
fi
rm -f "$OUT_ZIP"
echo "==> final post-staple ZIP: $OUT_ZIP"
# ZIP contains exactly JackinUsageMenuBar.app at top level.
(
  cd "$(dirname "$APP")"
  ditto -c -k --keepParent "$(basename "$APP")" "$OUT_ZIP"
)

JACKIN_APP_VERSION="$VERSION" JACKIN_APP_BUILD="$BUILD" RELEASE_MODE=1 \
  bash "$ROOT/scripts/verify-usage-menu-bar-app.sh" "$APP" "$OUT_ZIP"

# Drop disposable submission zip (final ZIP is the release artifact).
rm -f "$SUBMIT_ZIP"
echo "==> signed, notarized, stapled: $APP"
echo "==> release zip: $OUT_ZIP"
