# Plan 004: Stop the daemon resize-coalescing loop from dropping the frame queued behind a resize

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/daemon/control.rs`
> If either changed, compare the "Current state" excerpt against the live code
> before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

The capsule daemon coalesces consecutive `Resize` frames so a SIGWINCH storm
produces one reflow instead of N repaints. The coalescing loop uses `while let
Ok(ClientFrame::Resize { .. }) = cmd_rx.try_recv()`. When `try_recv()` returns a
**non-Resize** frame (Input, Detach, FocusIn/FocusOut, Command, …), the `while
let` pattern fails to match — but `try_recv()` has **already removed that frame
from the channel**, so it is silently dropped. Concretely: the first keystroke
or mouse click sent immediately after a window resize vanishes; a `Ctrl-B D`
(Detach) sent right after a resize is eaten and the session fails to detach; a
`FocusIn`/`FocusOut` right after a resize desyncs focus state. This is a
data-losing drain that pattern-filters and discards the non-matching value.

The fix: capture the non-Resize frame instead of dropping it, and process it
after the coalesced resize (order preserved, since the stray frame may depend on
the new geometry).

## Current state

The select arm — `crates/jackin-capsule/src/daemon.rs:1272-1301`:

```rust
// Inbound attach frame from the active client task.
Some(frame) = cmd_rx.recv() => {
    // Coalesce consecutive Resize frames: process only the latest size
    // so a SIGWINCH storm produces one reflow instead of N full repaints.
    let frame = if let ClientFrame::Resize { .. } = &frame {
        let mut latest = frame;
        let mut coalesced: u32 = 0;
        while let Ok(ClientFrame::Resize { rows, cols }) = cmd_rx.try_recv() {
            latest = ClientFrame::Resize { rows, cols };
            coalesced = coalesced.saturating_add(1);
        }
        if coalesced > 0 {
            crate::cdebug!("resize: coalesced {coalesced} pending resize(s), using latest");
        }
        latest
    } else {
        frame
    };
    handle_client_frame(&mut mux, frame).await;
    if mux.detach_requested {
        mux.detach_requested = false;
        detach_client(&mut mux).await;
    }
    if mux.no_live_sessions()
        && handle_last_session_exit(&mut mux, None).await
    {
        cleanup_clipboard_run_dir();
        return Ok(());
    }
}
```

The bug is the `while let Ok(ClientFrame::Resize { .. }) = cmd_rx.try_recv()`:
`try_recv()` consumes the frame *before* the pattern is tested, so a non-Resize
frame is removed and then dropped when the pattern fails.

Facts you need:
- `ClientFrame` is defined in `crates/jackin-protocol/src/attach.rs:455-494`;
  variants include `Hello`, `Resize { rows, cols }`, `Input(Vec<u8>)`,
  `Command(Vec<u8>)`, `Detach`, `FocusIn`, `FocusOut`, clipboard variants,
  `HostNotice`.
- `handle_client_frame(mux: &mut Multiplexer, frame: ClientFrame)` lives in
  `crates/jackin-capsule/src/daemon/control.rs:108`; `ClientFrame` is already in
  scope there (imported at `control.rs:5-6`).
- `cmd_rx.try_recv()` returns `Result<ClientFrame, tokio::sync::mpsc::error::TryRecvError>`;
  `.ok()` collapses both `Empty` and `Disconnected` to `None`, which is the
  correct "stop draining" signal.
- `jackin-capsule` does **not** depend on `smallvec` — use a plain `Vec`.
- `control.rs` currently has **no** `#[cfg(test)] mod tests;` and no
  `control/tests.rs` sibling — you will add both (allowed; matches the
  tests-in-sibling-file rule in `crates/AGENTS.md`).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Targeted test | `cargo nextest run -p jackin-capsule -E 'test(coalesce)'` | passes |
| Crate tests | `cargo nextest run -p jackin-capsule` | all pass |
| Clippy | `cargo clippy -p jackin-capsule --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope**:
- `crates/jackin-capsule/src/daemon/control.rs` (add the pure helper + `mod tests;`)
- `crates/jackin-capsule/src/daemon/control/tests.rs` (create — unit tests for the helper)
- `crates/jackin-capsule/src/daemon.rs` (replace the select arm body)

**Out of scope** (do NOT touch):
- `crates/jackin-protocol/src/attach.rs` — no wire-format change; `ClientFrame`
  stays as-is.
- `handle_client_frame` itself — its per-variant handling is correct.
- The other `select!` arms (state ticker, PTY output) in `daemon.rs`.

## Git workflow

- Branch: operator's active branch, or `fix/resize-coalesce-drop`.
- One commit, conventional, signed. Example:
  `fix(capsule): preserve non-resize frame queued behind coalesced resize`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add a pure, testable coalescing helper to `control.rs`

At the end of `crates/jackin-capsule/src/daemon/control.rs` (before any existing
`#[cfg(test)] mod tests;` if one gets added), add:

```rust
/// Coalesce a run of consecutive `Resize` frames into the latest size and
/// return the ordered frames the daemon must process, plus how many resizes
/// were coalesced away.
///
/// A non-`Resize` frame pulled from the channel while draining is preserved and
/// returned after the coalesced resize (previously it was silently dropped
/// because `try_recv()` removes a frame before the `while let` pattern rejects
/// it). Order is preserved because the stray frame may depend on the new
/// geometry.
pub(crate) fn coalesce_client_frames(
    first: ClientFrame,
    mut next: impl FnMut() -> Option<ClientFrame>,
) -> (Vec<ClientFrame>, u32) {
    if !matches!(first, ClientFrame::Resize { .. }) {
        return (vec![first], 0);
    }
    let mut latest = first;
    let mut coalesced: u32 = 0;
    loop {
        match next() {
            Some(ClientFrame::Resize { rows, cols }) => {
                latest = ClientFrame::Resize { rows, cols };
                coalesced = coalesced.saturating_add(1);
            }
            Some(other) => return (vec![latest, other], coalesced),
            None => return (vec![latest], coalesced),
        }
    }
}
```

Then add, at the very bottom of `control.rs`:

```rust
#[cfg(test)]
mod tests;
```

**Verify**: `cargo check -p jackin-capsule` — fails only because
`control/tests.rs` doesn't exist yet (next step) and the daemon arm still uses
the old code (step 3). A "file not found for module tests" error here is
expected; proceed.

### Step 2: Create the unit tests

Create `crates/jackin-capsule/src/daemon/control/tests.rs` with:

```rust
use super::coalesce_client_frames;
use jackin_protocol::attach::ClientFrame;

fn resize(rows: u16, cols: u16) -> ClientFrame {
    ClientFrame::Resize { rows, cols }
}

#[test]
fn non_resize_first_frame_passes_through_alone() {
    let (frames, coalesced) = coalesce_client_frames(ClientFrame::Detach, || None);
    assert!(matches!(frames.as_slice(), [ClientFrame::Detach]));
    assert_eq!(coalesced, 0);
}

#[test]
fn consecutive_resizes_coalesce_to_latest() {
    let mut queue = vec![resize(30, 100), resize(40, 120)].into_iter();
    let (frames, coalesced) = coalesce_client_frames(resize(20, 80), || queue.next());
    assert!(matches!(frames.as_slice(), [ClientFrame::Resize { rows: 40, cols: 120 }]));
    assert_eq!(coalesced, 2);
}

#[test]
fn stray_frame_behind_resize_is_preserved_not_dropped() {
    // The regression: a non-Resize frame queued directly behind a Resize used
    // to be consumed by try_recv and dropped. It must now survive, after the
    // resize, in order.
    let mut queue = vec![ClientFrame::Detach].into_iter();
    let (frames, coalesced) = coalesce_client_frames(resize(20, 80), || queue.next());
    assert!(matches!(
        frames.as_slice(),
        [ClientFrame::Resize { rows: 20, cols: 80 }, ClientFrame::Detach]
    ));
    assert_eq!(coalesced, 0);
}

#[test]
fn stray_frame_after_several_resizes_is_preserved() {
    let mut queue = vec![resize(25, 90), ClientFrame::Input(vec![0x61])].into_iter();
    let (frames, coalesced) = coalesce_client_frames(resize(20, 80), || queue.next());
    assert!(matches!(
        frames.as_slice(),
        [ClientFrame::Resize { rows: 25, cols: 90 }, ClientFrame::Input(_)]
    ));
    assert_eq!(coalesced, 1);
}
```

(If `ClientFrame` is not re-exported at `jackin_protocol::attach::ClientFrame`,
use the path the crate already uses — check an existing capsule test's import,
e.g. in `crates/jackin-capsule/src/daemon/tests.rs`, and match it.)

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(coalesce)'` — after
step 3 compiles, all four pass. (Right now the crate won't build until step 3;
that's fine.)

### Step 3: Rewrite the daemon select arm to process both frames

Replace the select-arm body in `crates/jackin-capsule/src/daemon.rs:1272-1301`
(the whole `Some(frame) = cmd_rx.recv() => { … }` block shown in "Current state")
with:

```rust
// Inbound attach frame from the active client task.
Some(frame) = cmd_rx.recv() => {
    // Coalesce consecutive Resize frames into the latest size; any
    // non-Resize frame pulled while draining is preserved and processed
    // after the resize, never dropped.
    let (frames, coalesced) =
        control::coalesce_client_frames(frame, || cmd_rx.try_recv().ok());
    if coalesced > 0 {
        crate::cdebug!("resize: coalesced {coalesced} pending resize(s), using latest");
    }
    let mut should_return = false;
    for frame in frames {
        handle_client_frame(&mut mux, frame).await;
        if mux.detach_requested {
            mux.detach_requested = false;
            detach_client(&mut mux).await;
        }
        if mux.no_live_sessions()
            && handle_last_session_exit(&mut mux, None).await
        {
            cleanup_clipboard_run_dir();
            should_return = true;
            break;
        }
    }
    if should_return {
        return Ok(());
    }
}
```

Use whatever module path the file already uses to reach `handle_client_frame`
for `coalesce_client_frames` — if `handle_client_frame` is called bare (imported
via `use`), import `coalesce_client_frames` the same way and call it bare; if it
is called as `control::handle_client_frame`, use `control::coalesce_client_frames`.
Check the existing `use` lines at the top of `daemon.rs`.

**Verify**: `cargo check -p jackin-capsule` exits 0.

### Step 4: Full crate check

**Verify**: `cargo nextest run -p jackin-capsule` all pass (including the 4 new
tests and the existing resize tests like `resize_then_full_frame_repaints_with_new_geometry`);
`cargo clippy -p jackin-capsule --all-targets --locked -- -D warnings` exits 0.

## Test plan

- New file `crates/jackin-capsule/src/daemon/control/tests.rs` with the four
  tests above; the load-bearing one is `stray_frame_behind_resize_is_preserved_not_dropped`
  — it is the direct regression test for this bug.
- The existing resize/reflow tests in `crates/jackin-capsule/src/daemon/tests.rs`
  (e.g. `resize_then_full_frame_repaints_with_new_geometry`) must still pass,
  proving coalescing behavior is unchanged for the all-Resize case.
- Verification: `cargo nextest run -p jackin-capsule` → all pass.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n 'while let Ok(ClientFrame::Resize' crates/jackin-capsule/src/daemon.rs` returns nothing
- [ ] `grep -n 'fn coalesce_client_frames' crates/jackin-capsule/src/daemon/control.rs` matches
- [ ] `crates/jackin-capsule/src/daemon/control/tests.rs` exists with a `stray_frame_behind_resize` test
- [ ] `cargo nextest run -p jackin-capsule` exits 0
- [ ] `cargo clippy -p jackin-capsule --all-targets --locked -- -D warnings` exits 0
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The select arm in `daemon.rs` no longer matches the "Current state" excerpt
  (someone already restructured the coalescing).
- `handle_last_session_exit` / `detach_client` / `cleanup_clipboard_run_dir`
  are not in scope at the select arm — that means the arm moved; report where.
- The `for frame in frames { … return … }` early-return doesn't type-check
  because the enclosing function's return type isn't `Result<...>` — report the
  actual signature (the existing `return Ok(())` at the old line 1299 proves it
  is, so a mismatch means drift).

## Maintenance notes

- The pure `coalesce_client_frames` helper is now the seam for any future
  frame-batching policy (e.g. coalescing consecutive `Input` too); extend the
  helper and its tests rather than re-inlining logic into the select arm.
- Reviewer should confirm order is preserved (resize before the stray frame) and
  that the per-frame `detach`/`last-session` post-checks run for **every** frame
  in the batch, not just the first.
- Related capsule robustness items (unbounded PTY-output channel backpressure,
  PTY failure-recovery test coverage) are recorded separately in
  `plans/code-health/README.md`.
