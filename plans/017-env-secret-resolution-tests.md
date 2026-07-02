# Plan 017: Cover the `jackin-env` secret-resolution path and widen the e2e path-filter to reach it

> **Executor instructions**: Test-coverage plan on a credential path. Run every verification command.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-env/src .github/workflows/ci.yml`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW (adds tests + CI filter only)
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The launch-time secret/credential resolution — the code that fetches secrets from `op` and materializes
operator/on-demand env per container launch — ships with **near-zero** regression coverage, and the CI
`docker_e2e` path-filter doesn't include `crates/jackin-env/**`, so a PR touching only `jackin-env` runs
neither adequate unit tests nor the Docker e2e lane. A bug that forwards an empty/wrong credential,
mis-scopes a secret across roles, or mishandles an `op` timeout/multi-account case would ship silently on
the exact path the product exists to get right.

## Current state

- `crates/jackin-env/src/op_cli.rs` — 843 lines wrapping the `op` subprocess; its test file
  `crates/jackin-env/src/op_cli/tests.rs` is **10 lines / 1 test** (asserts a timeout constant).
- `crates/jackin-env/src/resolve.rs` — the launch-time secret-injection functions have **0 test callers**
  workspace-wide: `resolve_operator_env` (`:370`), `collect_on_demand_bindings` (`:416`),
  `lookup_operator_env_raw` (`:347`), `has_operator_env` (`:320`); the file declares no `mod tests`.
- `validate_reserved_names` (`resolve.rs:18`) has no direct unit test (only covered one layer up in
  `jackin-config`).
- **A test seam already exists**: `crates/jackin-env/src/test_support.rs` (203 lines) provides a fake
  runner (`OpStructRunner`/`CommandRunner`) — use it; never call the real `op` binary.
- CI filter gap: `.github/workflows/ci.yml` `docker_e2e` paths list includes `jackin-usage/**`,
  `jackin-instance/**`, `jackin-runtime/**`, etc. but **not** `jackin-env/**`, `jackin-config/**`,
  `jackin-protocol/**`, or `jackin-console/**` (verified at ci.yml ~lines 109-133).

Conventions: tests in a sibling `tests.rs`, never inline; `cargo nextest` only.

## Scope

**In scope:**
- `crates/jackin-env/src/resolve.rs` (add `#[cfg(test)] mod tests;` declaration) + new
  `crates/jackin-env/src/resolve/tests.rs`
- `crates/jackin-env/src/op_cli/tests.rs` (expand)
- `.github/workflows/ci.yml` (add `crates/jackin-env/**` — and `jackin-config`/`jackin-protocol` — to the
  `docker_e2e` paths filter)

**Out of scope:** changing the resolver *logic* (this is coverage, not a refactor); the `op` binary itself.

## Steps

### Step 1: Unit-test `resolve_operator_env` / `collect_on_demand_bindings` via the fake runner

Add `crates/jackin-env/src/resolve/tests.rs` (and `#[cfg(test)] mod tests;` in `resolve.rs`). Using the
`test_support.rs` fake runner, cover:
- **op-miss**: a referenced `op://` item not found → resolver returns the aggregated error, not a silent empty.
- **timeout**: the fake runner simulates an `op` timeout → error surfaced (not a blank value).
- **empty value**: `op` returns an empty string → assert the resolver's intended handling (error vs empty —
  read the code and lock in current correct behavior as the regression).
- **multi-account**: two `op://` refs pinning different accounts resolve independently (the `OpRef::account`
  rebind path).
- **role scoping**: a secret scoped to role A is not resolved for role B.

### Step 2: Directly unit-test `validate_reserved_names`

Add a test asserting reserved env names are rejected and non-reserved pass — at the `jackin-env` layer,
not only via `jackin-config`.

### Step 3: Expand `op_cli` tests

Cover the `op read` argument construction (account flag present/absent) and the retry seam (`spawn_op_with_retry`)
for the `TEXT_FILE_BUSY` retry class, via the fake runner. (This dovetails with plan 001's `--`/`op://`
guard test — coordinate if both land together.)

### Step 4: Widen the CI e2e path-filter

In `.github/workflows/ci.yml`, add to the `docker_e2e` paths list:
`- 'crates/jackin-env/**'`, `- 'crates/jackin-config/**'`, `- 'crates/jackin-protocol/**'`. Keep list
formatting/ordering consistent with the surrounding entries.

**Verify**: `cargo nextest run -p jackin-env` → all pass (new tests included);
`actionlint .github/workflows/ci.yml` (via mise, if available) → exit 0, or at least YAML parses.

## Test plan

- New file `crates/jackin-env/src/resolve/tests.rs` with the cases in Step 1–2.
- Expanded `op_cli/tests.rs` for Step 3.
- Pattern to follow: `crates/jackin-env/src/test_support.rs` usage + any existing `jackin-config` reserved-name
  test for shape.
- Verification: `cargo nextest run -p jackin-env` passes with ≥8 new tests.

## Done criteria

- [ ] `resolve.rs` declares `#[cfg(test)] mod tests;` and `resolve/tests.rs` exists with op-miss/timeout/
      empty/multi-account/role-scope cases
- [ ] `validate_reserved_names` has a direct unit test
- [ ] `op_cli/tests.rs` covers arg construction + retry
- [ ] `.github/workflows/ci.yml` `docker_e2e` filter includes `jackin-env`, `jackin-config`, `jackin-protocol`
- [ ] `cargo nextest run -p jackin-env` green; `cargo clippy -p jackin-env -- -D warnings` exits 0
- [ ] No test invokes the real `op` binary (`grep -rn "Command::new" crates/jackin-env/src/*/tests.rs` → none)
- [ ] `plans/README.md` row updated

## STOP conditions

- The fake-runner seam can't express one of the cases (e.g. timeout) without invoking real `op` — report;
  extend `test_support.rs` minimally rather than shelling out.
- Adding the crates to the e2e filter makes CI run the Docker e2e lane on every `jackin-env` PR and that's
  deemed too heavy — then gate more narrowly (e.g. only on launch-reachable subpaths) and note the choice.

## Maintenance notes

- Reviewer: confirm no real credential values appear in fixtures — use synthetic `op://vault/item/field`
  refs and fake runner responses only.
- This crate is the credential path; new resolver code should come with a `resolve/tests.rs` case going
  forward — call that out in review.
