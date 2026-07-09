# Plan 014: Phase 4 — close the bench gap: compile-check every bench, cover the four unbenchmarked hot paths, add a measured scheduled lane

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- .github/workflows/ci.yml .github/workflows/hygiene.yml crates/*/benches/ crates/jackin-term/src/grid/write.rs crates/jackin-capsule/src/tui/pane_snapshot.rs crates/jackin-diagnostics/src/summary.rs crates/jackin-usage/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (new benches + CI lanes; no production code changes)
- **Depends on**: none (pairs naturally with plan 003, which optimizes `materialize_accounts` — land this bench first or together so the win is measured)
- **Category**: perf
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

Roadmap Phase 4 (lines 198-210) wants measured, non-flaky performance reporting for six hot paths: terminal pane rendering, launch/attach, grid resize, scrollback snapshotting, materialization, diagnostics parsing. Measured reality: five criterion benches exist, but CI compile-checks only two of them (`console_frame`, `pane_body`) — the other three (`present_frame`, `scroll_throughput`, `launch_attach`) are compiled by no workflow, so API drift silently rots them; nothing anywhere *runs* a bench or records a number; and four of the six roadmap hot paths have no bench at all — precisely the ones with recorded perf findings (grid resize / PERF-resize-clone, scrollback snapshot / PERF-scrollback-snapshot, materialization / plan 003, diagnostics JSONL / PERF-diag-double-parse). Until those benches exist, the Phase 4 perf-budget ratchet and any iai-callgrind gate have nothing to measure, and the already-planned optimizations can't prove their wins.

## Current state

- Bench inventory (audit-verified at `47dd5fca0`; all criterion, `harness = false`):
  - `crates/jackin/benches/console_frame.rs` — host-console compose+draw, 220×50. CI-built.
  - `crates/jackin-capsule/benches/pane_body.rs` — pane-body blit comparison. CI-built.
  - `crates/jackin-term/benches/present_frame.rs` — focused-pane present-frame; has an optional `dhat-heap` profiling mode. **Not built by any workflow.**
  - `crates/jackin-term/benches/scroll_throughput.rs` — `scroll_up` line-feed throughput. **Not built by any workflow.**
  - `crates/jackin-runtime/benches/launch_attach.rs` — naming/path/manifest micro-ops only (its header scopes it as a crate-carve baseline; it does **not** drive the launch pipeline). **Not built by any workflow.**
- `.github/workflows/ci.yml` `bench-build` job (lines 855-903, read directly): builds exactly two benches —

  ```yaml
      - run: |
          cargo build --bench console_frame -p jackin
          cargo build --bench pane_body -p jackin-capsule
  ```

  The job's own comment says its purpose is validating bench compilation. `hygiene.yml` (scheduled) contains zero bench references. No workflow runs `cargo bench`. No `iai-callgrind`/`hyperfine`/`critcmp` anywhere (deps, mise.toml, workflows).
- The four unbenchmarked hot paths, each with a recorded finding naming the exact code (from `plans/code-health/README.md` deferred ledger; read each function before writing its bench):
  - Grid resize: `resize_grid` in `crates/jackin-term/src/grid/write.rs:45-59` (PERF-resize-clone — allocates a fresh grid and deep-clones every retained cell on every `set_size`).
  - Scrollback snapshot: `pane_content_from_damagegrid` in `crates/jackin-capsule/src/tui/pane_snapshot.rs:182-194` (PERF-scrollback-snapshot — snapshots the whole scrollback to read a few rows).
  - Usage materialization: `materialize_accounts` in `jackin-usage` (plan 003 optimizes it; that plan's Current state carries the exact path).
  - Diagnostics JSONL parsing: `summarize_reader` / the summary path in `crates/jackin-diagnostics/src/summary.rs:153-206` (PERF-diag-double-parse — parses every line into an owned `serde_json::Value` and re-parses `detail`).
- dhat is already wired and CI-gating as *tests* (`crates/jackin-term/tests/allocation.rs`, `crates/jackin-capsule/tests/render_allocation.rs`, both `#![cfg(feature = "dhat-heap")]`, run by the `--all-features` test job) — do not touch them; the budget-ratchet integration is plan 017 territory.
- Conventions: benches live in `crates/<crate>/benches/<name>.rs` with a `[[bench]] name / harness = false` manifest section — copy the shape of `crates/jackin-term/benches/scroll_throughput.rs` (same crate as two of the new benches); workflows install tools via mise only; PR/main parity rule applies (a bench-build step must behave identically on PR and main).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Build every bench | `cargo build --benches --workspace --locked` | exit 0 |
| Run one bench briefly | `cargo bench --bench <name> -p <crate> -- --quick` | criterion report, exit 0 |
| Crate tests | `cargo nextest run -p <crate>` | all pass |
| Workflow lint | `actionlint .github/workflows/ci.yml .github/workflows/hygiene.yml` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- New bench files: `crates/jackin-term/benches/resize_storm.rs`, `crates/jackin-capsule/benches/scrollback_snapshot.rs`, `crates/jackin-usage/benches/materialize_accounts.rs`, `crates/jackin-diagnostics/benches/summarize_jsonl.rs` (+ the `[[bench]]` sections in those four Cargo.tomls)
- `.github/workflows/ci.yml` `bench-build` job run step (only the `- run:` block)
- `.github/workflows/hygiene.yml` (one new scheduled bench-run job)
- Roadmap Phase 4 status note
- Crate READMEs of the four crates gaining a bench (structure-change rule)

**Out of scope**:
- Any change to production code — benches call existing public/pub(crate) APIs; if an API is not reachable from a bench, STOP.
- iai-callgrind adoption, perf-budget TOML, dhat threshold ratcheting (plan 017 + a later wave)
- hyperfine cold-start lane, input-to-frame latency harness (recorded next wave)
- The existing five benches' content

## Git workflow

- Branch off `main`: `perf/hot-path-bench-coverage`.
- Conventional Commits (`perf(...)`/`ci(...)`), `-s`, push after every commit. PR to `main`; do not merge.

## Steps

### Step 1: Compile-check every bench in CI

Replace the two-line run block in ci.yml's `bench-build` job (lines 901-903) with:

```yaml
      - run: cargo build --benches --workspace --locked
```

This picks up all current and future benches automatically (the job comment above it stays accurate — update its text from "builds both benches" to "builds every workspace bench").

**Verify**: `cargo build --benches --workspace --locked` → exit 0 locally (this also proves the three currently-unbuilt benches still compile — if one does not, STOP and report which); `actionlint .github/workflows/ci.yml` → exit 0.

### Step 2: Grid-resize bench

Read `crates/jackin-term/src/grid/write.rs` around `resize_grid` (lines 45-59) to learn the entry API (likely `Grid::set_size` or similar on the public term surface — find how `crates/jackin-term/benches/scroll_throughput.rs` constructs its grid and reuse that setup). Create `crates/jackin-term/benches/resize_storm.rs` measuring: (a) width+height resize of a grid preloaded with a realistic scrollback (e.g. 2000 rows × 200 cols of text), (b) height-only resize, (c) same-size no-op resize, (d) a "storm" of 20 alternating resizes. Add the `[[bench]]` section.

**Verify**: `cargo bench --bench resize_storm -p jackin-term -- --quick` → completes with timings.

### Step 3: Scrollback-snapshot bench

Read `crates/jackin-capsule/src/tui/pane_snapshot.rs:150-220` to learn `pane_content_from_damagegrid`'s inputs. Create `crates/jackin-capsule/benches/scrollback_snapshot.rs` (model the grid setup on `crates/jackin-capsule/benches/pane_body.rs`): (a) snapshot with a large scrollback (near the 10k-row bound) reading the full range, (b) same grid, narrow range (a few rows), so the future range-scoped API (first-wave PERF-scrollback-snapshot) has a before/after story. Add the `[[bench]]` section.

**Verify**: `cargo bench --bench scrollback_snapshot -p jackin-capsule -- --quick` → completes.

### Step 4: Materialization + diagnostics-parse benches

- `crates/jackin-usage/benches/materialize_accounts.rs`: locate `materialize_accounts` in `crates/jackin-usage/src/` (plan 003's subject). Bench it over a synthetic store with ~50 accounts × realistic usage rows. If constructing its input requires the turso store, use the crate's existing test helpers (`jackin-usage/src/usage/tests.rs` shows how state is built); if only an async/DB-coupled path exists, bench through a tokio `Runtime::block_on` with an in-memory store — and if even that requires real I/O, STOP and report the API shape.
- `crates/jackin-diagnostics/benches/summarize_jsonl.rs`: read `crates/jackin-diagnostics/src/summary.rs:120-220` for the reader entry point (`summarize_reader` or the public wrapper). Bench parsing a generated ~20MB JSONL buffer (build it in the bench setup from a repeated realistic event line — copy a real line shape from `summary.rs` tests if present) via `Cursor<Vec<u8>>`.

**Verify**: both `cargo bench --bench <name> -p <crate> -- --quick` runs complete.

### Step 5: Scheduled measured lane

Add a `bench-run` job to `.github/workflows/hygiene.yml` (schedule + dispatch only, advisory — mirror the checkout/mise/rust-cache steps of the existing `scheduled-hygiene` job, install args `"rust"`):

```yaml
      - run: cargo bench --workspace --locked -- --quick 2>&1 | tee bench-output.txt
      - name: Upload bench results
        uses: actions/upload-artifact@<same pinned SHA as elsewhere in the file — copy it>
        with:
          name: criterion-results-${{ github.run_id }}
          path: |
            bench-output.txt
            target/criterion/**/estimates.json
          if-no-files-found: warn
```

Advisory: the job must not gate anything (`continue-on-error: false` is fine — a compile failure SHOULD fail, but no threshold comparison happens here). Numbers become budgets only via plan 017's ratchet engine after variance is observed.

**Verify**: `actionlint .github/workflows/hygiene.yml` → exit 0.

### Step 6: READMEs + roadmap

Add/extend the "How to verify" or structure section of the four touched crates' READMEs with their bench name. Roadmap Phase 4: mark item 1 as started (bench coverage complete for the six hot paths; measured scheduled lane advisory), note that budgets/iai remain open.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- Benches are the deliverable; each must complete `-- --quick` locally.
- `cargo nextest run -p jackin-term -p jackin-capsule -p jackin-usage -p jackin-diagnostics` stays green (benches must not disturb crate tests).
- `cargo build --benches --workspace --locked` green — the standing compile gate.

## Done criteria

- [ ] ci.yml bench-build compiles all benches via `--benches --workspace`
- [ ] Four new benches exist, each covering its recorded hot path, each completing `-- --quick`
- [ ] hygiene.yml has the advisory bench-run job uploading criterion artifacts
- [ ] All six roadmap hot paths now have ≥1 bench (pane render: existing 3; launch/attach: existing micro-op bench — full-pipeline bench recorded as open in the roadmap note)
- [ ] Four crate READMEs updated; roadmap Phase 4 status updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Any of the three currently-unbuilt benches (`present_frame`, `scroll_throughput`, `launch_attach`) fails to compile in Step 1 — that is pre-existing rot to report, not to fix silently here.
- A hot-path function is not reachable from a bench without production-code changes (no public/test-support constructor for its input).
- `materialize_accounts` requires live DB/network I/O even through test helpers.
- Criterion `--quick` runs exceed ~5 minutes per bench (setup too heavy — shrink the fixture, and if that guts the measurement, report).

## Maintenance notes

- Plan 017's ratchet engine later consumes these numbers as perf budgets (iai-callgrind instruction counts are the intended PR-gate form; criterion stays the local/scheduled harness). Keep bench names stable — they become budget keys.
- Plans 003 (materialize) and the deferred resize/snapshot/diagnostics optimizations should each cite their bench's before/after in their PR — reviewers should demand it.
- Reviewer should scrutinize: bench fixture realism (a trivial grid makes the resize bench meaningless) and that `--benches --workspace` didn't materially slow the bench-build job (it now compiles bench deps for four more crates).
