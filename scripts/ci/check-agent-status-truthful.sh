#!/usr/bin/env bash
# Bind the agent-runtime-status roadmap page's claimed status to reality.
#
# Root cause this guards (see the roadmap page's "Root cause" section): the page
# once claimed "Implemented in V1 / smoke-tested" while the agent_status module
# tree was never declared in the capsule crate, so it never compiled and no test
# ran. A doc status asserted from intent rather than from an executed gate is a
# whole class of "shipped" lies. This check fails CI when the page claims the
# feature is implemented but the implementation is not actually wired into the
# build — so the claim and the code can never diverge again.
#
# Rule: if the page's **Status** line says the feature is implemented/shipped,
# then `pub mod agent_status;` must exist in the capsule lib.rs AND the module
# root must no longer be grandfathered in cargo-shear's ignored-paths. Otherwise
# the page is making a claim the build does not back.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
page="$repo_root/docs/content/docs/reference/roadmap/agent-runtime-status.mdx"
lib="$repo_root/crates/jackin-capsule/src/lib.rs"
cargo_toml="$repo_root/Cargo.toml"

status_line="$(grep -m1 '^\*\*Status\*\*:' "$page" || true)"
if [[ -z "$status_line" ]]; then
  echo "check-agent-status-truthful: no **Status** line found in $page" >&2
  exit 1
fi

# Does the status claim the feature is implemented/shipped/done?
# Negative phrases ("not wired", "design complete", "partially") mean "not yet".
shopt -s nocasematch
claims_implemented=false
if [[ "$status_line" =~ (implemented|shipped|landed|complete\ and\ live) ]] \
   && [[ ! "$status_line" =~ (not\ wired|design\ complete|partially|not\ implemented|in\ progress) ]]; then
  claims_implemented=true
fi
shopt -u nocasematch

if [[ "$claims_implemented" != true ]]; then
  echo "check-agent-status-truthful: page does not claim 'Implemented' — nothing to enforce. OK."
  exit 0
fi

errors=0
if ! grep -qE '^\s*pub mod agent_status;' "$lib"; then
  echo "FAIL: page claims the feature is Implemented, but 'pub mod agent_status;' is absent from $lib" >&2
  errors=1
fi
if grep -qE 'agent_status\.rs' "$cargo_toml"; then
  echo "FAIL: page claims Implemented, but agent_status.rs is still grandfathered in cargo-shear ignored-paths in $cargo_toml" >&2
  errors=1
fi

if [[ "$errors" -ne 0 ]]; then
  echo "The roadmap page claims this feature is implemented while it is not wired into the build. Fix the code or correct the page's Status." >&2
  exit 1
fi

echo "check-agent-status-truthful: page claims Implemented and agent_status is wired. OK."
