# Plan 027: Finish or rename the "Transitional" console facade left by the #664 health push

> **Executor instructions**: Decide-then-act plan. Determine whether the console extraction is still in
> progress; then either finish it or re-document the split as steady state. Update `plans/README.md`.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin/src/console`

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 023 (the console crate boundary should settle first)
- **Category**: tech-debt
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

The #664 "codebase health layout" push left a permanent **"Transitional root-console TUI facade"** module
in the binary (`crates/jackin/src/console/tui.rs`) that re-exports `jackin_console::tui::input::editor::*`
and instantiates the console crate's generics. `crates/jackin/src/console` totals ~6.4K lines of
shell/effects/services wrapping the `jackin-console` crate. "Transitional" naming with no tracked
completion means an indefinitely half-migrated boundary: two places (`jackin/src/console` facade +
`jackin-console` crate) both describe editor/input dispatch, so contributors must learn which layer owns
what.

## Current state

- `crates/jackin/src/console/tui.rs:1` — `//! Transitional root-console TUI facades.`; `:9` — "Thin adapter
  shell — editor-stage input dispatch lives in jackin-console" (re-exports `jackin_console::tui::input::editor::*`).
- `crates/jackin/src/console` — `effects.rs` (936+ lines), `services.rs`, `tui/input/editor/tests.rs` (2636
  lines) wrapping the crate.
- #664 (`46511939d`) touched 617 files with near-1:1 churn; the "Transitional" label indicates the console
  extraction isn't finished.

## Steps

### Step 1: Determine the intended end state

- Check the roadmap / codebase-health docs for whether the console extraction has a defined completion
  (`grep -rn "console.*extract\|transitional\|health layout" docs/content/docs TODO.md`).
- Decide: (a) the extraction **should finish** (the imperative shell that must stay in the binary is small
  and identifiable, and the re-export shims should go), or (b) the split **is** the intended steady state
  (binary owns the imperative shell, crate owns the functional core) and only the "Transitional" framing is
  wrong.

### Step 2a: Finish the extraction (if a)

Move the shell code that legitimately belongs in the binary out of a module named "facade", delete the
re-export shims, and re-point call sites to the crate directly. Coordinate with plan 023 (don't move things
that 023 is also reshaping).

### Step 2b: Re-document as steady state (if b)

Rename the module away from "Transitional", and add a short doc comment (and a note in the codebase-map
docs) stating the intended split: binary = imperative shell (effects/services/root wiring), crate =
functional core. This removes the "half-migrated" ambiguity without code churn.

**Verify (either branch)**: `cargo check -p jackin --all-targets` → exit 0;
`cargo nextest run -p jackin` → all pass;
`grep -rn "Transitional" crates/jackin/src/console` → no matches.

## Done criteria

- [ ] A written decision (finish vs steady-state) with doc evidence in the row note
- [ ] `grep -rn "Transitional" crates/jackin/src/console` → no matches
- [ ] Branch a: re-export shims removed, call sites re-pointed; **or** Branch b: module renamed + split documented
- [ ] `cargo nextest run -p jackin` green
- [ ] `PROJECT_STRUCTURE.md`/codebase-map reflects the console boundary (docs gate)
- [ ] `plans/README.md` row updated

## STOP conditions

- Plan 023 hasn't settled the console crate boundary — do 023 first, or this churns twice.
- Finishing the extraction (branch a) turns out to require the generics collapse (plan 022) first — report
  the dependency chain.

## Maintenance notes

- Whatever is decided, the word "Transitional" should not survive without a tracked completion — a reviewer
  should reject re-introducing indefinitely-transitional module names.
