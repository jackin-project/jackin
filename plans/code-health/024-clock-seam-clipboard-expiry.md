# Plan 024: Phase 3/6 — introduce the `Clock` seam in `jackin-core`; first consumer: clipboard-transfer expiry

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat b42c97d4c..HEAD -- crates/jackin-core/src/ crates/jackin-capsule/src/clipboard.rs crates/jackin-capsule/src/daemon.rs`
> On a mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW-MED (behavior-preserving injection; the default path stays the real clock)
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `b42c97d4c`, 2026-07-09

## Why this matters

The audit's root test-infrastructure blocker (first-wave finding TEST-clock): the workspace has **no injected clock** — a fresh census counts 242 raw `Instant::now()`/`SystemTime::now()`/`Utc::now()` sites and zero `Clock` abstractions — so every expiry, throttle, and deadline behavior is untestable without real sleeps, and the roadmap's whole deterministic-simulation program (Phase 3: turmoil, proptest-state-machine, failpoints; Phase 6 principle "no wall-clock-dependent tests — use an injected clock") is blocked on this seam existing. The ledger's guidance: do NOT sweep all 242 sites; introduce the port and convert **one consumer** end-to-end. The chosen consumer is clipboard-transfer idle expiry in the capsule — small, self-contained, currently unreachable by tests (the deferred TEST-clipboard finding), and its module already half-anticipates injection (`abort_idle_before(cutoff)` takes the cutoff as a parameter).

## Current state

Verified at the planning commit.

- No clock trait exists anywhere (`rg -in 'trait.*clock' crates/ -g '*.rs'` → nothing relevant).
- The port-trait convention this follows: `jackin-core` already hosts injection seams lifted for exactly this purpose — `CommandRunner` (`crates/jackin-core/src/runner.rs:56`), `BuildLogSink` (`build_log_sink.rs:12`), `DebugLogSink` (`debug_log.rs:21`), `OperatorNoticeSink` (`operator_notice.rs:23`). Read `runner.rs` before writing the trait: match its doc style, its module layout (flat file + `pub mod` in lib.rs), and how implementors are shipped alongside the trait.
- The first consumer, `crates/jackin-capsule/src/clipboard.rs` (verified):
  - line 13: `use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};`
  - line 24: `pub(crate) const CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT: Duration = Duration::from_mins(5);`
  - line 47: `last_activity: Instant,` (per-active-transfer state)
  - line 74: `last_activity: Instant::now(),` (transfer start)
  - line 120: `active.last_activity = Instant::now();` (chunk activity)
  - lines 155-158: `pub(crate) fn abort_idle_older_than(&mut self, max_idle: Duration) -> usize { let cutoff = Instant::now() … .unwrap_or_else(Instant::now); … }`
  - line 162: `fn abort_idle_before(&mut self, cutoff: Instant) -> usize` — the cutoff-parameterized inner function (already seam-shaped).
  - line 192: `SystemTime::now()` (used with `UNIX_EPOCH` for staged-file naming — this one is NOT expiry logic; leave it on the real clock, it is content-addressing not time-dependent behavior).
- Call sites of the expiry API: find them with `rg -n 'abort_idle_older_than|CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT' crates/jackin-capsule/src` — expect the daemon tick/housekeeping path plus the transfer-tracking struct's owner. Read each before Step 2.
- Repo conventions: no inline test modules (sibling `tests.rs`); `unsafe_code = forbid`; unwrap/expect/panic denied in production; workspace lint table inherited; per-crate README updated on structural change.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Core check/tests | `cargo clippy -p jackin-core --all-targets -- -D warnings` / `cargo nextest run -p jackin-core` | exit 0 / all pass |
| Capsule check/tests | `cargo clippy -p jackin-capsule --all-targets -- -D warnings` / `cargo nextest run -p jackin-capsule` | exit 0 / all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-core/src/clock.rs` (create) + `clock/tests.rs` (create) + `lib.rs` registration + `crates/jackin-core/README.md` row
- `crates/jackin-capsule/src/clipboard.rs` (+ its `clipboard/tests.rs`) — inject the clock into the transfer-tracking struct and the expiry path
- The daemon call site(s) constructing that struct (minimal constructor change only)
- Roadmap note (Phase 3 sim section: clock seam shipped; Phase 6 determinism principle: first consumer converted)
- `plans/code-health/README.md` row + strike TEST-clock/TEST-clipboard deferred entries as partially planned

**Out of scope**:
- Converting ANY other `now()` site (241 remain — later consumers ride behind this seam one at a time)
- `SystemTime::now()` at clipboard.rs:192 (file naming, not behavior)
- tokio time / `tokio::time::pause` integration (a later consumer's concern; this seam is std-time)
- The git/PR-watch throttle (deferred TEST-git-watch — next consumer candidate, not this plan)
- Any daemon decomposition

## Git workflow

- Branch off `main`: `feat/clock-seam-clipboard`.
- Conventional Commits, `-s`, push per commit. PR to `main`; do not merge. Touches jackin-capsule → capsule smoke block in the PR body (copy from `.github/PULL_REQUEST_TEMPLATE.md`).

## Steps

### Step 1: The `Clock` port in jackin-core

Create `crates/jackin-core/src/clock.rs` following the `runner.rs` port style:

```rust
//! Injected time source. Production code takes a `Clock` (or a `&dyn Clock`)
//! wherever behavior depends on elapsed time, so tests advance time
//! deterministically instead of sleeping. First consumer: capsule clipboard
//! transfer expiry. Do not use for content addressing / file naming — only
//! for behavior that varies with time.

use std::time::Instant;

pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> Instant;
}

/// The real wall clock. The default everywhere outside tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
```

Plus a test clock shipped here (so every future consumer's tests reuse one implementation, not N copies):

```rust
/// Deterministic test clock: starts at an arbitrary epoch, advances only via
/// `advance`. Lives in production code (not cfg(test)) so downstream crates'
/// tests can use it without a test-support feature dance; it is inert unless
/// constructed.
#[derive(Debug)]
pub struct ManualClock { /* Mutex<Instant> or AtomicU64-nanos offset from a base Instant */ }
impl ManualClock {
    pub fn new() -> Self { … }
    pub fn advance(&self, by: std::time::Duration) { … }
}
impl Clock for ManualClock { … }
```

Implementation note: `Instant` is opaque — `ManualClock` stores a base `Instant::now()` captured at construction plus an atomic nanosecond offset; `now()` returns `base + offset`. That keeps the trait's `Instant` type honest without fake time types. `Default for ManualClock` = `new()`. Workspace lints apply: no unwrap/expect (a `Mutex` approach would need lock-poison handling — prefer the atomic offset).

Register `pub mod clock;` in lib.rs; add the README structure row; write `clock/tests.rs`: `SystemClock::now` is monotonic non-decreasing across two calls; `ManualClock::advance` moves `now()` by exactly the delta; two clones/references observe the same advance.

**Verify**: `cargo clippy -p jackin-core --all-targets -- -D warnings` → exit 0; `cargo nextest run -p jackin-core` → all pass incl. new tests.

### Step 2: Inject into clipboard expiry

In `crates/jackin-capsule/src/clipboard.rs`:
1. The transfer-tracking struct (owner of `last_activity`, found via line 47's context) gains a clock: store `Arc<dyn Clock>` (or a generic `C: Clock` if the struct is not otherwise dyn-boxed — read how the daemon holds it and pick the smaller diff; `Arc<dyn Clock>` is the default choice).
2. Replace the three behavior sites: line 74 `Instant::now()` → `self.clock.now()` (or the constructor-passed clock), line 120 likewise, lines 155-158's cutoff computation likewise. `abort_idle_before` stays as-is (already parameterized).
3. Constructor: `new()` keeps existing signature defaulting to `SystemClock` (so daemon call sites change minimally or not at all); add `with_clock(clock: Arc<dyn Clock>)` (or equivalent) for tests. Check the daemon call sites found in recon — if construction is a struct literal rather than `new()`, add the field with a `SystemClock` default via a constructor function and migrate the literal(s).

**Verify**: `cargo clippy -p jackin-capsule --all-targets -- -D warnings` → exit 0; `cargo nextest run -p jackin-capsule` → all existing tests pass (behavior unchanged on the real clock).

### Step 3: The expiry characterization tests (the payoff)

In `crates/jackin-capsule/src/clipboard/tests.rs` (extend the existing sibling file — read its current helpers first and reuse its transfer-construction pattern), add deterministic tests using `ManualClock`:
1. A transfer started, clock advanced by `CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT + 1s`, then `abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT)` → returns 1, transfer gone.
2. A transfer with chunk activity at T+4min (clock-advanced), expiry check at T+5min+1s → NOT aborted (activity reset the idle window); at T+9min+1s → aborted.
3. Two transfers, one active one idle → exactly the idle one aborted.
4. Boundary: idle exactly == timeout → assert whichever behavior the current `<`/`<=` comparison implements (read `abort_idle_before`'s comparison first; the test pins current behavior, it does not choose new behavior).

No sleeps anywhere: `rg -n 'sleep' crates/jackin-capsule/src/clipboard/tests.rs` must stay empty.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/clipboard/)'` → all pass including 4 new tests, total runtime < 1s.

### Step 4: Docs + roadmap + ledger

- `crates/jackin-core/README.md` (done in Step 1) + `crates/jackin-capsule/README.md` only if its structure table lists clipboard (check; update the Owns line if it mentions expiry).
- Roadmap: Phase 3 "Deterministic simulation" intro — note the clock seam exists (`jackin_core::clock`) and names the conversion order (clipboard shipped; git/PR-watch throttle next); Phase 6 item 5 references it.
- `plans/code-health/README.md`: strike TEST-clock (seam shipped, 241 sites remain as rolling conversions) and TEST-clipboard (expiry half covered; image-chunk-assembly tests remain deferred).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo nextest run --workspace --all-features --locked` → all pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- New: `clock/tests.rs` (3 tests) + 4 clipboard expiry tests per Step 3.
- Regression: full capsule suite green with zero edits to existing tests (the default `SystemClock` path is behavior-identical).

## Done criteria

- [ ] `jackin_core::clock::{Clock, SystemClock, ManualClock}` exist with tests
- [ ] clipboard.rs expiry paths read time only through the injected clock (`rg -n 'Instant::now' crates/jackin-capsule/src/clipboard.rs` → 0 matches; line 192's `SystemTime::now` may remain)
- [ ] 4 deterministic expiry tests pass in <1s with no `sleep`
- [ ] Workspace suites + clippy green; `cargo xtask ci --fast` → `ci gate OK`
- [ ] Roadmap + ledger + plan README updated

## STOP conditions

Stop and report back if:

- The transfer-tracking struct is constructed in more than 3 places (injection diff bigger than planned — report the sites).
- `Arc<dyn Clock>` hits an object-safety or Send/Sync wall in the daemon's usage (report; do not switch the daemon to generics unilaterally).
- Any existing clipboard test asserts wall-clock behavior that the injection changes.
- You are tempted to convert other `now()` sites "while here" — one consumer is the scope.

## Maintenance notes

- Conversion order for future consumers (each its own small PR): git/PR-watch throttle (TEST-git-watch), usage-monitor polling, daemon status heartbeats, launch wait-for-state deadlines. Each conversion = swap `Instant::now()` for an injected clock + add the deterministic tests that were impossible before.
- The Phase 3 turmoil/proptest harnesses will inject `ManualClock` (or wrap it); keep `Clock` minimal (one method) until a consumer genuinely needs `SystemTime` — then add a separate method deliberately, not speculatively.
- Reviewer should scrutinize: the `ManualClock` atomic-offset implementation (no Mutex poisoning path, no unwrap), and that daemon construction sites default to `SystemClock` with zero behavior change.
