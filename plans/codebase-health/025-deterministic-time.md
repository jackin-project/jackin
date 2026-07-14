# Plan 025: Deterministic time — wall-clock seam + first boundary conversions

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-core/src/clock.rs crates/jackin-image/src/agent_binary.rs crates/jackin-runtime/src/runtime/repo_cache.rs crates/jackin-env/src/token_setup.rs crates/jackin-capsule/src/daemon/session_lifecycle.rs`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: M (seam + 4 boundaries; further conversions roll on)
- **Risk**: MED (timestamp semantics)
- **Depends on**: none
- **Category**: tests (determinism)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Characterization item 8: "Production wall-clock reads remain outside the injected `jackin_core::clock` seam. Convert them one boundary at a time, beginning with observable expiry, retry, and lifecycle behavior, and add a focused regression test for each conversion." The seam today is `Instant`-only with exactly ONE production consumer (capsule clipboard expiry), while the named first-priority classes read the wall clock directly: image-cache TTL (`SystemTime::now()`), repo-cache TTL, token expiry (`chrono::Utc::now()`), and session lifecycle timestamps. Because the seam can't even express epoch time, those sites CANNOT route through it — tests for expiry behavior use real-time offsets and stay non-deterministic.

## Current state

- Seam: `crates/jackin-core/src/clock.rs:14-27` — `trait Clock { fn now(&self) -> Instant }` + `SystemClock`; header (`clock.rs:4-5`) warns the seam is for behavior, not file naming/content-addressing. Sole consumer: `crates/jackin-capsule/src/clipboard.rs`.
- First-conversion targets (verified sites):
  - `crates/jackin-image/src/agent_binary.rs:861` — `SystemTime::now()` vs `CACHE_TTL`; tests offset real time (`agent_binary/tests.rs:145`).
  - `crates/jackin-runtime/src/runtime/repo_cache.rs:386` — cache TTL.
  - `crates/jackin-env/src/token_setup.rs:1034,1144` — `chrono::Utc::now()` token expiry.
  - `crates/jackin-capsule/src/daemon/session_lifecycle.rs:321,332,471` — `Utc::now()` for `started_at`/`exited_at`.
- Broader census (for the doc, not for conversion here): ~17 non-test `SystemTime::now()`, ~10 `chrono` now sites, ~60 non-test `Instant::now()` across production crates.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Core | `cargo nextest run -p jackin-core` | pass |
| Converted crates | `cargo nextest run -p jackin-image -p jackin-runtime -p jackin-env -p jackin-capsule` | pass |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: extend `clock.rs` with a wall-clock face (`fn now_system(&self) -> SystemTime` on the trait or a sibling `WallClock` trait + `ManualClock` test double supporting both); convert exactly the four boundaries above, each with a focused `ManualClock` regression test; a short census note (remaining unconverted sites + suggested order) appended to the plan's PR description.

**Out of scope**: `Instant::now()` perf-timing sites (not behavior); file-naming/content-addressing timestamps (seam header forbids); converting everything (one boundary at a time is the roadmap's own instruction).

## Git workflow

Branch `test/wall-clock-seam`; Conventional Commits; `git commit -s`; push per commit (seam commit, then one commit per boundary).

## Steps

### Step 1: Seam extension

Extend the seam (keep `Instant` API intact for clipboard): add epoch access; `ManualClock` (new or extended — check if a test clock already exists in test-support or clipboard tests) supports advancing both monotonic and wall time. Decide `chrono` interop at the seam edge: seam returns `SystemTime`; converting sites map to `DateTime<Utc>` at the boundary (`DateTime::<Utc>::from(system_time)`) so `jackin-core` gains no chrono dependency (check first whether it already has one).

**Verify**: `cargo nextest run -p jackin-core -p jackin-capsule` → pass (clipboard untouched).

### Step 2: Convert the four boundaries (one commit each)

Per boundary: inject the clock (constructor param or existing DI seam — match how clipboard receives it), replace the direct read, add the regression test: e.g. agent-binary cache — `ManualClock` advances past `CACHE_TTL` → refetch decision flips, without real sleeps or real-time offsets; token expiry — expiry boundary exact-second behavior; session lifecycle — `started_at`/`exited_at` reflect injected time (also unlocks deterministic lifecycle assertions in daemon tests).

**Verify per boundary**: owning crate suite passes; the old real-time-offset test replaced or supplemented by the ManualClock test; `grep -n "SystemTime::now\|Utc::now" <file>` → gone from the converted paths.

### Step 3: Census note + gates

Append the remaining-sites census (file:line + class) to the PR; full gates.

**Verify**: `cargo clippy --workspace … -D warnings` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

One focused ManualClock regression test per converted boundary (the roadmap's per-conversion requirement); existing suites guard everything else.

## Done criteria

- [x] Seam expresses wall-clock time; ManualClock drives both faces
- [x] Four boundaries converted, each with its deterministic regression test
- [x] No direct wall-clock reads remain in the four converted paths (grep-proven)
- [x] Census of remaining sites recorded in the PR
- [x] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- A target site turns out to feed file naming/content-addressing (seam header forbids conversion) — skip it, record why.
- Injecting the clock into session_lifecycle collides with in-flight plan 017 subsystem moves — rebase or defer that boundary.
- The chrono interop decision would add chrono to `jackin-core` — check tiers/deps first; if unavoidable, STOP and propose.

## Maintenance notes

- New expiry/retry/lifecycle code takes a `Clock` from birth — reviewer rule.
- Remaining census sites convert boundary-by-boundary with the same recipe; the census note is the queue.
