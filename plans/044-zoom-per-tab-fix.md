# Plan 044: Store pane zoom state per tab

> **Executor instructions**: Follow-up from Plan 009. Implement the behavior change in this same PR branch.
> Do not create a new branch. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat HEAD -- crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/daemon/session_lifecycle.rs crates/jackin-capsule/src/daemon/pane_layout.rs`

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: 009
- **Category**: bug
- **Planned at**: follow-up created by Plan 009, 2026-07-04

## Why this matters

Pane zoom is intended to behave as per-tab state: switching away from a zoomed tab should not let another tab overwrite that tab's zoom choice. Today the daemon stores one global `Option<u64>` in `Multiplexer::zoomed`, so zooming tab B replaces tab A's zoomed pane id. Returning to tab A shows it unzoomed even though the operator never unzoomed tab A.

## Current state

The investigation in Plan 009 found no docs requiring mux-wide global zoom. The code comments point the other way:

- `session_lifecycle.rs::toggle_zoom` says toggling on tab B must not unzoom what tab A had pinned.
- `pane_layout.rs::active_zoomed_id` says render/input/scroll/mouse paths must behave as if zoom is per-tab.
- `daemon.rs` currently keeps one `zoomed: Option<u64>` slot for all tabs.

## Scope

**In scope:** `crates/jackin-capsule/src/daemon.rs`, `crates/jackin-capsule/src/daemon/session_lifecycle.rs`, `crates/jackin-capsule/src/daemon/pane_layout.rs`, and focused daemon tests.

**Out of scope:** visual redesign of zoom chrome, keymap changes, and palette wording changes.

## Call sites to update

From `rg -n "active_zoomed_id|self\\.zoomed|\\.zoomed|zoom_id|ToggleZoom|ZoomPane" crates/jackin-capsule/src -g '*.rs'`:

- `crates/jackin-capsule/src/daemon.rs` — move `zoomed: Option<u64>` off `Multiplexer` and onto the tab state.
- `crates/jackin-capsule/src/daemon/session_lifecycle.rs` — update tab close/session removal/new session reset/toggle paths that clear or write `self.zoomed`.
- `crates/jackin-capsule/src/daemon/pane_layout.rs` — update `active_zoomed_id`, killed-pane cleanup, `resize_panes`, visible rect, and focus navigation paths to read the active tab's zoom.
- `crates/jackin-capsule/src/daemon/compositor.rs` and `crates/jackin-capsule/src/daemon/mouse_input.rs` — should continue through `active_zoomed_id`; no direct state read should remain.
- `crates/jackin-capsule/src/tui/model.rs` and `crates/jackin-capsule/src/tui/view.rs` — should continue receiving a single active-tab zoom id/view bool; no behavior change expected.

## Steps

### Step 1: Move zoom state onto tabs

Add `zoomed: Option<u64>` to the daemon tab struct. Initialize it to `None` wherever tabs are constructed. Remove `Multiplexer::zoomed`.

### Step 2: Rewrite active-tab zoom helpers

Make `active_zoomed_id` read the active tab's `zoomed` field and verify the id still belongs to that tab. Keep the public helper shape so compositor, mouse, layout, and model call sites keep using one abstraction.

### Step 3: Update lifecycle cleanup

When closing a tab, dropping a session, killing a pane, or resetting all sessions, clear only the owning tab's zoom state. If the active zoomed pane is killed, clear that tab's zoom before resizing. New tabs start unzoomed.

### Step 4: Tests

Add focused daemon tests:

- zoom pane in tab A, switch to tab B, zoom pane in tab B, switch back to tab A: tab A is still zoomed to its original pane.
- unzoom in tab B does not clear tab A's zoom.
- killing the zoomed pane in one tab clears only that tab's zoom.

Model these after existing daemon tab-switch and split/close tests in `crates/jackin-capsule/src/daemon/tests.rs`.

## Verify

- `cargo fmt --check`
- `cargo check -p jackin-capsule --all-targets`
- `cargo test -p jackin-capsule zoom`
- `cargo clippy -p jackin-capsule --all-targets -- -D warnings`

If `cargo nextest` is available, also run:

- `cargo nextest run -p jackin-capsule -E 'test(/zoom/)'`

## Done criteria

- [ ] Zoom state survives independently per tab.
- [ ] Existing active-tab rendering/input APIs still use `active_zoomed_id`.
- [ ] Tests cover tab A/tab B independent zoom and kill cleanup.
- [ ] `plans/README.md` rows 009 and 044 updated.

## STOP conditions

- Moving zoom state onto tabs conflicts with serialized attach/control protocol state. If that happens, stop and document the protocol compatibility issue before changing message shape.

## Maintenance notes

- Do not use a `HashMap<tab_index, pane_id>` keyed by tab index; tab removal shifts indexes. Store the zoom id on the tab object itself.
