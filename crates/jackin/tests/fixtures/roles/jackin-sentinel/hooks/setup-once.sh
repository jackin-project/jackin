#!/usr/bin/env sh

# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

set -eu

state_dir="/jackin/state/jackin-sentinel"
bin_dir="$state_dir/bin"
mkdir -p "$state_dir" "$bin_dir"

{
  printf 'setup_once=1\n'
  printf 'agent=%s\n' "${JACKIN_AGENT:-unset}"
  printf 'static_default=%s\n' "${STATIC_DEFAULT:-unset}"
  printf 'literal_template=%s\n' "${LITERAL_TEMPLATE:-unset}"
} > "$state_dir/setup-once.env"

cat > "$bin_dir/jackin-sentinel-report" <<'REPORT'
#!/usr/bin/env sh
set -eu

state_dir="${JACKIN_SENTINEL_STATE_DIR:-/jackin/state/jackin-sentinel}"
preflight_count=0
if [ -f "$state_dir/preflight.log" ]; then
  preflight_count="$(grep -c '^preflight=1$' "$state_dir/preflight.log" || true)"
fi

echo "JACKIN_SENTINEL_REPORT_BEGIN"
echo "JACKIN=${JACKIN:-unset}"
echo "JACKIN_AGENT=${JACKIN_AGENT:-unset}"
echo "JACKIN_INSTANCE_ID=${JACKIN_INSTANCE_ID:-unset}"
echo "JACKIN_DIND_HOSTNAME=${JACKIN_DIND_HOSTNAME:-unset}"
echo "TESTCONTAINERS_HOST_OVERRIDE=${TESTCONTAINERS_HOST_OVERRIDE:-unset}"
echo "STATIC_DEFAULT=${STATIC_DEFAULT:-unset}"
echo "LITERAL_TEMPLATE=${LITERAL_TEMPLATE:-unset}"
echo "FREE_TEXT=${FREE_TEXT:-unset}"
echo "FREE_TEXT_REQUIRED=${FREE_TEXT_REQUIRED:-unset}"
echo "SELECT_PROJECT=${SELECT_PROJECT:-unset}"
echo "SELECT_MODE=${SELECT_MODE:-unset}"
echo "OPTIONAL_API_KEY=${OPTIONAL_API_KEY:-unset}"
echo "BRANCH=${BRANCH:-unset}"
echo "COMBINED_LABEL=${COMBINED_LABEL:-unset}"
echo "OPTIONAL_DERIVED=${OPTIONAL_DERIVED:-unset}"
echo "JACKIN_SENTINEL_SOURCE_HOOK=${JACKIN_SENTINEL_SOURCE_HOOK:-unset}"
echo "JACKIN_SENTINEL_PREFLIGHT_COUNT=$preflight_count"
if [ -f "$state_dir/setup-once.env" ]; then
  sed 's/^/SETUP_ONCE_MARKER_/' "$state_dir/setup-once.env"
fi
if command -v docker >/dev/null 2>&1; then
  echo "DOCKER_CLI=available"
  docker ps --format 'DOCKER_PS={{.Names}} {{.Status}}' || true
else
  echo "DOCKER_CLI=missing"
fi
echo "JACKIN_SENTINEL_REPORT_END"
REPORT

chmod +x "$bin_dir/jackin-sentinel-report"
