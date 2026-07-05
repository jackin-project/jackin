# Plan 003: Decouple `advance_status` from the biased I/O select so state keeps advancing while the agent is busy

> **Executor instructions**: This touches the daemon's hottest loop. Do plan 008 (testability seam) FIRST so
> you can verify a full tick. Run every verification command; honor STOP conditions. Update the README row.
>
> **Drift check (run first)**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/tui/subscriptions.rs`

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (central daemon loop; `&mut mux` borrow discipline)
- **Depends on**: 008 (add the test seam before changing this)
- **Category**: bug (compute cadence)
- **Planned at**: commit `5d3661cff`, 2026-07-03
- **Implementation status**: DONE in PR 714 (`state_ticker` is now above PTY output in the biased daemon select; the output arm remains one-event-per-pass bounded and regression tests pin the ordering/ready-tick behavior)

## Why this matters

`advance_status` is the **only** call that authors agent state, and it runs from a single arm of a
`tokio::select! { biased; … }` — ranked **after** the unbounded PTY-output arm. In a `biased` select, branches
are polled top-to-bottom and the first `Ready` wins; a working agent (streaming tokens, animated spinner,
live dialog) keeps the output channel continuously `Ready`, so the select resolves the output arm every
iteration and the `state_ticker` future is never polled. `tokio::interval` only advances when polled, so
`advance_status` stops running and the tab status **freezes at its last value — exactly while the agent is
busy** (the moment the operator is watching to see if it's working). The "1 Hz floor" is asserted in a
comment but has no structural backing. Root cause: the time-critical state machine is co-scheduled inside a
`biased` I/O select, below unbounded throughput — the architecture *permits* starvation by construction.

## Current state

- `crates/jackin-capsule/src/tui/subscriptions.rs:17` — `STATE_TICK_INTERVAL = Duration::from_secs(1)`.
- `crates/jackin-capsule/src/daemon.rs:966` — `tokio::select! { biased; … }`.
- Arm order (verified): sigterm(968), sigint(973), new_clients(986), control_rx(997), handshake_rx(1010),
  cmd_rx(1156), **`Some(event) = mux.event_rx.recv()` (PTY output) at 1187** — one event per wake, no drain
  loop — then esc/render deadlines, `branch_context_ticker`(1341), and the **`state_ticker.tick()` arm at
  ~1366** which is the sole caller of `advance_status` (`daemon.rs:1421`), writing `session.state`
  (`daemon.rs:1447`) and repainting on change (`daemon.rs:1473-1477`).
- So the state arm sits below the starvable output arm. (Contrast: the `cmd_rx` arm has an explicit drain
  comment at `daemon.rs:1081` — output does not.)

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Build | `cargo check -p jackin-capsule --all-targets` | exit 0 |
| Test | `cargo nextest run -p jackin-capsule -E 'test(/daemon|status_tick|advance_status/)'` | all pass |
| Clippy | `cargo clippy -p jackin-capsule -- -D warnings` | exit 0 |

## Scope

**In scope:** `crates/jackin-capsule/src/daemon.rs` (the select loop / where `advance_status` is driven).
**Out of scope:** the arbitration/detection internals (other plans); the render path (plan 001).

## Steps

### Step 1 (prerequisite): confirm plan 008's seam exists

You need a way to assert "a full tick still advances state under output load" without a live container. If
plan 008 landed, use its injectable `EvidenceSnapshot`/tick seam. If not, STOP and do 008 first — changing
the hottest loop without a test that proves the floor is restored is unsafe.

### Step 2: Guarantee a cadence floor independent of I/O

Remove the enabling condition — `advance_status` must run ~1 Hz regardless of output volume. Pick the approach
that fits the daemon's borrow structure (the state tick needs `&mut mux`):
- **Preferred — bound the output arm per wake:** cap how much the output arm can monopolize the loop. E.g. give
  the output arm a per-iteration budget (process at most N queued events, or run at most for a time slice) and
  re-enter the select so the ticker gets polled; OR move the `state_ticker` arm **above** `event_rx` in the
  biased order **and** drain-bound the output arm so state can't be starved. Moving the arm alone is *not
  enough* (a continuously-Ready output future above the ticker still wins) — it must be combined with an output
  budget so the ticker is reached.
- **Alternative — a coalescing "state dirty" wake:** on PTY output/input, set a `state_dirty` flag / send on a
  small-capacity channel, and have a separate select arm (or the existing tick arm) recompute when dirty OR on
  the 1 Hz interval, whichever first. This also delivers the deferred "sub-1 Hz faster-recheck on PTY damage"
  the roadmap notes — but watch render thrash (debounce already caps transitions in `policy.rs`).
Do **not** spawn `advance_status` on a separate task if it needs `&mut mux` shared with the loop (that forces a
lock/Arc<Mutex> on the hot path); prefer keeping it in the loop with a guaranteed-poll structure.

Whichever you choose, the invariant to establish: **under sustained output, `advance_status` still runs at
least once per `STATE_TICK_INTERVAL`.**

**Verify**: `cargo check -p jackin-capsule --all-targets` → exit 0; `cargo clippy … -D warnings` → exit 0.

### Step 3: Test the floor under load

Using plan 008's seam, add a daemon-level test that pushes a continuous stream of `SessionEvent::Output` and
asserts `advance_status` is still invoked within ~1 tick (e.g. a counter/`tokio::time` paused-clock test that
advances virtual time and asserts the state recomputes despite pending output). Model after any existing
paused-clock daemon test (`grep -rn "time::pause\|start_paused" crates/jackin-capsule/src`).

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/status_tick|floor|advance_status/)'` → pass.

## Done criteria

- [x] `advance_status` runs ≥ once per `STATE_TICK_INTERVAL` even under a saturating output stream (test proves)
- [x] The output arm no longer starves the state arm (budget/drain bound in place)
- [x] `cargo nextest run -p jackin-capsule` green (no regression in the loop's other arms)
- [x] `cargo clippy -p jackin-capsule -- -D warnings` exits 0
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- Plan 008's seam isn't in place — do 008 first.
- Bounding the output arm introduces visible input/render latency (the output path is also what paints agent
  bytes) — the budget must be large enough that interactive latency is unaffected but small enough that the
  ticker is reached; if you can't find that window, report with measurements (the biased order + a coalescing
  dirty-wake is then the better structure).
- The `&mut mux` borrow can't be satisfied by any in-loop restructuring without an `Arc<Mutex>` on the hot path
  — report before adding locking (there is likely a select-shape that avoids it).

## Maintenance notes

- Reviewer: the acceptance test is the whole point — it must fail if a future change re-subordinates the state
  tick to output. Keep it.
- This also unblocks the roadmap's deferred "sub-1 Hz faster-recheck on PTY damage" if you take the
  coalescing-dirty-wake path — note that in the row if so.
- Do not "fix" this by merely deleting `biased` — a fair select still can't guarantee a floor under saturation
  (recorded in the README's considered-rejected).
