# MBX setup and PR #1120 prerequisite audit

This is a checkpoint audit, with test/process statuses recorded at `33ddba4`. Later results and closed integration failures are in [verification.md](verification.md); do not interpret the pending process IDs below as current work. The local MBX setup finding remains valid and was rechecked during cleanup: Mise resolves Cargo to its MBX 1.22.0 wrapper.

**Status:** Main already has a local Cargo-to-MBX route. The read-only setup
and PR-status audits found that PR #1120 is not required to enable MBX on main.
At source checkpoint `33ddba4`, the isolated Claude usage (23), coordinator
(54), and broker (160) tests passed; consumer compilation and docs rendering
also passed. A previous MBX-backed CLI run passed 48 tests, while a newer CLI
rerun and four integration-failure rerun remain pending. These results do not
establish native Keychain, real-account, installed-binary, or whole-workspace
readiness.

## Existing main setup

At source checkpoint `33ddba4`, [`mise.toml`](../../mise.toml#L29-L59)
pins `mr-boxington = "1.22.0"` and wraps `cargo` with `command = "mbx"` plus
`MBX_CARGO_SHIM_MODE = "1"`. The checked-in
[`mise.lock`](../../mise.lock#L490-L507) pins the MBX platform assets, checksums,
and Sigstore signer. The read-only host inspection reported Mise `2026.10.6`
and MBX `1.22.0`.

Rust selection remains separately pinned: [`rust-toolchain.toml`](../../rust-toolchain.toml#L9-L12)
uses Rust `1.97.1`, matching the workspace `rust-version` in
[`Cargo.toml`](../../Cargo.toml#L42-L46). The current CI workflow has its own
tool setup at [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml#L76-L89),
including Rust `1.98.1`, `mr-boxington@1.21.0`, and disabled Mise auto-install
for subsequent steps. Treat this as a toolchain-alignment issue to audit
separately; local MBX routing is already configured.

## Focused usage test (completed)

The parent launched this command in an isolated checkout:

```sh
MISE_AUTO_INSTALL=false mise exec -- cargo test --offline --locked -p jackin-usage --lib usage::claude::
```

Process `44301` completed with exit status 0: 23 passed, 0 failed. The log is
`/private/tmp/jackin-mbx-claude-tests.log`. Its output includes
`mbx[cache]: moved existing target under managed root`. The MBX report says
614 MiB stored locally, 0 B uploaded/downloaded, 427 cache misses, 21 bypassed,
and 19 not looked up. This proves the isolated Cargo invocation used the MBX
path and its local cache; it does not establish native Keychain access, a real
account callback, a provider request, or whole-workspace readiness. These are
offline fixture tests, not live credential checks.

The parent-run coordinator test process `30495` completed with exit status 0:
54 passed, 0 failed; its log is `/private/tmp/jackin-mbx-coordinator-tests.log`.
This is also isolated MBX-backed test evidence, not native Keychain or
whole-workspace readiness. Consumer compilation run 5 passed for binary and
library targets in `jackin`, `usage-ffi`, `runtime`, and `capsule`; its log is
`/private/tmp/jackin-mbx-consumers-check-5.log`.

The final isolated broker suite passed 160 tests and failed none at source
checkpoint `33ddba4`; its log is
`/private/tmp/jackin-mbx-broker-tests-final-2.log`. MBX reported 521 cache
hits, 384 misses, 16 not looked up, 21 bypassed, 0 B transferred, and 1.4 GiB
stored locally. This focused suite does not complete migration or integration
verification.

A previous MBX-backed CLI test run passed 48 tests. The newer rerun was
reported running as process `12990`; its log is
`/private/tmp/jackin-mbx-cli-tests-2.log`. Do not treat the earlier pass as
current-checkpoint proof until the rerun is reviewed.

MDX regeneration and docs verification passed 18 tests, built 1,293 HTML
pages, and passed HTML plus hydrated rendering. The render verifier saved
`/private/tmp/jackin-usage-schema4-final.png` and
`/private/tmp/jackin-collector-final.png`; this is documentation proof only.

## PR #1120 status and relevance

[PR #1120](https://github.com/jackin-project/jackin/pull/1120) remains open
and draft at head `9ef617d014df15c6d967501bcbbdcb3689ed453c`, based on main
`868ce53519234f879d26a8bd2dffaa30fae2d729` at the time of the status audit;
the observed merge base was `9b6e1d…`. Re-fetch both refs before integration
decisions; this is not a claim that the PR head matches current main. The
read-only check inventory
reported Actionlint and DCO passing, Plan and Required failing, and 28 checks
skipped. Plan failed while Velnor prepared generated files: Rust `1.98.1` was
unavailable with auto-install disabled and offline Cargo metadata exited
nonzero; Required failed as a consequence. The draft and blocked required
check prevent treating the PR as merge-ready. The status audit found no
submitted GitHub reviews or review threads. Independent Rust source review
retained one bounded candidate around the download writer in
[`net.rs`](https://github.com/jackin-project/jackin/blob/9ef617d014df15c6d967501bcbbdcb3689ed453c/crates/jackin-docker/src/net.rs#L197):
the writer-error path may silently return `Ok`. This is source evidence, not
runtime proof; disposition remains pending. Do not call the PR's Rust changes
ready.

PR #1120 is not needed to turn on MBX for the current main checkout because
the `mise.toml` wrapper already routes Cargo through MBX. Consider the PR only
if a separate main-integration audit identifies a need for its changes. Any
merge decision must follow current-head review, disposition of findings, and
required checks; no merge was performed or verified in this audit.

The generator pin also needs an upstream-supported release before regenerating
project files. The published [Velnor v0.1.4 release](https://github.com/tailrocks/velnor-new/releases/tag/v0.1.4)
and current [v0.1.5 draft](https://github.com/tailrocks/velnor-new/releases/tag/untagged-6899c9b4aa4e941dadba)
both use catalog pins Rust `1.98.1` and MBX `1.21.1` (see the
[v0.1.4 catalog](https://github.com/tailrocks/velnor-new/blob/d3590d321e51f7b99bfb0a92c11d3d6eb7af61cd/crates/velnor-actions-mise/src/catalog.rs#L43-L51)).
The current draft has the same pins, so it does not provide a newer coherent
release combination for a different project toolchain. The Velnor config schema
has no repository-level Rust/MBX version override
([config contract](https://github.com/tailrocks/velnor-new/blob/d3590d321e51f7b99bfb0a92c11d3d6eb7af61cd/crates/velnor-actions-contract/src/config/mod.rs#L83-L110)).

No generated workflow was hand-edited, no local generator was run, and no PR
was merged for this audit. The upstream inspection covered release assets and
attestation only. Do not claim root Rust readiness from the local MBX route or
from upstream asset availability.

## Landing gates

- Record the completed Claude usage test result with its exact scope and
  offline-fixture limits in verification notes.
- Preserve the coordinator test result (54 passed, 0 failed) with its isolated
  MBX and fixture limits.
- Preserve the completed consumer compilation result and its scoped limits.
- Preserve the 160/0 broker result with its isolated MBX and fixture limits.
- Review the pending CLI rerun and rerun the four failed Required integration
  cases after their source fixes; do not treat either result as complete yet.
- Record disposition of the source-only finding and refresh PR #1120's reviews,
  threads, and checks at its current head.
- Decide whether PR #1120 is needed for main integration; do not merge it just
  to enable MBX.
- Pass the main-integration checks on the chosen usage candidate before the
  usage PR lands.

The usage protocol gate remains 133 passing tests at source checkpoint
`33ddba4` (wire v7, durable monitor schema v4). The isolated broker suite
passed 160 tests with no failures. Four Required integration failures have
source fixes in the worktree awaiting rerun; CLI rerun and migration runtime
verification also remain pending.
