#!/usr/bin/env bash
# Offline fixtures for release-usage-menu-bar-state.sh (no network writes).
# Mocks `gh` and `curl` via PATH wrappers.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE="$ROOT/scripts/release-usage-menu-bar-state.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

bin="$TMP/bin"
mkdir -p "$bin"

# --- fixture: no release ---
cat >"$bin/gh" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" == *"release view"* ]]; then
  exit 1
fi
exit 0
EOF
cat >"$bin/curl" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
chmod +x "$bin/gh" "$bin/curl"
out="$(PATH="$bin:$PATH" bash "$STATE" 1.2.3)"
echo "$out" | grep -q 'release_exists=false'
echo "$out" | grep -q 'app_file_assets_complete=false'
echo "$out" | grep -q 'complete=false'
echo "ok: missing release"

# --- fixture: complete release + cask + formula ---
# release-usage-menu-bar-state.sh calls:
#   gh release view vX (existence)
#   gh release view vX --json assets --jq '.assets[].name'  → one name per line
cat >"$bin/gh" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" == *"release view"* && "$*" == *"--json assets"* ]]; then
  cat <<'NAMES'
jackin-usage-menu-bar-1.2.3-aarch64-apple-darwin.zip
jackin-usage-menu-bar-1.2.3-aarch64-apple-darwin.zip.sha256
jackin-usage-menu-bar-1.2.3-aarch64-apple-darwin.zip.bundle
jackin-usage-menu-bar-1.2.3-aarch64-apple-darwin.zip.sbom.json
NAMES
  exit 0
fi
if [[ "$*" == *"release view"* ]]; then
  exit 0
fi
exit 0
EOF
cat >"$bin/curl" <<'EOF'
#!/usr/bin/env bash
url="${@: -1}"
if [[ "$url" == *Formula/jackin.rb* ]]; then
  echo '  version "1.2.3"'
  exit 0
fi
if [[ "$url" == *Casks/jackin-usage-menu-bar.rb* ]]; then
  cat <<'CASK'
  version "1.2.3"
  sha256 "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  url "https://github.com/jackin-project/jackin/releases/download/v1.2.3/jackin-usage-menu-bar-1.2.3-aarch64-apple-darwin.zip"
CASK
  exit 0
fi
exit 1
EOF
chmod +x "$bin/gh" "$bin/curl"
out="$(PATH="$bin:$PATH" bash "$STATE" 1.2.3)"
echo "$out" | grep -q 'release_exists=true'
echo "$out" | grep -q 'app_file_assets_complete=true'
echo "$out" | grep -q 'formula_complete=true'
echo "$out" | grep -q 'cask_complete=true'
echo "$out" | grep -q 'complete=true'
# idempotent second run
out2="$(PATH="$bin:$PATH" bash "$STATE" 1.2.3)"
[[ "$out" == "$out2" ]]
echo "ok: complete state + idempotent"

# --- fixture: conflict-ish partial assets (zip without sidecars) ---
cat >"$bin/gh" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" == *"release view"* && "$*" == *"--json assets"* ]]; then
  echo 'jackin-usage-menu-bar-9.9.9-aarch64-apple-darwin.zip'
  exit 0
fi
if [[ "$*" == *"release view"* ]]; then
  exit 0
fi
exit 0
EOF
cat >"$bin/curl" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
chmod +x "$bin/gh" "$bin/curl"
out="$(PATH="$bin:$PATH" bash "$STATE" 9.9.9)"
echo "$out" | grep -q 'release_exists=true'
echo "$out" | grep -q 'app_file_assets_complete=false'
echo "$out" | grep -q 'complete=false'
echo "ok: partial assets incomplete"

echo "ALL FIXTURES PASS"
