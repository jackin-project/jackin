# Plan 008: Prove parity and deliver the signed desktop artifact

## Status
REJECTED (external release authorization required)

## Why this matters
Completion means one truth survives concurrency and one public artifact survives the actual install chain.

## Preconditions — run before anything else
Plans 001–007 DONE; no unresolved must-not or zero-test gates; external credentials available only for credentialed release steps.

## Spec contract
Parity/release: one fixture truth, architecture proof, surface conformance, immutable release chain, single branch.

## Must NOT
N1-N14.

## Inputs to provide
All golden fixtures/tests, bypass audit, desktop release scripts, Apple/Homebrew credential locations/types without values.

## Starting state
Every surface implemented on current branch; public distribution proof remains.

## Commands you will need
All named focused tests; `rtk mise run ci`; `rtk mise run desktop-ci`; `rtk mise run desktop-merge`; `rtk mise run desktop-sign-notarize`; `rtk mise run desktop-release-state`; exact tag/release/cask commands from research 08 after revalidation against current workflow help.

## Suggested executor toolkit
Cross-process harness, fixture diff tool, macOS quarantine/Gatekeeper tooling, immutable artifact digest ledger.

## Scope
Final parity, architecture, docs/roadmap, release build/sign/notarize/staple/publish/cask/install proof. No new features.

## Git workflow
Remain on `chore/roadmap-unified-agent-usage` and PR #898. Signed commits and immediate normal pushes only. Never open another PR or force-push.

## Steps
### Step 1: Run cross-surface fixture parity
Assert identical IDs/order/values/labels/states/failures across Rust adapters; layout-only differences explicitly allowlisted.
### Step 2: Run adversarial authority proof
Concurrent consumers, owner exit, cancellation, crash/restart, catalog replacement, direct-call/bypass audit.
### Step 3: Run complete surface matrices
CLI, Console, Capsule, FFI/desktop, accessibility, runtime/display and documentation gates; zero filtered tests fail.
### Step 4: Produce and attest one artifact
Build once; record digest; Developer ID sign, notarize, staple, quarantine-aware Gatekeeper launch against that digest.
### Step 5: Publish and install same digest
After every premerge gate passes, merge PR #898, tag that exact merge result through the
existing stable release workflow, and prove immutable publication, cask audit, clean
install/launch/uninstall, and digest continuity. This creates no additional jackin❯
implementation branch or PR. If the separate Homebrew tap requires its normal automated
release PR, treat it as an external distribution operation, not implementation work;
record its URL and require operator approval rather than silently bypassing it.
### Step 6: Close roadmap and PR evidence
Update user/contributor docs, roadmap status/log/index, PR checklist and exact command/results. Keep PR #898 as sole delivery PR.

## Test plan
Every command from research 05/08 plus full repo gates. Credential-absent state is BLOCKED, never falsely passed; noncredentialed gates remain runnable.

## Done criteria
All B1–B8 pass; must-not registry has executable enforcement; PR #898 is the only
jackin❯ implementation PR and is merged before tag publication; the exact public digest
completes the release/cask chain; roadmap/docs are current.

## STOP conditions
Artifact digest changes mid-chain; signing/notary/cask credential absent; any direct provider bypass; any surface semantic mismatch; branch/PR differs.

## Maintenance notes
Preserve release evidence without secrets; future schema/provider changes rerun parity and distribution gates.

## Execution evidence

- Cross-surface and authority proof passed through the unified Rust/FFI/native
  harnesses: `rtk mise run desktop-test` completed 310 Rust tests plus the
  native parity, architecture, provider-mark, and Swift unit harnesses.
- Native production gates passed on this branch with
  `rtk mise run desktop-ci`, `rtk mise run desktop-bindings-check`,
  `rtk mise run desktop-format-check`, and `rtk mise run desktop-lint`. The
  retired Capsule direct diagnostic was removed from the provider-call
  allowlist; the inventory test now passes. The real-app UI matrix now passes
  all 19 tests through `rtk mise run desktop-test-ui`; the same matrix also
  passed inside `rtk mise run desktop-merge`, including retained-window,
  accessibility, minimum-size, popover, and command-state cases.
- Ad-hoc artifact proof passed with
  `rtk mise run desktop-verify native/dist/JackinDesktop.app 0.6.0 1`.
  Release-mode verification correctly failed closed because the artifact is
  not Developer ID signed/notarized. Read-only release reconciliation reports
  `release_exists=false`, `app_file_assets_complete=false`,
  `formula_complete=false`, and `cask_complete=false`.
- Plan 008 is explicitly rejected at the external boundary. The remaining
  actions—Developer ID signing, notarization/stapling, stable publication,
  merging PR #898, tag creation, Homebrew-tap PR mutation, and clean-machine
  install/uninstall proof—were not executed because the operator explicitly
  prohibited them. No credentials, tag, release, merge, or tap write was
  performed.
