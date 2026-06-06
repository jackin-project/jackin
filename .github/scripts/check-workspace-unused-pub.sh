#!/usr/bin/env bash
set -euo pipefail

raw_output="$(mktemp)"
unexpected="$(mktemp)"

cleanup() {
  rm -f "${raw_output}" "${unexpected}" index.scip
}
trap cleanup EXIT

set +e
cargo workspace-unused-pub >"${raw_output}" 2>&1
status=$?
set -e

cat "${raw_output}"

if [[ "${status}" -eq 0 ]]; then
  exit 0
fi

awk '
  /^crates\// {
    current = $0
    next
  }

  /^[[:space:]]*[0-9]+[[:space:]]+/ {
    line = $0

    # cargo-workspace-unused-pub 0.1.0 reports test functions as unused API.
    if (current ~ /\/tests\//) next
    if (current == "crates/jackin-runtime/src/runtime/drift.rs" && line ~ /async fn detect_drift_/) next

    # Trait impl methods required by external traits, but reported as functions.
    if (current == "crates/jackin-capsule/src/tui/socket_backend.rs" && line ~ /fn (get_cursor_position|set_cursor_position|window_size)/) next
    if (current == "crates/jackin-term/src/grid.rs" && line ~ /fn esc_dispatch\(/) next

    # Documented intentional survivors.
    if (current == "crates/jackin-tui-lookbook/src/svg.rs" && line ~ /render_story_to_text/) next
    if (current == "crates/jackin-tui/src/components/focus_owner.rs") next

    print current ":" line
  }
' "${raw_output}" >"${unexpected}"

if [[ -s "${unexpected}" ]]; then
  echo "::error::cargo workspace-unused-pub found undocumented unused public API"
  cat "${unexpected}"
  exit 1
fi

echo "cargo workspace-unused-pub found only documented false positives/exceptions"
