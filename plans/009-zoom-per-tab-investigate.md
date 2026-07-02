# Plan 009: Decide whether pane zoom should be per-tab (investigate)

> **Executor instructions**: This is an **investigate-and-decide** plan (LOW confidence — the current
> behavior may be intentional). Produce a recommendation + a follow-up fix plan; do **not** implement a
> refactor blindly. Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-capsule/src/daemon/session_lifecycle.rs crates/jackin-capsule/src/daemon/pane_layout.rs`

## Status

- **Priority**: P3
- **Effort**: M (fix, if pursued) / S (investigation)
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`toggle_zoom` stores the zoomed pane in a **single global** `Option<u64>` (`self.zoomed`). With tab A
zoomed, switching to tab B and zooming there overwrites `self.zoomed` with B's pane id; returning to tab
A shows it un-zoomed — the operator's zoom on A is lost without any action on A. Only one pane can be
zoomed mux-wide. **However**, an in-code comment frames zoom as "a single global field but scoped
per-tab", so this may be a deliberate simplification. The job is to determine intent and either fix or
document, not to churn.

## Current state

- `crates/jackin-capsule/src/daemon/session_lifecycle.rs:464-481` — `toggle_zoom`:
  ```rust
  let was_zoomed = self.active_zoomed_id().is_some();
  self.zoomed = if was_zoomed { None } else { focused };   // self.zoomed: Option<u64>
  ```
- `crates/jackin-capsule/src/daemon/pane_layout.rs:253-261` — `active_zoomed_id` returns the zoom only if
  the zoomed id belongs to the *active* tab (so a zoom pinned in tab A reads as "not zoomed" from tab B).

## Steps

### Step 1: Determine intended semantics

- Read the comment near `self.zoomed`'s declaration and `active_zoomed_id`. Quote it into this plan's row.
- Check whether any UX/docs spec says zoom is per-tab or global (grep docs:
  `grep -rn "zoom" docs/content/docs`).
- Decide: (a) **intended** (global, one zoom mux-wide) → document it and mark this plan
  `REJECTED (by design)`, OR (b) **bug** (should be per-tab) → proceed to Step 2.

### Step 2 (only if bug): write the fix as a follow-up plan

If per-tab is wanted, do **not** implement inline. Instead, write `plans/009a-zoom-per-tab-fix.md` (full
template) specifying: move the zoomed-pane id onto the `Tab` struct; update every `active_zoomed_id` /
`resize_panes` / compose call site; add a test that zoom in tab A survives switching to tab B and back.
List the exact call sites (`grep -rn "active_zoomed_id\|self.zoomed\|\.zoomed" crates/jackin-capsule/src`).

## Done criteria

- [ ] A written decision (by-design vs bug) with the quoted comment as evidence, in this plan's row note
- [ ] If bug: `plans/009a-zoom-per-tab-fix.md` exists with concrete call-site list; row set to `BLOCKED
      (fix plan 009a written, awaiting scheduling)`
- [ ] If by-design: row set to `REJECTED (zoom is intentionally global — documented)` and a one-line doc
      note added near the code
- [ ] No source behavior changed by this plan itself

## STOP conditions

- The comment/spec is genuinely ambiguous about intent — report both interpretations and let the operator
  choose; do not guess.

## Maintenance notes

- Whichever way this resolves, leave a one-line comment at `self.zoomed` stating the decided semantics so
  it isn't re-litigated.
