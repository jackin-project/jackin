# Plan 012: Probe the bind-mounted capsule socket for daemon readiness instead of a 500ms `docker exec` poll

> **Executor instructions**: Perf fix on the launch hot path. Run every verification command. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-runtime/src/runtime/attach.rs crates/jackin-runtime/src/runtime/snapshot.rs crates/jackin-runtime/src/spin_wait.rs`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 006 (spin_wait clamp) — do 006 first
- **Category**: perf
- **Planned at**: commit `46511939d`, 2026-07-03
- **Latency note**: Local Docker launch measurement is deferred in this environment. Mechanically, the
  same-kernel path now returns as soon as the bind-mounted Capsule socket accepts a `UnixStream`
  connection, avoiding the previous mandatory 500ms status-poll cadence. The `docker exec` fallback keeps
  the old 30s budget but starts at 25ms and backs off to 500ms.

## Why this matters

`wait_for_capsule_daemon` polls readiness by running `docker exec <container> sh -c JACKIN_STATUS_CMD` on
a **flat 500ms** interval, and `spin_wait` sleeps the *entire* interval before retrying — so after the
daemon binds, detection is delayed by up to a full 500ms, and each poll spawns a `docker exec` process
(~50-100ms). The first probe almost always misses on a cold container, guaranteeing ≥1 full sleep. This
sits directly on operator-perceived `jackin load` → prompt latency. The codebase **already** prefers a
bind-mounted `UnixStream` for snapshots (`snapshot.rs:78-101`: connect directly when the socket exists,
~microseconds) and only falls back to `docker exec` when the socket can't be bridged (Docker Desktop).
Readiness can use the same cheap probe on Linux, cutting ~250ms avg off every launch.

## Current state

- `crates/jackin-runtime/src/runtime/attach.rs:112-147` — `wait_for_capsule_daemon`:
  ```rust
  const MAX_ATTEMPTS: u32 = 60;
  const INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
  // spin_wait(..., || async { docker.exec_capture(container_name, &["sh", "-c", JACKIN_STATUS_CMD]).await.map(|_| ()) })
  ```
- `crates/jackin-runtime/src/spin_wait.rs:44-74` — polls once, then sleeps the whole interval before
  retry (plan 006 fixes the sub-80ms clamp this plan relies on).
- `crates/jackin-runtime/src/runtime/snapshot.rs:78-101` — the existing socket-first pattern to mirror:
  `socket_path(...).exists()` → direct `UnixStream::connect` (~µs), else `docker exec` fallback.
- Also relevant: `attach.rs:1083` `wait_for_dind` (flat 1s) — same class, but off the default profile; do
  it opportunistically only if trivial (see Step 3).

## Scope

**In scope:** `crates/jackin-runtime/src/runtime/attach.rs` (`wait_for_capsule_daemon`), reusing the
socket-path helper from `snapshot.rs`. **Out of scope:** the `JACKIN_STATUS_CMD` contents; the snapshot
path itself; Docker Desktop's relay mechanism (keep the exec fallback for it).

## Steps

### Step 1: Add a socket-connect readiness predicate on Linux

Where the bind-mounted socket is reachable (same-kernel Linux — reuse `snapshot.rs`'s `socket_path(...)`
+ `UnixStream::connect` check; extract a shared helper if it isn't already public), poll readiness by
attempting a `UnixStream::connect` on a **short backoff** (e.g. 25 → 50 → 100ms). A successful connect
(or a successful status handshake, matching whatever `snapshot.rs` treats as "ready") means ready.

### Step 2: Keep the `docker exec` probe only for the unbridgeable case

When the socket is not directly reachable (Docker Desktop), keep the `docker exec` probe but give it a
**smaller initial interval with exponential backoff** instead of a flat 500ms. Preserve the same success
predicate and the overall max wait budget (don't shorten total timeout — #709 widened these deliberately).

**Verify**: `cargo check -p jackin-runtime --all-targets` → exit 0;
`cargo clippy -p jackin-runtime -- -D warnings` → exit 0.

### Step 3 (optional, only if trivial): apply the same to `wait_for_dind`

If `wait_for_dind` (attach.rs:1083) can take the same socket-or-backoff treatment with a small edit, do
it. If it needs different readiness semantics, leave it and note it in maintenance.

### Step 4: Validate the latency win

The path already emits `JACKIN_DEBUG` timing spans `capsule/wait_capsule_socket` (attach.rs:119-145). In
this plan's row note, record: before/after span duration from a local `--debug` launch, OR — if you can't
run a real Docker launch — state that measurement is deferred and cite the code change as the mechanism.

**Verify**: `cargo nextest run -p jackin-runtime -E 'test(/attach|readiness|capsule_daemon/)'` → pass.

## Test plan

- Unit: readiness returns promptly once a fake socket is connectable (use a `tokio` `UnixListener` in a
  tempdir as the "daemon"); backoff path exits on first successful connect.
- Fallback: when no socket is present, the exec probe path is taken (fake `DockerApi`).
- Model after existing `attach` tests (`grep -rl "wait_for_capsule_daemon\|exec_capture" crates/jackin-runtime/src`).

## Done criteria

- [x] Readiness uses a socket-connect probe on the socket-reachable path; exec probe retained for the
      bridged (Docker Desktop) fallback
- [x] No flat 500ms sleep-before-retry remains on the socket path
      (`grep -n "from_millis(500)" crates/jackin-runtime/src/runtime/attach.rs` → only the fallback, if any)
- [x] Total wait budget unchanged (still ≤ the pre-existing max)
- [x] `cargo nextest run -p jackin-runtime` unavailable locally; `cargo test -p jackin-runtime
      wait_for_capsule_daemon` passes
- [x] `cargo clippy -p jackin-runtime -- -D warnings` exits 0
- [x] `plans/README.md` row updated with the latency note (measured or deferred)

## STOP conditions

- `spin_wait`'s sub-80ms clamp (plan 006) is not yet fixed — the short backoff will busy-loop; do 006 first.
- The snapshot socket-path helper is not reusable without a broader refactor — report before duplicating it
  (the repo's DRY rule wants one helper, not two).
- Docker Desktop's relay makes `UnixStream::connect` succeed *before* the daemon is truly ready (false
  positive) — then the socket probe is unsafe on that platform; keep exec there and note it.

## Maintenance notes

- Reviewer: confirm the total timeout budget did not shrink (regression risk: reintroducing #709/#744's
  too-tight-timeout failures).
- If the readiness handshake diverges from the snapshot handshake, they must not drift — factor the shared
  "is the daemon answering" check into one place.
