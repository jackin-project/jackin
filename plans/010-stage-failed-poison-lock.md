# Plan 010: Don't treat a poisoned view lock as an acknowledged launch failure (investigate + harden)

> **Executor instructions**: LOW-confidence hardening. Confirm the mechanism, then apply the small guard.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-launch-tui/src/progress.rs`

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Decision

- Chosen fail-safe: recover the poisoned guard with `into_inner()` and read `failure_ack`.
- Reason: the failure view remains the source of truth for whether the rich renderer's input path acknowledged the popup; poison should not synthesize an acknowledgement.
- The helper logs the poison through `debug_log!` and keeps the healthy path behavior unchanged.
- A focused test poisons the view mutex and proves the helper returns `false` until the recovered view sets `failure_ack = true`.

## Why this matters

`stage_failed`'s failure-ack wait loop breaks when
`let acked = self.view.lock().map_or(true, |v| v.failure_ack)` is true. On a **poisoned** mutex,
`map_or(true, …)` yields `true`, so the loop exits on the first tick **as if the operator dismissed the
launch-failure popup** — the flow proceeds past a real failure without acknowledgement. Reachability is
low (poisoning requires a prior panic, and workspace lints deny `panic`/`unwrap`), and breaking out is
arguably safer than hanging — so this is a latent edge, not an active bug. Worth a clear decision + a
tiny guard so a future panic doesn't silently skip the failure dialog.

## Current state

`crates/jackin-launch-tui/src/progress.rs:169-177`:
```rust
let acked = self.view.lock().map_or(true, |v| v.failure_ack);
if acked { break; }
```
On `Err(poisoned)`, `map_or(true, ...)` returns `true` → loop breaks → failure treated as acknowledged.

## Steps

### Step 1: Confirm and decide the fail-safe direction

Read the surrounding loop and the panic/lints posture. Decide the correct fail-safe: on a poisoned lock,
should the launch (a) still surface the failure (recover the guard via `into_inner()` and read
`failure_ack`), or (b) exit via an explicit error path — but **not** silently treat the failure as
acknowledged. Record the choice.

### Step 2: Apply the guard

Replace the `map_or(true, …)` with handling that distinguishes `Ok(guard)` (read `failure_ack`) from
`Err(poisoned)` (recover with `poisoned.into_inner()` and read `failure_ack`, and `debug_log!` the
poison), so a poisoned lock does not auto-acknowledge. Keep behavior identical on the healthy path.

**Verify**: `cargo check -p jackin-launch-tui --all-targets` → exit 0;
`cargo clippy -p jackin-launch-tui -- -D warnings` → exit 0.

### Step 3: Test (best-effort)

If the view mutex can be poisoned in a test seam, add a test that a poisoned lock does not break the loop
as "acknowledged". If poisoning can't be triggered without a real panic (lints forbid), document that in
the row note and rely on the `into_inner` recovery being obviously correct by inspection.

## Done criteria

- [ ] Poisoned lock no longer auto-acknowledges a launch failure (by code inspection or test)
- [ ] Healthy path unchanged
- [ ] `cargo clippy -p jackin-launch-tui -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- The loop's surrounding logic makes "recover and continue" unsafe (e.g. the guard holds partially-updated
  state) — report; the right fix may be an explicit error exit instead.

## Maintenance notes

- This is defense-in-depth given `panic`-deny lints; keep the `into_inner` recovery even if poisoning
  seems impossible today, since a future `#[expect(panic)]` site could reintroduce it.
