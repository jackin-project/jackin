# Plan 008: Guard CSI Insert-Line / Delete-Line against a cursor outside the scroll region

> **Executor instructions**: VT-correctness fix in the terminal grid. Run every verification command.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-term/src/grid/perform.rs`

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

Per VT100/xterm, IL (`CSI L`) and DL (`CSI M`) are **no-ops when the cursor is outside the vertical
scroll margins**. The current implementation mutates the grid regardless: it removes/inserts rows using
`bottom = scroll_bottom` and `row = cursor_row` with only an `if row <= bottom` guard on the *telemetry*
push — the grid mutation itself never checks `scroll_top <= cursor_row <= scroll_bottom`. A program that
reserves a status line via `DECSTBM` (region `[0, rows-2]`) and issues IL/DL with the cursor on the
reserved row displaces the wrong rows, corrupting the visible grid until the next full repaint. DECSTBM
homes the cursor to absolute `(0,0)`, so with `scroll_top > 0` the "cursor above region" state is
reachable, not hypothetical.

## Current state

`crates/jackin-term/src/grid/perform.rs:160-205` — IL (`'L'`) and DL (`'M'`):
```rust
'L' => {   // Insert Lines
    let n = p0.max(1) as usize;
    let row = self.cursor_row as usize;
    let bottom = self.scroll_bottom as usize;
    let cols = self.cols;
    if row <= bottom {                     // <-- gates only the telemetry push
        self.scroll_ops.push(ScrollOp::Down { top: self.cursor_row, bottom: self.scroll_bottom, rows: p0.max(1) });
    }
    let grid = self.active_grid();
    for _ in 0..n {                        // <-- mutation runs even when row < scroll_top
        if bottom < grid.len() { grid.remove(bottom); }
        grid.insert(row, blank_row(cols));
    }
    self.dirty.mark_all();
}
'M' => {   // Delete Lines — same shape, remove at `row`, insert at `bottom`
    // ... no scroll_top / region-membership check ...
}
```
There is a `scroll_top` field on the same struct (used by DECSTBM at `perform.rs:287-305`). Confirm its
exact name with `grep -n "scroll_top" crates/jackin-term/src/grid/perform.rs`.

## Scope

**In scope:** `crates/jackin-term/src/grid/perform.rs` (the `'L'` and `'M'` arms) and
`crates/jackin-term/src/grid/perform/tests.rs` (or the grid tests file — locate with
`grep -rl "ScrollOp::Down\|blank_row" crates/jackin-term/src`).
**Out of scope:** other CSI arms; the `ScrollOp` telemetry model; the horizontal-margin logic (if any).

## Steps

### Step 1: Early-return IL/DL when the cursor is outside the scroll region

At the top of both the `'L'` and `'M'` arms, before any `grid.remove`/`grid.insert`, add:
```rust
if (self.cursor_row as usize) < scroll_top || (self.cursor_row as usize) > bottom {
    // xterm: IL/DL are no-ops outside the vertical scroll margins.
    return; // (or `break`/skip depending on the surrounding match structure)
}
```
Use the real `scroll_top` field name. Keep the in-region path (the common alt-screen case) byte-for-byte
unchanged. Add the one-line comment naming the xterm constraint (this is exactly the "looks weird but
intentional" case the repo's comment rule allows).

**Verify**: `cargo check -p jackin-term --all-targets` → exit 0.

### Step 2: Tests

Add grid tests:
- DECSTBM sets region `[2, rows-1]`; cursor homed to `(0,0)` (above region); `CSI L` / `CSI M` → grid
  **unchanged**.
- Cursor inside the region; `CSI L` inserts a blank at the cursor and drops the region bottom row (the
  existing correct behavior still holds — this is the regression guard that the fix didn't break the
  normal path).
Model after existing grid tests that build a small grid and assert cell contents.

**Verify**: `cargo nextest run -p jackin-term -E 'test(/insert_line|delete_line|il_dl|scroll/)'` → pass.

## Done criteria

- [ ] IL/DL are no-ops when `cursor_row` is outside `[scroll_top, scroll_bottom]` (test proves)
- [ ] In-region IL/DL behavior unchanged (test proves)
- [ ] `cargo clippy -p jackin-term -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- The struct has no `scroll_top` (only `scroll_bottom`): then the terminal doesn't model a top margin and
  this fix is moot — report that (the finding would be re-scoped).
- Adding the guard breaks an existing IL/DL test that encoded the *buggy* behavior — update that test to
  the xterm-correct expectation, but if the break is elsewhere, STOP.

## Maintenance notes

- Reviewer: verify the guard uses `>=`/`<=` bounds matching xterm (inclusive region), and that the
  telemetry `ScrollOp` push is also skipped on the no-op path (it should be, since the whole arm returns).
- Cross-check against the capsule render-conformance harness — if a fixture depended on the old behavior,
  regenerate it (see plan 018 for the fixture workflow).
