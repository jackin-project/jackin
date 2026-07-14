# Plan 026: Measured performance completion — missing benches, live allocation lane, first-frame harness, build-time budgets

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done. This plan is four separable slices; land them as separate PRs if the operator prefers.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-config/ crates/jackin-manifest/ crates/jackin-usage/benches/ crates/jackin-term/benches/ crates/jackin-xtask/src/ratchet.rs .github/workflows/hygiene.yml ratchet.toml`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P3
- **Effort**: L total (slices: S/M/M/L)
- **Risk**: LOW-MED (measurement infra; budgets advisory-first)
- **Depends on**: none
- **Category**: perf (measurement)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap "Measured performance and feedback speed" items 1–3 leave five measured gaps: (1) config/manifest resolution — run on every `jackin load` — has no Criterion bench (the other named hot paths do); (2) the dhat allocation tests are feature-gated behind `dhat-heap` which NO workflow builds, so they never execute, and the `perf` ratchet parses budget constants textually from `perf_budgets.rs` rather than ratcheting measured output; (3) the roadmap-named jackin-usage benchmarks (whole-file token-log rereads, per-row snapshot-DB upserts) and the `preserve_visible_rows_to_scrollback` miss bench don't exist, so their "if material" optimization decisions can't be made; (4) cold-start CI measures `--help` only — no deterministic headless/PTY first-frame or input-to-frame measurement exists (the workflow comment admits it); (5) build-time measurements are produced on schedule but feed no ratchet budget.

## Current state

- Benches inventory: `benches/` exist in jackin-capsule, jackin-diagnostics, jackin-runtime, jackin-term, jackin-usage, jackin — NOT config/manifest. jackin-usage has only `materialize_accounts.rs` (shows the hermetic temp-DB pattern + `#[doc(hidden)]` path seam to copy). jackin-term has `present_frame.rs`, `resize_storm.rs` ("`set_size` never touches `self.scrollback`"), `scroll_throughput.rs`.
- Dead allocation tests: `crates/jackin-term/tests/allocation.rs:1`, `crates/jackin-capsule/tests/render_allocation.rs` — `#![cfg(feature = "dhat-heap")]`; `grep dhat-heap .github/workflows/` → nothing. Ratchet provider `measure_perf_dhat_budgets` (`crates/jackin-xtask/src/ratchet.rs:419`) parses `const FOCUSED_…` from `crates/jackin-capsule/src/perf_budgets.rs:7`.
- Roadmap-named bench targets: token-log reread (`crates/jackin-usage/src/token_monitor.rs:216` — "Token logs are re-read whole each recompute pass"); upserts (`crates/jackin-usage/src/telemetry_store.rs:278` `upsert_account_snapshot_rows`); scrollback preserve (`crates/jackin-term/src/grid.rs:1293`, called `:1449,:1470`). Roadmap also names: borrowed account materialization (bench exists), resize-storage reuse, dependency-safe isolated-mount parallelism, width-change reflow product decision — measure-before-change items; record them as open questions, not work here.
- Cold start: `.github/workflows/hygiene.yml:203` hyperfine on `jackin --help` / `jackin console --help` only ("Measures process start + --help only — not TUI first frame"). Existing PTY tooling: `crates/jackin-xtask/src/pty_fixture.rs` is a byte-stream extractor, not a timing harness.
- Build times: `hygiene.yml:358-397` → `build-times.json`, advisory (`TESTING.md:172`); no `build-time` ratchet family.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Bench build | `cargo bench -p <crate> --no-run` | builds |
| Bench test-mode | `cargo bench -p <crate> -- --test` | runs |
| Allocation lane | `cargo test -p jackin-term -p jackin-capsule --features dhat-heap` (exact invocation per step 2) | runs, emits stats |
| Ratchet | `cargo xtask lint ratchet` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (slice per letter):
- A: `crates/jackin-config/benches/config_resolve.rs`, `crates/jackin-manifest/benches/manifest_validate.rs` (+ Cargo bench tables).
- B: an xtask/scheduled-CI allocation lane that RUNS the dhat suites and emits measured numbers; ratchet `perf` family fed from that output (constants demoted to in-test guardrails); OR, if runner constraints block it, an explicit written static-budget policy in TESTING.md + the lane running the tests without ratcheting — the roadmap allows either, but the choice must be recorded.
- C: `crates/jackin-usage/benches/{token_log_reread,snapshot_upsert}.rs`, `crates/jackin-term/benches/preserve_scrollback.rs`.
- D: first-frame/input-to-frame headless PTY harness (xtask lane spawning the console under a PTY, measuring first painted frame and keypress-to-repaint; advisory artifact first, budget after repeatability proven); build-time ratchet family consuming the scheduled `build-times.json` (enforced on the scheduled lane, tolerance-banded).

**Out of scope**: acting on any measurement (incremental reads, clone removal, reflow decision — each is a follow-up gated on this plan's numbers); iai-callgrind (pinned).

## Git workflow

Branches `perf/bench-<slice>`; Conventional Commits (`perf:`/`test:`/`ci:`); `git commit -s`; push per commit.

## Steps

### Step A: Config/manifest benches

Criterion benches over representative fixtures (reuse migration-corpus fixtures for config; a real role manifest fixture for validation). Mirror `summarize_jsonl.rs` bench structure.

**Verify**: `cargo bench -p jackin-config -p jackin-manifest -- --test` → runs.

### Step B: Live allocation lane

Add a scheduled hygiene job (or xtask lane invoked there) building + running the two dhat suites with `--features dhat-heap`, emitting measured blocks/bytes to an artifact; keep it non-blocking initially. Then either (i) repoint `measure_perf_dhat_budgets` at the measured artifact, or (ii) record the static-budget policy explicitly in TESTING.md — decide by whether scheduled measurement proves stable across 3 runs (note results in PR).

**Verify**: lane runs green in `workflow_dispatch`; decision recorded; `cargo xtask lint ratchet` → exit 0.

### Step C: The three named micro-benches

Token-log reread: synthetic logs at several sizes → recompute pass cost (exposes the O(n·rereads) growth). Snapshot upsert: temp Turso DB, per-row vs batched. Scrollback preserve: deep grid, genuine miss path (read `grid.rs:1293` callers to construct misses). Record baseline numbers in the PR — they are the "if material" decision input.

**Verify**: `cargo bench -p jackin-usage -p jackin-term -- --test` → runs.

### Step D: First-frame harness + build-time family

Harness: xtask lane using a PTY (the `portable-pty`/`nix` dep the repo already uses for capsule PTYs — check `crates/jackin-capsule/Cargo.toml`) to spawn `jackin console` headless with a fixed config fixture, timestamping first full frame (detect via the console's alt-screen entry + first complete paint — the pty_fixture byte patterns show what frames look like) and a keypress-to-repaint delta; emit JSON artifact; run in scheduled hygiene. Build-time: add `build_time_budgets` ratchet provider reading `build-times.json` with a tolerance band, enforced only in the scheduled lane.

**Verify**: harness produces stable numbers across 3 local runs (note variance); `actionlint` clean; `cargo xtask lint ratchet` → exit 0.

## Test plan

Benches run in `--test` mode in CI-fast (Criterion smoke); allocation lane + harness verified by observed scheduled/dispatch runs; provider unit tests for the two new ratchet providers.

## Done criteria

- [ ] Config + manifest benches exist and run
- [ ] Allocation tests actually execute in a CI lane; ratchet-vs-static decision recorded (and implemented accordingly)
- [ ] Token-log, upsert, scrollback-preserve benches exist with recorded baselines
- [ ] First-frame/input-to-frame harness emits artifacts on schedule; build-time ratchet family live (scheduled enforcement)
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- dhat under CI runners proves >2× flaky variance — take the static-policy arm, record it, stop tuning.
- PTY first-frame detection can't be made deterministic (no stable frame sentinel) — deliver the harness as measurement-only with documented variance; budget-setting deferred.
- Turso temp-DB benches hit the same `#[doc(hidden)]` seam limits — extend the seam in jackin-usage (in scope) but STOP if it requires public API changes.

## Maintenance notes

- Each recorded baseline unlocks its roadmap decision (incremental reads, batched statements, clone removal, reflow) — file follow-ups with numbers attached.
- Budgets ratchet shrink-only once repeatability is proven; never budget from a single run.
