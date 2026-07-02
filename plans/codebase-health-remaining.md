# Codebase health — PR #664 merge-blocker checklist

> **Status: open for PR #664 only.** This is not a new ledger burn-down plan. It is the merge-readiness checklist from the post-refactor audit and should be closed again once every blocker below is resolved or explicitly accepted.

## Goal

PR #664 is intended to be a structure-only codebase-health refactor. Before merge, prove that it did not relax behavior, hide runtime failures, or weaken the CI gates that are supposed to prevent the codebase-health ledgers from regressing.

## Blocker 1 — restore `jackin prewarm` CLI invariants

The refactor moved `prewarm` flags into nested structs, but several previous `clap` constraints were dropped. Restore the old invalid-combination behavior before merge.

- [ ] In <RepoFile path="crates/jackin/src/cli/prewarm.rs">crates/jackin/src/cli/prewarm.rs</RepoFile>, restore `--keep-sidecar-container` requiring `--sidecar-container`.
- [ ] Restore `--role`, `--workspace`, `--role-git`, `--all-workspaces`, and `--all-roles` conflicts to match the pre-refactor behavior.
- [ ] Restore `--all-roles` requiring `--image`.
- [ ] Add CLI parse regression tests that reject these combinations:
  - [ ] `prewarm --keep-sidecar-container`
  - [ ] `prewarm --role <name> --all-workspaces`
  - [ ] `prewarm --role <name> --all-roles --image`
  - [ ] `prewarm --workspace <name> --all-roles --image`
  - [ ] `prewarm --role <name> --role-git <url> --all-workspaces`
  - [ ] `prewarm --all-roles` without `--image`
- [ ] Add at least one positive parse test for the intended image-prewarm path.

## Blocker 2 — keep launch failures visible and actionable

The launch TUI can currently enter a failed stage while lower-priority overlays are still open. The failure surface must win so an operator can see and acknowledge the failure.

- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/update.rs">crates/jackin-launch-tui/src/tui/update.rs</RepoFile>, make `StageFailed` close or reset lower-priority overlays such as the build log, build-log dragging state, and container-info dialog.
- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/view.rs">crates/jackin-launch-tui/src/tui/view.rs</RepoFile>, make the failure dialog render above the build log as a defensive guarantee.
- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/components/failure_dialog.rs">crates/jackin-launch-tui/src/tui/components/failure_dialog.rs</RepoFile>, prevent long failure diagnostics or next-step rows from being silently clipped with no scroll path.
- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/subscriptions.rs">crates/jackin-launch-tui/src/tui/subscriptions.rs</RepoFile>, route outside clicks on the failure dialog through the modal classifier and dispatch the same acknowledgement path used by the keyboard/button flow.
- [ ] Add tests for:
  - [ ] `StageFailed` while `build_log_open = true`.
  - [ ] failure rendering when a build log was previously open.
  - [ ] outside-click dismissal for the failure dialog.
  - [ ] long diagnostics/next-step content remaining reachable instead of clipped.

## Blocker 3 — make codebase-health CI gates actually required

The strict gates exist, but the aggregate required job does not currently depend on every gate that should block a merge.

- [ ] In <RepoFile path=".github/workflows/ci.yml">.github/workflows/ci.yml</RepoFile>, add `file-size-gate` to `ci-required.needs`.
- [ ] Add the existing `schema-check` job to `ci-required.needs` so the aggregate job cannot pass while it is skipped, failed, or cancelled.
- [ ] Add `clippy.toml`, `file-size-budget.toml`, and `test-layout-allowlist.toml` to the Rust path filter so ratchet edits run the Rust gate set.
- [ ] Run `actionlint` or the repo's workflow lint command after editing the workflow.

## Blocker 4 — make the ratchets shrink-only

The ledgers are empty today, but the lint implementation still accepts stale budget rows if they return later. The gate should fail when a ratchet row no longer matches a real over-cap exception.

- [ ] In <RepoFile path="crates/jackin-xtask/src/lint.rs">crates/jackin-xtask/src/lint.rs</RepoFile>, fail when a file-size budget entry points at a missing file.
- [ ] Fail when a budget entry is no longer needed because the measured file is under the production/test cap.
- [ ] Fail when a budgeted over-cap file records a count higher than the current measured count.
- [ ] In <RepoFile path="crates/jackin-xtask/src/test_layout.rs">crates/jackin-xtask/src/test_layout.rs</RepoFile>, fail stale or missing `test-layout-allowlist.toml` entries instead of only printing a note.
- [ ] Update xtask tests so stale/shrunken allowlist rows are rejected, not accepted.
- [ ] Run `cargo run -p jackin-xtask --locked -- lint --strict`.

## Final Verification

- [ ] `cargo fmt --check`
- [ ] `cargo check --workspace --all-targets --all-features --locked`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo run -p jackin-xtask --locked -- lint --strict`
- [ ] `cargo nextest run --all-features --no-fail-fast -E 'not test(/shell_session_gets_only_status_socket/)'`
- [ ] Docs gate: `bun run build`, `bun run check:repo-links`, `bun run check:roadmap-sidebar`, `bunx tsc --noEmit`, and `bun test`
- [ ] `gh pr checks 664 --watch=false` shows the latest pushed head has the required checks, not only DCO.
