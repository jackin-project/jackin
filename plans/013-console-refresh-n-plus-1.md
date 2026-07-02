# Plan 013: Coalesce the console instance-refresh docker fan-out (N+1 every 500ms)

> **Executor instructions**: Perf fix on the console tick loop. Run every verification command. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-console/src/tui/subscriptions.rs crates/jackin/src/console/effects.rs crates/jackin/src/console/services.rs crates/jackin-runtime/src/runtime/snapshot.rs`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: perf
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

While the console is open, an instance-refresh fires on a 500ms throttle continuously (driven off the
50ms animation tick). Each refresh runs one `docker ps` **plus** one snapshot per active instance; on
Docker Desktop (the maintainer's own platform, Darwin) each snapshot degrades to a `docker exec`. So with
N running instances it's `1 + N` process spawns every 500ms (~`2 + 2N` docker subprocesses/sec) forever,
purely to refresh a status list. On Linux the direct socket avoids the per-instance exec, but the
unconditional `docker ps` every 500ms remains. Steady background CPU/process churn.

## Current state

- `crates/jackin-console/src/tui/subscriptions.rs:5` — `INSTANCE_REFRESH_INTERVAL = 500ms`.
- `crates/jackin/src/console/effects.rs:45-56` — `RequestInstanceRefresh` fires on the throttle each due
  tick, continuously while the console is open.
- `crates/jackin/src/console/services.rs:391-420` — `running_role_containers()` shells `docker ps` on
  **every** refresh (all platforms).
- `crates/jackin/src/console/services.rs:366-367,489-520` — `fetch_snapshots_parallel` spawns a thread
  per active instance (chunks of 8) calling `fetch_snapshot`.
- `crates/jackin-runtime/src/runtime/snapshot.rs:91,224-240` — on Docker Desktop each `fetch_snapshot`
  degrades to `docker exec <container> sh -lc <script>`.

## Scope

**In scope:** `crates/jackin/src/console/services.rs`, `crates/jackin-console/src/tui/subscriptions.rs`,
`crates/jackin/src/console/effects.rs`. **Out of scope:** `snapshot.rs`'s per-instance snapshot content;
the animation tick cadence; the socket-vs-exec decision inside `snapshot.rs`.

## Steps

Pick the cheapest wins that don't make the list feel stale. Do Step 1; Steps 2–3 as budget allows.

### Step 1: Back off the refresh interval when the exec fallback is in play

Detect whether snapshots are going through the direct socket (fast) or the `docker exec` fallback (slow —
Docker Desktop). When exec-fallback, raise the effective refresh interval (e.g. 500ms socket path, 2–3s
exec path). Keep the fast path at 500ms so Linux feels live. Determine the platform/bridge state from the
same signal `snapshot.rs` uses (`grep -n "exec\|socket_path\|bridged" crates/jackin-runtime/src/runtime/snapshot.rs`).

### Step 2: Skip snapshots for non-running instances

Use the single `docker ps` result to reconcile *and* to filter which instances get a `fetch_snapshot` —
don't snapshot instances that `docker ps` shows as not running. This cuts N toward "actually running".

### Step 3 (optional): scope snapshots to the focused/expanded workspace

If the console only displays detailed status for the focused/expanded workspace, only snapshot those
instances rather than all. Confirm the view model to avoid dropping data the UI shows.

**Verify (each step)**: `cargo check -p jackin -p jackin-console --all-targets` → exit 0;
`cargo clippy -p jackin -p jackin-console -- -D warnings` → exit 0.

## Test plan

- Unit: with a fake snapshot backend reporting "exec fallback", the refresh cadence is the slower interval;
  with "socket", it's 500ms.
- Unit: instances `docker ps` reports as stopped are not snapshotted.
- Model after existing `console/services` tests (`grep -rl "fetch_snapshots\|running_role_containers" crates/jackin/src`).

## Done criteria

- [ ] Exec-fallback refresh cadence is measurably slower than the socket cadence (test asserts the chosen
      interval selection)
- [ ] Non-running instances are not snapshotted (test asserts)
- [ ] `docker ps` is still issued at most once per refresh (not once per instance)
- [ ] `cargo nextest run -p jackin -p jackin-console` green
- [ ] `plans/README.md` row updated

## STOP conditions

- The refresh cadence is load-bearing for some animation/liveness the UI depends on — report before
  slowing it; the operator may prefer a different tradeoff.
- Filtering by `docker ps` would drop instances the view legitimately shows (e.g. exited-but-restorable) —
  keep those; only skip snapshots that are pure status polls.

## Maintenance notes

- Reviewer: confirm the slower exec cadence doesn't make a just-launched instance take multiple seconds to
  appear in the list.
- A future batch status command in the container image would let N execs collapse to one — note as a
  larger follow-up (needs an image-side change, out of scope here).
