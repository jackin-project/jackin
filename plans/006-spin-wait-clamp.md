# Plan 006: Clamp `spin_wait`'s inter-attempt delay so sub-80ms intervals still throttle

> **Executor instructions**: Small, self-contained fix. Run every verification command. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-runtime/src/spin_wait.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (but plan 012 builds on this — do 006 first)
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03
- **Implemented choice**: decoupled poll delay from spinner cadence, so sub-80ms intervals sleep the requested interval instead of over-waiting at 80ms.

## Why this matters

`spin_wait` computes the number of animation frames between poll attempts as
`interval.as_millis() / SPIN_MS` with `SPIN_MS = 80`. For any `interval < 80ms` this is `0`, so the
sleep loop never runs: `poll()` is retried with **zero delay**, burning through all `max_attempts`
near-instantly and giving an effectively instant "timeout" instead of the intended
`max_attempts × interval` wait. Currently latent — both callers pass ≥500ms — but plan 012 (readiness
probe) wants a short polling interval, and any sub-80ms caller would silently busy-loop. Fixing this
first makes 012 safe.

## Current state

`crates/jackin-runtime/src/spin_wait.rs:59-72`:
```rust
let spins = interval.as_millis() as u64 / SPIN_MS;   // == 0 when interval < 80ms
for _ in 0..spins {
    if !suppressed { /* draw spinner frame */ }
    tokio::time::sleep(std::time::Duration::from_millis(SPIN_MS)).await;
    frame_idx += 1;
}
```
`SPIN_MS = 80`. Callers today: `attach.rs:117` (500ms) and `attach.rs:1089` (1s).

## Scope

**In scope:** `crates/jackin-runtime/src/spin_wait.rs` and its `tests.rs` (create if absent).
**Out of scope:** the callers in `attach.rs` (plan 012 changes those); the spinner-frame rendering.

## Steps

### Step 1: Guarantee at least one sleep of the true interval per attempt

Change the delay so that when `interval < SPIN_MS` the loop still sleeps `interval` once (not zero).
Simplest correct shape:
```rust
let spins = (interval.as_millis() as u64 / SPIN_MS).max(1);
```
This keeps the spinner cadence for large intervals but guarantees ≥1 sleep for small ones. **Note:** with
`.max(1)` and a sub-80ms interval, the single sleep is still `SPIN_MS` (80ms), which *over*-waits a tiny
interval. If plan 012 needs the *actual* small interval honored, instead decouple poll cadence from
spinner cadence: sleep `min(interval, SPIN_MS)` per frame and run `ceil(interval / frame)` frames. Pick
the `.max(1)` one-liner unless plan 012's interval accuracy demands the decoupled form — record which you
chose.

**Verify**: `cargo check -p jackin-runtime` → exit 0.

### Step 2: Test

Add a `spin_wait/tests.rs` test: a poll that always fails with a small interval (e.g. 20ms) and
`max_attempts = 3` takes a non-trivial, bounded time (not ~0) — use `tokio::time` paused clock
(`tokio::time::pause()` / `advance`) to assert the total awaited duration ≈ `max_attempts × interval`
without real wall-clock sleeping. Model the tokio-time test setup after any existing paused-clock test in
`jackin-runtime` (`grep -rn "time::pause\|start_paused" crates/jackin-runtime/src`).

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(/spin_wait/)'` → pass.

## Done criteria

- [ ] `spin_wait` sleeps ≥ once per attempt for any `interval > 0`
- [ ] Paused-clock test proves a sub-80ms interval no longer busy-loops
- [ ] `cargo clippy -p jackin-runtime -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- No paused-clock test infrastructure exists and adding it pulls in `tokio` `test-util` in a way that
  isn't already available — check `crates/jackin-runtime/Cargo.toml` (it lists `tokio = { features = ["test-util"] }`
  in dev-deps at line 57, so this should be fine); if not, report.

## Maintenance notes

- Plan 012 will pass a short interval here; whichever fix shape you pick must actually throttle at that
  interval, or 012's readiness probe will spin the CPU.
