# Codebase health — PR #664 merge-blocker checklist

> **Status: open for PR #664 only.** This is not a new ledger burn-down plan. It is the merge-readiness checklist from the post-refactor audit and should be closed again once every blocker below is resolved or explicitly accepted.

## Goal

PR #664 is intended to be a structure-only codebase-health refactor. Before merge, prove that it did not relax behavior, hide runtime failures, or weaken the CI gates that are supposed to prevent the codebase-health ledgers from regressing.

## Pre-Refactor Baseline References

Use these references when a blocker says "restore previous behavior" or "compare with before the refactor." Do not infer the old behavior from memory.

### General Diff Commands

- Current pre-PR baseline is `origin/main`. PR-local changes can be inspected with:
  - `git diff origin/main...HEAD -- <path>`
  - `git show origin/main:<path>`
- If `git show origin/main:<path>` fails with "path exists on disk, but not in origin/main", that file is new in PR #664. In that case, inspect the current file and this plan instead of looking for a pre-refactor version.
- For renamed files, compare old and new paths explicitly:
  - `git show origin/main:<old-path>`
  - `sed -n '<start>,<end>p' <new-path>`
  - `git diff origin/main...HEAD -- <old-path> <new-path>`

### Blocker 1 Baseline: `jackin prewarm`

- Old file before the refactor:
  - `git show origin/main:crates/jackin/src/cli/prewarm.rs`
- Current file after the refactor:
  - <RepoFile path="crates/jackin/src/cli/prewarm.rs">crates/jackin/src/cli/prewarm.rs</RepoFile>
- Exact old constraints to restore were present in `origin/main`:
  - `keep_sidecar_container`: `#[arg(long, requires = "sidecar_container")]`
  - `role`: `#[arg(long, conflicts_with_all = ["workspace", "all_workspaces"])]`
  - `workspace`: `#[arg(long, conflicts_with_all = ["role", "all_workspaces"])]`
  - `all_workspaces`: `#[arg(long, conflicts_with_all = ["role", "workspace", "role_git", "all_roles"])]`
  - `all_roles`: `#[arg(long, requires = "image", conflicts_with_all = ["role", "workspace", "role_git", "all_workspaces"])]`
  - `role_git`: `#[arg(long, requires = "role", conflicts_with_all = ["workspace", "all_workspaces"])]`
- Helpful command to inspect just that old area:
  - `git show origin/main:crates/jackin/src/cli/prewarm.rs | sed -n '20,72p'`
- Existing CLI tests live in:
  - <RepoFile path="crates/jackin/src/cli/tests.rs">crates/jackin/src/cli/tests.rs</RepoFile>
- Existing parse-test pattern to copy:
  - Use `Cli::try_parse_from([...])`.
  - Use `unwrap()` for accepted combinations and `unwrap_err()` or `is_err()` for rejected combinations.

### Blocker 2 Baseline: Launch TUI

- Old crate before the rename/refactor is `crates/jackin-launch/`.
- New crate after the refactor is `crates/jackin-launch-tui/`.
- Compare old/new files with these mappings:
  - `origin/main:crates/jackin-launch/src/tui/update.rs` → <RepoFile path="crates/jackin-launch-tui/src/tui/update.rs">crates/jackin-launch-tui/src/tui/update.rs</RepoFile>
  - `origin/main:crates/jackin-launch/src/tui/view.rs` → <RepoFile path="crates/jackin-launch-tui/src/tui/view.rs">crates/jackin-launch-tui/src/tui/view.rs</RepoFile>
  - `origin/main:crates/jackin-launch/src/tui/subscriptions.rs` → <RepoFile path="crates/jackin-launch-tui/src/tui/subscriptions.rs">crates/jackin-launch-tui/src/tui/subscriptions.rs</RepoFile>
  - `origin/main:crates/jackin-launch/src/tui/components/failure_dialog.rs` → <RepoFile path="crates/jackin-launch-tui/src/tui/components/failure_dialog.rs">crates/jackin-launch-tui/src/tui/components/failure_dialog.rs</RepoFile>
- Current failure-hiding risk points:
  - <RepoFile path="crates/jackin-launch-tui/src/tui/view.rs">view.rs</RepoFile> currently has an early `if view.build_log_open { ... return; }` before the failure dialog path.
  - <RepoFile path="crates/jackin-launch-tui/src/tui/update.rs">update.rs</RepoFile> currently handles `LaunchMessage::StageFailed` without clearing `build_log_open`, `build_log_scroll_dragging`, or `container_info_open`.
- Existing focused test files to extend:
  - <RepoFile path="crates/jackin-launch-tui/src/tui/update/tests.rs">crates/jackin-launch-tui/src/tui/update/tests.rs</RepoFile>
  - <RepoFile path="crates/jackin-launch-tui/src/tui/subscriptions/tests.rs">crates/jackin-launch-tui/src/tui/subscriptions/tests.rs</RepoFile>
  - <RepoFile path="crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs">crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs</RepoFile>
- If a view render test file does not exist for the exact render helper, add the test to the nearest sibling `tests.rs` that already exercises `render_launch_view` or the component-level render function. Do not create inline tests in `view.rs`.

### Blocker 3 Baseline: CI Wiring

- CI workflow existed before the refactor:
  - `git show origin/main:.github/workflows/ci.yml`
  - Current file: <RepoFile path=".github/workflows/ci.yml">.github/workflows/ci.yml</RepoFile>
- Important distinction: `file-size-gate` is PR-introduced. It does not exist in `origin/main`, so this blocker is not "restore old CI"; it is "finish wiring the new gate so the PR claim is true."
- Useful inspection commands:
  - `rg -n "rust:|schema-check:|file-size-gate:|ci-required:" .github/workflows/ci.yml`
  - `git diff origin/main...HEAD -- .github/workflows/ci.yml`
- Current audited problem:
  - The `rust:` path filter does not include `clippy.toml`, `file-size-budget.toml`, or `test-layout-allowlist.toml`.
  - `ci-required.needs` does not include every gate that must block merge, especially `schema-check` and `file-size-gate`.

### Blocker 4 Baseline: Ratchet Semantics

- These files are PR-introduced:
  - <RepoFile path="crates/jackin-xtask/src/lint.rs">crates/jackin-xtask/src/lint.rs</RepoFile>
  - <RepoFile path="crates/jackin-xtask/src/test_layout.rs">crates/jackin-xtask/src/test_layout.rs</RepoFile>
- Because they are new in PR #664, `origin/main` cannot show the old implementation. Use current source plus tests to verify the audited gap.
- Current audited file-size gap:
  - In <RepoFile path="crates/jackin-xtask/src/lint.rs">crates/jackin-xtask/src/lint.rs</RepoFile>, the match arm `Some(_) => {}` accepts budgeted files that are at or under the recorded high-water mark. That means stale or no-longer-needed rows can pass.
  - Inspect with: `nl -ba crates/jackin-xtask/src/lint.rs | sed -n '147,164p'`
- Current audited test-layout gap:
  - In <RepoFile path="crates/jackin-xtask/src/test_layout.rs">crates/jackin-xtask/src/test_layout.rs</RepoFile>, stale allowlist entries are printed as notes, not errors.
  - Inspect with: `rg -n "stale|note:" crates/jackin-xtask/src/test_layout.rs`
- Existing xtask tests to extend:
  - <RepoFile path="crates/jackin-xtask/src/lint/tests.rs">crates/jackin-xtask/src/lint/tests.rs</RepoFile>
  - <RepoFile path="crates/jackin-xtask/src/test_layout/tests.rs">crates/jackin-xtask/src/test_layout/tests.rs</RepoFile>

## Executor Rules

- Do not add new broad refactors while working this plan. Each edit must map to one blocker below.
- Put Rust tests in sibling `tests.rs` files or existing test files only. Do not add inline `#[test]` functions to production files.
- For each blocker, first add or identify a failing regression test, then implement the smallest fix that makes the test pass.
- After finishing a blocker, remove or check off only that blocker. Do not leave completed work in this plan unless the PR still needs action.

## Blocker 1 — restore `jackin prewarm` CLI invariants

The refactor moved `prewarm` flags into nested structs, but several previous `clap` constraints were dropped. Restore the old invalid-combination behavior before merge.

Why this blocks merge: invalid command combinations are now accepted and silently resolved by later runtime code. This is a behavior regression from the pre-refactor CLI.

- [ ] In <RepoFile path="crates/jackin/src/cli/prewarm.rs">crates/jackin/src/cli/prewarm.rs</RepoFile>, restore these exact `clap` relationships:
  - [ ] `--keep-sidecar-container` has `requires = "sidecar_container"`.
  - [ ] `--role` conflicts with `--workspace` and `--all-workspaces`.
  - [ ] `--workspace` conflicts with `--role` and `--all-workspaces`.
  - [ ] `--role-git` requires `--role` and conflicts with `--workspace` and `--all-workspaces`.
  - [ ] `--all-workspaces` conflicts with `--role`, `--workspace`, `--role-git`, and `--all-roles`.
  - [ ] `--all-roles` requires `--image` and conflicts with `--role`, `--workspace`, `--role-git`, and `--all-workspaces`.
- [ ] Add parse regression tests in <RepoFile path="crates/jackin/src/cli/tests.rs">crates/jackin/src/cli/tests.rs</RepoFile>. Use the existing pattern in that file: call `Cli::try_parse_from([...])`, assert `is_err()` for rejected combinations, and assert `unwrap()` plus field matches for accepted combinations.
  - [ ] Add `rejects_prewarm_keep_sidecar_container_without_sidecar_container`: `["jackin", "prewarm", "--keep-sidecar-container"]` must return an error. The error should mention `--sidecar-container` or the clap-required argument relationship.
  - [ ] Add `rejects_prewarm_role_with_all_workspaces`: `["jackin", "prewarm", "--role", "architect", "--all-workspaces"]` must return an error.
  - [ ] Add `rejects_prewarm_workspace_with_all_workspaces`: `["jackin", "prewarm", "--workspace", "demo", "--all-workspaces"]` must return an error.
  - [ ] Add `rejects_prewarm_role_with_all_roles`: `["jackin", "prewarm", "--image", "--role", "architect", "--all-roles"]` must return an error.
  - [ ] Add `rejects_prewarm_workspace_with_all_roles`: `["jackin", "prewarm", "--image", "--workspace", "demo", "--all-roles"]` must return an error.
  - [ ] Add `rejects_prewarm_role_git_with_all_workspaces`: `["jackin", "prewarm", "--role", "architect", "--role-git", "https://example.invalid/role.git", "--all-workspaces"]` must return an error.
  - [ ] Add `rejects_prewarm_all_roles_without_image`: `["jackin", "prewarm", "--all-roles"]` must return an error.
  - [ ] Add `parses_prewarm_image_all_roles`: `["jackin", "prewarm", "--image", "--all-roles"]` must parse and set `args.flags.image == true` and `args.flags.all_roles == true`.
  - [ ] Add `parses_prewarm_image_role_git`: `["jackin", "prewarm", "--image", "--role", "architect", "--role-git", "https://example.invalid/role.git"]` must parse and set `args.role == Some("architect")`, `args.role_git == Some(...)`, and `args.flags.image == true`.
- [ ] Run `cargo test -p jackin cli::tests::prewarm` or the closest available test filter that runs only the prewarm CLI tests.
- [ ] Run the final verification commands after this blocker and the other blockers are complete.

## Blocker 2 — keep launch failures visible and actionable

The launch TUI can currently enter a failed stage while lower-priority overlays are still open. The failure surface must win so an operator can see and acknowledge the failure.

Why this blocks merge: a failed launch can hide the actionable failure dialog behind the build log overlay. That can make an operator think the launch is still only showing logs rather than failed.

- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/update.rs">crates/jackin-launch-tui/src/tui/update.rs</RepoFile>, update the `LaunchMessage::StageFailed` arm so it clears every lower-priority overlay state that can hide or intercept the failure:
  - [ ] `build_log_open = false`.
  - [ ] `build_log_scroll_dragging = false`.
  - [ ] `container_info_open = false`.
  - [ ] Leave the failure-specific fields initialized exactly as they are today: `failure_ack = false`, failure copy/open/reveal hover state cleared, and `failure = Some(failure)`.
- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/view.rs">crates/jackin-launch-tui/src/tui/view.rs</RepoFile>, make failure rendering defensively higher priority than the build-log overlay. The current early return for `view.build_log_open` must not run before a failure dialog can render.
  - [ ] If `view.failure.is_some()`, render the normal cockpit background with frozen animation and then render the failure dialog.
  - [ ] A stale `view.build_log_open == true` must not hide the failure dialog.
  - [ ] Keep the current container-info behavior lower priority than failure.
- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/subscriptions.rs">crates/jackin-launch-tui/src/tui/subscriptions.rs</RepoFile>, route failure-dialog pointer handling through the modal click classifier:
  - [ ] Inside copy/reveal/open button targets keeps the existing action behavior.
  - [ ] Inside non-target dialog body clicks are swallowed.
  - [ ] Outside clicks dispatch `FailureAcknowledged`, matching the keyboard/button acknowledgement path.
- [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/components/failure_dialog.rs">crates/jackin-launch-tui/src/tui/components/failure_dialog.rs</RepoFile>, make long diagnostics and next-step rows reachable instead of silently truncating them:
  - [ ] Prefer the shared scrollable dialog-body helper if it fits the component.
  - [ ] If not using the shared helper, add explicit scroll state and input handling for the failure body.
  - [ ] Do not increase modal height beyond the existing viewport-safe sizing contract.
- [ ] Add focused tests in sibling `tests.rs` files:
  - [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/update/tests.rs">crates/jackin-launch-tui/src/tui/update/tests.rs</RepoFile>, add a test that starts with `build_log_open = true`, `build_log_scroll_dragging = true`, and `container_info_open = true`; after `StageFailed`, assert all three are false and `failure.is_some()`.
  - [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/view/tests.rs">crates/jackin-launch-tui/src/tui/view/tests.rs</RepoFile> or the nearest existing render test file, add a render test where both `build_log_open = true` and `failure = Some(...)`; assert failure title/body/action text appears and build-log-only content does not own the screen.
  - [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/subscriptions/tests.rs">crates/jackin-launch-tui/src/tui/subscriptions/tests.rs</RepoFile>, add an outside-click test that asserts `LaunchMessage::FailureAcknowledged` is applied or the model ends with `failure_ack == true`.
  - [ ] In the same subscriptions test file, add an inside-dialog non-target click test that proves the click is swallowed and does not trigger background build-log/container-info behavior.
  - [ ] In <RepoFile path="crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs">crates/jackin-launch-tui/src/tui/components/failure_dialog/tests.rs</RepoFile>, add a long-content test using enough diagnostic/next-step text to exceed the old body height; assert the content can be reached by scrolling or that the rendered output exposes the overflow through the chosen scroll mechanism.
- [ ] Run focused launch TUI tests:
  - [ ] `cargo test -p jackin-launch-tui update`
  - [ ] `cargo test -p jackin-launch-tui view`
  - [ ] `cargo test -p jackin-launch-tui subscriptions`
  - [ ] `cargo test -p jackin-launch-tui failure_dialog`

## Blocker 3 — make codebase-health CI gates actually required

The strict gates exist, but the aggregate required job does not currently depend on every gate that should block a merge.

Why this blocks merge: GitHub branch protection can report the stable required check as passing even when the new codebase-health gate is not part of the required aggregate. That means the refactor's enforcement promise is not true yet.

- [ ] In <RepoFile path=".github/workflows/ci.yml">.github/workflows/ci.yml</RepoFile>, update the `changes` job's `rust:` path filter so editing any ratchet/config file runs the Rust gate set:
  - [ ] `clippy.toml`
  - [ ] `file-size-budget.toml`
  - [ ] `test-layout-allowlist.toml`
- [ ] In the same workflow, add these jobs to `ci-required.needs`:
  - [ ] `schema-check`
  - [ ] `file-size-gate`
- [ ] Confirm `ci-required` still uses `if: always()`. Do not remove this: it is what lets the aggregate job report skipped/failed/cancelled dependency state.
- [ ] Confirm the `ci-required` steps still call <RepoFile path=".github/actions/aggregate-needs/action.yml">.github/actions/aggregate-needs/action.yml</RepoFile> or the current aggregate-needs helper. The stable required check must evaluate the full `needs` list.
- [ ] Update stale comments near `file-size-gate` if they still say dependency-direction is informational; the command is already strict and should be described as strict.
- [ ] Run workflow validation:
  - [ ] `actionlint`
  - [ ] Any repo wrapper for workflow linting if one exists.
- [ ] After pushing, confirm `gh pr checks 664 --watch=false` shows real workflow checks for the latest head. It is not enough for DCO alone to pass.
- [ ] After pushing, open the latest GitHub Actions run and confirm the stable aggregate job cannot complete successfully unless `file-size-gate` and `schema-check` are successful or intentionally skipped by the same path-filter policy.

## Blocker 4 — make the ratchets shrink-only

The ledgers are empty today, but the lint implementation still accepts stale budget rows if they return later. The gate should fail when a ratchet row no longer matches a real over-cap exception.

Why this blocks merge: PR #664 claims the codebase-health ledgers cannot grow stale. Today a future stale row can still pass, so the enforcement is weaker than the roadmap/PR claim.

- [ ] In <RepoFile path="crates/jackin-xtask/src/lint.rs">crates/jackin-xtask/src/lint.rs</RepoFile>, make file-size budget entries shrink-only:
  - [ ] Fail if a budget entry points at a file that no longer exists.
  - [ ] Fail if a budget entry points at a measured file that is now at or under the production/test cap; the row must be deleted.
  - [ ] Fail if a budgeted over-cap file records a count higher than the current measured count; the row must shrink to the measured count or the file must be refactored under cap.
  - [ ] Keep the existing failure when a measured file grows beyond its recorded budget.
  - [ ] The failure message must include the repo-relative path and the action the contributor should take: delete the stale row, lower the recorded count, or refactor the file.
- [ ] Update <RepoFile path="crates/jackin-xtask/src/lint/tests.rs">crates/jackin-xtask/src/lint/tests.rs</RepoFile>:
  - [ ] Replace any test that accepts stale/shrunken entries with a rejection test.
  - [ ] Add missing-file budget-entry rejection coverage.
  - [ ] Add under-cap budget-entry rejection coverage.
  - [ ] Add current-count-lower-than-budget rejection coverage.
  - [ ] Keep growth-above-budget rejection coverage.
- [ ] In <RepoFile path="crates/jackin-xtask/src/test_layout.rs">crates/jackin-xtask/src/test_layout.rs</RepoFile>, make stale or missing `test-layout-allowlist.toml` rows fail instead of printing only a note.
- [ ] Update <RepoFile path="crates/jackin-xtask/src/test_layout/tests.rs">crates/jackin-xtask/src/test_layout/tests.rs</RepoFile>:
  - [ ] Add stale allowlist row rejection coverage: an allowlist row whose path is not in the scanned violation map must make `check(...)` return an error.
  - [ ] Add missing allowlist path rejection coverage if the parser can distinguish it from stale; otherwise document in the test name that missing paths are treated as stale rows.
  - [ ] Keep new-violation rejection coverage.
- [ ] Run focused and umbrella xtask verification:
  - [ ] `cargo test -p jackin-xtask lint`
  - [ ] `cargo test -p jackin-xtask test_layout`
  - [ ] `cargo run -p jackin-xtask --locked -- lint --strict`

## Final Verification

Run this only after every blocker above is implemented.

- [ ] `cargo fmt --check`
- [ ] `cargo check --workspace --all-targets --all-features --locked`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] `cargo run -p jackin-xtask --locked -- lint --strict`
- [ ] `cargo nextest run --all-features --no-fail-fast -E 'not test(/shell_session_gets_only_status_socket/)'`
- [ ] Docs gate: `bun run build`, `bun run check:repo-links`, `bun run check:roadmap-sidebar`, `bunx tsc --noEmit`, and `bun test`
- [ ] `gh pr checks 664 --watch=false` shows the latest pushed head has the required checks, not only DCO.
