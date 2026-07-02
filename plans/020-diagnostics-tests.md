# Plan 020: Characterization tests for `jackin-diagnostics` render/formatting

> **Executor instructions**: Test-coverage plan. Run every verification command. Update `plans/README.md`.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-diagnostics/src`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`TESTING.md` makes the diagnostics JSONL the canonical debugging path ("agent reads JSONL to localize
issue, not pasted scrollback"). Yet the crate is the weakest-tested operationally-relevant crate
(~73 lines/test): `summary.rs` (400 lines) has **0 tests**, and `terminal.rs`, `operator_notice.rs`,
`debug_log.rs` have no co-located tests. Bugs in run-summary rendering or operator-notice formatting
degrade the one tool operators/agents rely on when a launch misbehaves — and would surface only in the
field.

## Current state

- `crates/jackin-diagnostics/src/summary.rs` (400 lines) — no `summary/tests.rs`.
- `terminal.rs`, `operator_notice.rs`, `debug_log.rs` — no co-located tests.
- `observability.rs` (1203 lines) leans on `observability/otlp/tests.rs` (147) + crate `tests.rs` (673).
- Conventions: `#[cfg(test)] mod tests;` in the source file, tests in sibling `tests.rs`.

## Scope

**In scope:** new `summary/tests.rs`, `operator_notice/tests.rs`, `debug_log/tests.rs` (and the
`#[cfg(test)] mod tests;` declarations in those source files). **Out of scope:** changing the diagnostics
*format/logic* (characterization tests lock in **current** behavior; if you find a bug, report it, don't
silently "fix" it into the test).

## Steps

### Step 1: Characterize `summary.rs`

Add `summary/tests.rs`. Build a representative run record (find the input type via
`grep -n "pub fn\|pub struct\|impl " crates/jackin-diagnostics/src/summary.rs`) and assert the rendered
summary output for: a clean run, a run with a failed stage, and a run with warnings. Snapshot with `insta`
if the crate already uses it (`grep -rn "insta" crates/jackin-diagnostics`), else assert on substrings.

### Step 2: Characterize `operator_notice.rs` and `debug_log.rs`

Assert the formatting of an operator notice and a debug-log line for representative inputs — enough that a
formatting regression fails a test rather than shipping.

**Verify**: `cargo nextest run -p jackin-diagnostics` → all pass (new tests included);
`cargo clippy -p jackin-diagnostics -- -D warnings` → exit 0.

## Test plan

- `summary/tests.rs`: clean / failed-stage / warning-present render cases.
- `operator_notice/tests.rs`, `debug_log/tests.rs`: representative formatting cases.
- Pattern: existing `crates/jackin-diagnostics/src/observability/otlp/tests.rs` for structure; `insta` if used.

## Done criteria

- [ ] `summary.rs`, `operator_notice.rs`, `debug_log.rs` each have a sibling `tests.rs` with ≥1 meaningful test
- [ ] Tests assert on real rendered output, not `assert!(true)` placeholders
- [ ] `cargo nextest run -p jackin-diagnostics` green; crate lines/test ratio improved
- [ ] `plans/README.md` row updated

## STOP conditions

- A characterization test reveals current output is actually wrong (e.g. a summary miscounts stages) —
  report it as a bug finding; don't encode the wrong output as "expected".

## Maintenance notes

- These are characterization (golden) tests; when the diagnostics format is *intentionally* changed, update
  the expected output deliberately (or re-accept `insta` snapshots), and a reviewer should confirm the
  change was intended.
