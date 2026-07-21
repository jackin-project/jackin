#!/usr/bin/env bash
# Interactive bootstrap for environment `release-macos` Apple credentials.
# Never prints secret material. Uses `gh secret set` / `gh variable set`.
#
# Sources (first match wins per item):
#   1) Env vars already exported
#   2) Local file paths via flags
#   3) 1Password `op read` references (when `op` session is unlocked)
#
# Usage examples:
#   ./scripts/bootstrap-release-macos-secrets.sh \
#     --p12 ./DeveloperID.p12 --p12-password-env P12_PASS \
#     --p8 ./AuthKey_XXXXXX.p8 --key-id XXXXXX --issuer UUID \
#     --team-id ABCD123456 --cert-sha256 <hex>
#
#   ./scripts/bootstrap-release-macos-secrets.sh \
#     --op-p12 'op://Private/Apple Developer ID/p12' \
#     --op-p12-password 'op://Private/Apple Developer ID/password' \
#     --op-p8 'op://Private/App Store Connect API/credential' \
#     --key-id … --issuer … --team-id … --cert-sha256 …
#
# After secrets land: cut a non-dev version on main, then
#   gh workflow run release.yml -f mode=publish -f lanes=github
set -euo pipefail

ENV_NAME=release-macos
REPO="${GITHUB_REPOSITORY:-jackin-project/jackin}"

P12_PATH=""
P12_PASSWORD=""
P12_PASSWORD_ENV=""
P8_PATH=""
KEY_ID=""
ISSUER=""
TEAM_ID=""
CERT_SHA256=""
OP_P12=""
OP_P12_PASSWORD=""
OP_P8=""
DRY_RUN=0

fail() { echo "error: $*" >&2; exit 1; }
info() { echo "==> $*" >&2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --p12) P12_PATH="$2"; shift 2 ;;
    --p12-password) P12_PASSWORD="$2"; shift 2 ;;
    --p12-password-env) P12_PASSWORD_ENV="$2"; shift 2 ;;
    --p8) P8_PATH="$2"; shift 2 ;;
    --key-id) KEY_ID="$2"; shift 2 ;;
    --issuer) ISSUER="$2"; shift 2 ;;
    --team-id) TEAM_ID="$2"; shift 2 ;;
    --cert-sha256) CERT_SHA256="$2"; shift 2 ;;
    --op-p12) OP_P12="$2"; shift 2 ;;
    --op-p12-password) OP_P12_PASSWORD="$2"; shift 2 ;;
    --op-p8) OP_P8="$2"; shift 2 ;;
    --repo) REPO="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    *) fail "unknown arg: $1" ;;
  esac
done

command -v gh >/dev/null || fail "gh CLI required"
command -v base64 >/dev/null || fail "base64 required"
command -v shasum >/dev/null || fail "shasum required"

op_read() {
  local ref="$1"
  command -v op >/dev/null || fail "1Password CLI (op) required for op:// refs"
  op read "$ref"
}

resolve_p12_b64() {
  if [[ -n "${DEVELOPER_ID_APPLICATION_P12_BASE64:-}" ]]; then
    printf '%s' "$DEVELOPER_ID_APPLICATION_P12_BASE64"
    return
  fi
  local path="$P12_PATH"
  if [[ -z "$path" && -n "$OP_P12" ]]; then
    local tmp
    tmp="$(mktemp)"
    op_read "$OP_P12" >"$tmp"
    path="$tmp"
    trap 'rm -f "'"$tmp"'"' RETURN
  fi
  [[ -n "$path" && -f "$path" ]] || fail "provide --p12, --op-p12, or DEVELOPER_ID_APPLICATION_P12_BASE64"
  base64 <"$path" | tr -d '\n'
}

resolve_p12_password() {
  if [[ -n "${DEVELOPER_ID_APPLICATION_P12_PASSWORD:-}" ]]; then
    printf '%s' "$DEVELOPER_ID_APPLICATION_P12_PASSWORD"
    return
  fi
  if [[ -n "$P12_PASSWORD" ]]; then
    printf '%s' "$P12_PASSWORD"
    return
  fi
  if [[ -n "$P12_PASSWORD_ENV" ]]; then
    eval "printf '%s' \"\${$P12_PASSWORD_ENV:-}\""
    return
  fi
  if [[ -n "$OP_P12_PASSWORD" ]]; then
    op_read "$OP_P12_PASSWORD"
    return
  fi
  fail "provide --p12-password, --p12-password-env, --op-p12-password, or DEVELOPER_ID_APPLICATION_P12_PASSWORD"
}

resolve_p8() {
  if [[ -n "${APP_STORE_CONNECT_API_KEY_P8:-}" ]]; then
    printf '%s' "$APP_STORE_CONNECT_API_KEY_P8"
    return
  fi
  if [[ -n "$P8_PATH" ]]; then
    [[ -f "$P8_PATH" ]] || fail "p8 not found: $P8_PATH"
    cat "$P8_PATH"
    return
  fi
  if [[ -n "$OP_P8" ]]; then
    op_read "$OP_P8"
    return
  fi
  fail "provide --p8, --op-p8, or APP_STORE_CONNECT_API_KEY_P8"
}

derive_cert_sha256_from_p12() {
  local p12_path="$1" pass="$2"
  local tmp pem
  tmp="$(mktemp -d)"
  pem="$tmp/cert.pem"
  if ! openssl pkcs12 -in "$p12_path" -clcerts -nokeys -passin "pass:$pass" -out "$pem" 2>/dev/null; then
    rm -rf "$tmp"
    return 1
  fi
  openssl x509 -in "$pem" -fingerprint -sha256 -noout 2>/dev/null \
    | awk -F= '{print tolower($2)}' | tr -d ':'
  rm -rf "$tmp"
}

KEY_ID="${KEY_ID:-${APP_STORE_CONNECT_KEY_ID:-}}"
ISSUER="${ISSUER:-${APP_STORE_CONNECT_ISSUER_ID:-}}"
TEAM_ID="${TEAM_ID:-${JACKIN_DEVELOPER_ID_TEAM_ID:-}}"
CERT_SHA256="${CERT_SHA256:-${JACKIN_DEVELOPER_ID_CERT_SHA256:-}}"

[[ -n "$KEY_ID" ]] || fail "App Store Connect key id required (--key-id)"
[[ -n "$ISSUER" ]] || fail "App Store Connect issuer id required (--issuer) for team keys"

info "resolving PKCS#12 (not printing bytes)"
P12_B64="$(resolve_p12_b64)"
info "resolving PKCS#12 password (not printing)"
P12_PASS="$(resolve_p12_password)"
info "resolving App Store Connect .p8 (not printing)"
P8_BODY="$(resolve_p8)"

# Optional fingerprint derivation when p12 path known.
if [[ -z "$CERT_SHA256" && -n "$P12_PATH" && -f "$P12_PATH" ]]; then
  info "deriving certificate SHA-256 from p12"
  CERT_SHA256="$(derive_cert_sha256_from_p12 "$P12_PATH" "$P12_PASS" || true)"
fi
if [[ -z "$CERT_SHA256" ]]; then
  info "warning: CERT_SHA256 empty — publish will skip fingerprint fail-closed check unless you set JACKIN_DEVELOPER_ID_CERT_SHA256"
fi
if [[ -z "$TEAM_ID" ]]; then
  info "warning: TEAM_ID empty — set JACKIN_DEVELOPER_ID_TEAM_ID for fail-closed Team ID check"
fi

if [[ "$DRY_RUN" == "1" ]]; then
  info "dry-run: would set secrets/vars on $REPO env=$ENV_NAME"
  info "  secrets: DEVELOPER_ID_APPLICATION_P12_BASE64, DEVELOPER_ID_APPLICATION_P12_PASSWORD, APP_STORE_CONNECT_API_KEY_P8, APP_STORE_CONNECT_KEY_ID, APP_STORE_CONNECT_ISSUER_ID"
  info "  variables: JACKIN_DEVELOPER_ID_TEAM_ID, JACKIN_DEVELOPER_ID_CERT_SHA256"
  info "  key-id length=${#KEY_ID} issuer length=${#ISSUER} p12_b64 length=${#P12_B64} p8 length=${#P8_BODY}"
  exit 0
fi

info "ensuring environment $ENV_NAME exists"
gh api -X PUT "repos/${REPO}/environments/${ENV_NAME}" --input - <<'EOF' >/dev/null
{"wait_timer":0,"prevent_self_review":false,"reviewers":[],"deployment_branch_policy":null}
EOF

info "writing secrets to $REPO environment $ENV_NAME (values not echoed)"
printf '%s' "$P12_B64" | gh secret set DEVELOPER_ID_APPLICATION_P12_BASE64 --repo "$REPO" --env "$ENV_NAME"
printf '%s' "$P12_PASS" | gh secret set DEVELOPER_ID_APPLICATION_P12_PASSWORD --repo "$REPO" --env "$ENV_NAME"
printf '%s' "$P8_BODY" | gh secret set APP_STORE_CONNECT_API_KEY_P8 --repo "$REPO" --env "$ENV_NAME"
printf '%s' "$KEY_ID" | gh secret set APP_STORE_CONNECT_KEY_ID --repo "$REPO" --env "$ENV_NAME"
printf '%s' "$ISSUER" | gh secret set APP_STORE_CONNECT_ISSUER_ID --repo "$REPO" --env "$ENV_NAME"

if [[ -n "$TEAM_ID" ]]; then
  printf '%s' "$TEAM_ID" | gh variable set JACKIN_DEVELOPER_ID_TEAM_ID --repo "$REPO"
fi
if [[ -n "$CERT_SHA256" ]]; then
  printf '%s' "$CERT_SHA256" | gh variable set JACKIN_DEVELOPER_ID_CERT_SHA256 --repo "$REPO"
fi

info "done. Verify names only:"
gh secret list --repo "$REPO" --env "$ENV_NAME"
info "Next: on main with non-dev version, run"
info "  gh workflow run release.yml --ref main -f mode=publish -f lanes=github"
