#!/usr/bin/env bash
# jackin> Desktop clean-host install proof.
#
# Verifies a published jackin-desktop cask installs, is notarized + stapled +
# Gatekeeper-accepted, reports the expected version, and launches as a menu-bar
# (LSUIElement) app on a Mac that does NOT have this repo checked out. Copy this
# single file to the target host, or run the numbered manual fallback by hand.
#
# Usage: desktop-install-proof.sh <version> [--keep]
#   <version>  the released X.Y.Z (e.g. 1.2.0)
#   --keep     skip the uninstall/cleanup at the end
#
# Manual fallback (same steps, no script):
#   1. brew install --cask jackin-project/tap/jackin-desktop
#   2. test -d /Applications/JackinDesktop.app
#   3. spctl --assess --type execute --verbose=2 /Applications/JackinDesktop.app
#      (expect "accepted" + "source=Notarized Developer ID")
#   4. xcrun stapler validate /Applications/JackinDesktop.app
#   5. codesign --verify --deep --strict /Applications/JackinDesktop.app
#   6. /usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' \
#        /Applications/JackinDesktop.app/Contents/Info.plist   (equals <version>)
#   7. open -a /Applications/JackinDesktop.app; sleep 10; pgrep -x JackinDesktop
#   8. Confirm by eye: enabled providers appear in the menu bar with real data.
#   9. pkill -x JackinDesktop; brew uninstall --cask jackin-desktop
#
# This script never prints provider tokens, account values, or any credential
# material — only command exit statuses and the fixed PASS/FAIL/MANUAL strings.

set -euo pipefail

APP="/Applications/JackinDesktop.app"

usage() {
  echo "usage: $(basename "$0") <version> [--keep]" >&2
  echo "  <version>  released X.Y.Z (e.g. 1.2.0)" >&2
  echo "  --keep     skip uninstall/cleanup at the end" >&2
}

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

VERSION=""
KEEP=0
for arg in "$@"; do
  case "$arg" in
    --keep) KEEP=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    -*) fail "unknown option: $arg" ;;
    *)
      if [ -n "$VERSION" ]; then
        fail "unexpected extra argument: $arg"
      fi
      VERSION="$arg"
      ;;
  esac
done

if [ -z "$VERSION" ]; then
  usage
  exit 2
fi

command -v brew >/dev/null 2>&1 || fail "Homebrew (brew) is required on the target host"

# 1. Cask installs on a Mac without the repo.
if brew install --cask jackin-project/tap/jackin-desktop; then
  echo "PASS: cask installed (jackin-project/tap/jackin-desktop)"
else
  fail "brew install --cask jackin-project/tap/jackin-desktop failed"
fi

# 2. App landed where the cask's app stanza installs it.
test -d "$APP" || fail "$APP not present after install"
echo "PASS: $APP present"

# 3. Gatekeeper accepts a notarized Developer ID build.
spctl_out="$(spctl --assess --type execute --verbose=2 "$APP" 2>&1 || true)"
case "$spctl_out" in
  *accepted*Notarized\ Developer\ ID* | *Notarized\ Developer\ ID*accepted*)
    echo "PASS: Gatekeeper accepted (Notarized Developer ID)" ;;
  *)
    fail "Gatekeeper did not accept a notarized Developer ID build" ;;
esac

# 4. Notarization ticket is stapled.
xcrun stapler validate "$APP" >/dev/null 2>&1 || fail "stapler validate failed (ticket not stapled)"
echo "PASS: notarization ticket stapled"

# 5. Signature is intact.
codesign --verify --deep --strict "$APP" >/dev/null 2>&1 || fail "codesign --verify --deep --strict failed"
echo "PASS: code signature valid"

# 6. Reported version matches the released version.
got_version="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist" 2>/dev/null || true)"
if [ "$got_version" = "$VERSION" ]; then
  echo "PASS: CFBundleShortVersionString is $VERSION"
else
  fail "version mismatch: bundle reports '$got_version', expected '$VERSION'"
fi

# 7. Launch liveness (LSUIElement — no Dock icon; a live process is the check).
open -a "$APP"
sleep 10
if pgrep -x JackinDesktop >/dev/null 2>&1; then
  echo "PASS: JackinDesktop launched and is alive (menu-bar accessory)"
else
  fail "JackinDesktop did not stay alive after launch"
fi

# 8. Human-only clause: real provider data needs credentialed eyes.
echo "MANUAL: confirm enabled providers appear in the menu bar with real provider data"

# 9. Cleanup unless --keep.
if [ "$KEEP" -eq 1 ]; then
  echo "PASS: --keep set, leaving $APP installed"
  exit 0
fi

pkill -x JackinDesktop || true
brew uninstall --cask jackin-desktop || fail "brew uninstall --cask jackin-desktop failed"
test ! -d "$APP" || fail "$APP still present after uninstall"
echo "PASS: uninstalled cleanly"
