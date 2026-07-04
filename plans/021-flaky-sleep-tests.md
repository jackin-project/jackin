# Plan 021: De-flake the wall-clock-sleep tests

> **Executor instructions**: Test-reliability plan. Preserve what each test actually guards. Run every
> verification command. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-instance/src/auth/tests.rs crates/jackin-usage/src/usage/tests.rs`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (naively removing timing can mask the race/idempotency the test guards)
- **Depends on**: none
- **Category**: tests
- **Completed at**: PR #713
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

Several tests use real wall-clock `sleep`, the classic nextest flake source. The worst two: a **75ms/250ms
sleep gating a `max_active` concurrency assertion** in `jackin-usage` (timing-dependent under CI load — can
false-fail or false-pass), and a **1100ms mtime-idempotency sleep** in `jackin-instance/auth` (inflates
suite time). ~13 real-sleep sites total across launch/image/socket/persist/console tests. They're already
`#[expect(clippy::disallowed_methods)]`-annotated, so they're known exceptions — this plan converts the
two highest-value ones to deterministic waits and audits the rest.

## Current state

- `crates/jackin-instance/src/auth/tests.rs:2211` — `sleep(1100ms)` for an mtime-idempotency boundary (the
  single slowest test).
- `crates/jackin-usage/src/usage/tests.rs:780,819` — `sleep(75ms)`/`sleep(250ms)` gating a `max_active`
  concurrency assertion.
- ~13 real-sleep sites total (`grep -rn "sleep(" crates/*/src/**/tests.rs`).

## Scope

**In scope:** `crates/jackin-usage/src/usage/tests.rs` (concurrency test), `crates/jackin-instance/src/auth/tests.rs`
(mtime idempotency test), and — only if it's a clean seam — a controllable-clock helper. **Out of scope:**
the production code's timing behavior; wholesale removal of every sleep (audit them, fix the two that matter).

## Steps

### Step 1: Replace the concurrency sleep with a deterministic barrier

In `usage/tests.rs`, the `max_active` assertion currently *hopes* 75ms is enough to force overlap. Replace
the sleep-based overlap with a synchronization primitive that **deterministically** forces the intended
concurrency: e.g. a `tokio::sync::Barrier` (or a channel/`Notify`) that holds each task until N have
entered, so the `max_active` observation is guaranteed, not timing-dependent. The assertion's *intent*
(max N active) must be preserved exactly.

**Verify**: run the test 20× to confirm no flake:
`for i in $(seq 20); do cargo nextest run -p jackin-usage -E 'test(/max_active/)' || break; done` → all pass.

### Step 2: Make the mtime-idempotency test use a controllable clock (or a forced mtime)

In `auth/tests.rs:2211`, the 1.1s sleep exists to cross a filesystem-mtime second boundary. Instead of
real-time sleeping, force the mtime delta deterministically — set the file's mtime explicitly (e.g. via
`filetime`/`std::fs` set-times) to simulate the boundary, or inject a controllable clock if the code under
test reads time through a seam. Preserve exactly what the test guards (idempotent re-seed does not rewrite
when nothing changed).

**Verify**: the test no longer sleeps ~1.1s (`grep -n "sleep(" crates/jackin-instance/src/auth/tests.rs`
shows the site removed); `cargo nextest run -p jackin-instance -E 'test(/mtime|idempoten/)'` → pass.

### Step 3: Audit the remaining ~11 sleep sites

For each remaining `sleep(` in a `tests.rs`, add a one-line note in this plan's row: is it (a) genuinely
needed (a real timing boundary), or (b) convertible to a barrier/paused-clock later? Do **not** convert
them all now — just triage so the debt is visible.

Remaining sleep audit after this change:

- `crates/jackin-usage/src/usage/tests.rs`: 250ms worker sleep intentionally exercises timeout fallback;
  keep until the timeout collector has an injectable clock/receiver seam.
- `crates/jackin-capsule/src/socket/tests.rs`: 20ms accept-loop settling wait is convertible later to an
  explicit server-side signal.
- `crates/jackin-tui/src/runtime/tests.rs`: 1ms OS-worker poll is a bounded worker scheduling wait; low
  priority, convertible to a worker-ready signal later.
- `crates/jackin/tests/manager_flow.rs`: 1ms config-save worker poll is a bounded integration wait;
  convertible to an explicit background-event notification later.

## Done criteria

- [x] `usage` concurrency test uses a deterministic barrier; passes 20× consecutively
- [x] `auth` mtime test no longer real-sleeps ~1.1s; still asserts idempotency
- [x] Remaining sleep sites triaged in the row note (needed vs convertible)
- [x] `cargo nextest run -p jackin-usage -p jackin-instance` green
- [x] `plans/README.md` row updated

## STOP conditions

- Removing the timing from the mtime test can't preserve what it guards (the code genuinely depends on a
  real clock boundary and offers no injection seam) — report; a clock-injection refactor of the production
  code is a bigger, separate change.
- The concurrency barrier changes the test's meaning (it no longer exercises real overlap) — report before
  weakening the assertion.

## Maintenance notes

- Keep the `#[expect(clippy::disallowed_methods)]` annotations only where a real sleep genuinely remains
  after triage; remove them where the sleep is gone.
- Reviewer: confirm the barrier version still fails if the production concurrency cap regresses (i.e. the
  test can still catch the bug it was written for).
