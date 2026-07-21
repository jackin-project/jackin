#!/usr/bin/env bash
# Compute independent publication states for jackin-desktop release assets.
# Usage:
#   release-usage-menu-bar-state.sh <version> [--repo owner/name]
# Prints KEY=value lines suitable for GITHUB_OUTPUT.
#
# Keys:
#   release_exists
#   app_file_assets_complete   # ZIP + .sha256 + .bundle + .sbom.json
#   formula_complete           # Formula/jackin.rb version matches (best-effort remote)
#   cask_complete              # Casks/jackin-desktop.rb version+url+sha present on tap main
#   complete                   # release + app assets + cask (formula may lag independently)
set -euo pipefail

VERSION="${1:-}"
shift || true
REPO="jackin-project/jackin"
TAP_REPO="jackin-project/homebrew-tap"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    --tap) TAP_REPO="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  echo "usage: $0 <version> [--repo owner/name] [--tap owner/name]" >&2
  exit 2
fi

ASSET="jackin-desktop-${VERSION}-aarch64-apple-darwin.zip"

release_exists=false
app_file_assets_complete=false
formula_complete=false
cask_complete=false

if gh release view "v${VERSION}" --repo "$REPO" >/dev/null 2>&1; then
  release_exists=true
  assets="$(gh release view "v${VERSION}" --repo "$REPO" --json assets --jq '.assets[].name' 2>/dev/null || true)"
  has_zip=false
  has_sha=false
  has_bundle=false
  has_sbom=false
  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    case "$name" in
      "$ASSET") has_zip=true ;;
      "${ASSET}.sha256") has_sha=true ;;
      "${ASSET}.bundle") has_bundle=true ;;
      "${ASSET}.sbom.json") has_sbom=true ;;
    esac
  done <<<"$assets"
  if $has_zip && $has_sha && $has_bundle && $has_sbom; then
    app_file_assets_complete=true
  fi
fi

# Formula version on tap main (CLI formula, not cask).
if formula_body="$(curl -fsSL --max-time 30 \
  "https://raw.githubusercontent.com/${TAP_REPO}/main/Formula/jackin.rb" 2>/dev/null)"; then
  formula_version="$(printf '%s\n' "$formula_body" | sed -n 's/^[[:space:]]*version "\(.*\)"/\1/p' | head -1)"
  if [[ "$formula_version" == "$VERSION" ]]; then
    formula_complete=true
  fi
fi

# Cask presence on tap main.
if cask_body="$(curl -fsSL --max-time 30 \
  "https://raw.githubusercontent.com/${TAP_REPO}/main/Casks/jackin-desktop.rb" 2>/dev/null)"; then
  cask_version="$(printf '%s\n' "$cask_body" | sed -n 's/^[[:space:]]*version "\(.*\)"/\1/p' | head -1)"
  cask_url="$(printf '%s\n' "$cask_body" | sed -n 's/^[[:space:]]*url "\(.*\)"/\1/p' | head -1)"
  cask_sha="$(printf '%s\n' "$cask_body" | sed -n 's/^[[:space:]]*sha256 "\(.*\)"/\1/p' | head -1)"
  expected_url="https://github.com/${REPO}/releases/download/v${VERSION}/${ASSET}"
  if [[ "$cask_version" == "$VERSION" \
    && "$cask_url" == "$expected_url" \
    && -n "$cask_sha" \
    && ${#cask_sha} -eq 64 ]]; then
    cask_complete=true
  fi
fi

complete=false
if $release_exists && $app_file_assets_complete && $cask_complete; then
  complete=true
fi

{
  echo "release_exists=${release_exists}"
  echo "app_file_assets_complete=${app_file_assets_complete}"
  echo "formula_complete=${formula_complete}"
  echo "cask_complete=${cask_complete}"
  echo "complete=${complete}"
  echo "asset=${ASSET}"
}
