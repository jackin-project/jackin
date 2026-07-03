# Plan 014: ConfirmSaveState onto a keymap — no hand-matched confirm keys

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-console/src/tui/components/confirm_save.rs crates/jackin-tui/src/components/save_discard_dialog.rs crates/jackin-tui/src/components/confirm_dialog.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (commit path payload must be preserved)
- **Depends on**: plans/003-tui-focus-taxonomy.md (uses `ButtonFocus`)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: BLOCKED — drift check found existing changes in `confirm_save.rs`, `confirm_dialog.rs`, and `save_discard_dialog.rs` before plan work.

## Why this matters

The workspace has one keymap discipline: dispatch and hint advertisement derive from the same `Keymap` table ("Replace every hand-written confirm-dialog hint array with `CONFIRM_KEYMAP.hint_spans()`" — `confirm_dialog.rs:108`). The console's save-confirmation dialog (`ConfirmSaveState`) is the one confirm-shaped component that bypasses it: a bespoke `match key.code` block. A keybinding change to the shared confirm dialogs (e.g. the Ctrl+Q cancel that `CONFIRM_KEYMAP` already carries) silently skips this dialog, and its hints are hand-assembled rather than derived. This plan gives it a keymap and the `ButtonFocus` cycling from plan 003, keeping its scrollable preview and typed commit payload intact.

## Current state

`crates/jackin-console/src/tui/components/confirm_save.rs` (verified):

```
:24  pub enum SaveChoice
:29  pub enum ConfirmSaveFocus { Save, Cancel }   // (2 variants; verify)
:42  pub struct ConfirmSaveState<M: Clone = ()>   // carries preview + planner payload M
:74  pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveChoice::Save),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Up | KeyCode::Char('k' | 'K')   => { self.scroll_preview_by(-1); Continue }
            KeyCode::Down | KeyCode::Char('j' | 'J') => { self.scroll_preview_by(1);  Continue }
            KeyCode::Tab | BackTab | Right | Left | Char('l'|'L'|'h'|'H') => toggle focus,
            KeyCode::Enter => match self.focus { ... }
        } }
:118 pub fn scroll_axes(&self) -> ScrollAxes
:129 required_height   :135 prepare_for_render   :147 render
```

Used across ~10 console files (`input/save.rs`, `state.rs`, `model/modal.rs`, `model/stage.rs`, `input/global_mounts.rs`, `model/modal/auth_impls.rs`, `modal_rects.rs`, tests).

The shared pattern to follow: `crates/jackin-tui/src/components/save_discard_dialog.rs:104` `pub static SAVE_DISCARD_KEYMAP: Keymap<SaveDiscardAction>` and `confirm_dialog.rs:109` `CONFIRM_KEYMAP` — bindings tables with `Visibility` + glyphs, `hint_spans()` derivation, dispatch via `keymap.action_for(...)`-style lookup (read either file for the exact dispatch idiom).

Scroll-hint precedent: `SCROLL_HINT_KEYMAP` + `hint_spans_for_axes(self.scroll_axes())` (`crates/jackin-tui/src/keymap.rs:283,:534`) — the preview-scroll keys should advertise through it, like other scrollable dialogs.

Plan 003's `ButtonFocus` trait (in `focus_owner.rs` after 003 lands): implement for `ConfirmSaveFocus` with `RING = &[Save, Cancel]`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-console` then full | pass |

## Scope

**In scope**:
- `crates/jackin-console/src/tui/components/confirm_save.rs` — new `CONFIRM_SAVE_KEYMAP` (S/save, C/Esc cancel, Enter activate-focused, focus-move keys, j/k/↑↓ preview scroll), keymap-derived `confirm_save_hint_spans()`, `handle_key` dispatching through it, `ButtonFocus` impl
- The hint-arm(s) that currently hand-assemble this dialog's footer hints — find them: `rg -n 'confirm_save|ConfirmSave' crates/jackin-console/src/tui/components/footer_hints/` — switch to the derived fn

**Out of scope**:
- The preview content pipeline (`save_preview.rs` — plan 016)
- The commit payload plumbing (`M`, `effective_removals`/`final_mounts` threading) — types unchanged
- Whether ConfirmSave should merge with the shared save/discard dialog (deliberately NOT merged: 2-button Save/Cancel + scrollable preview + typed payload is a distinct component; the shared discipline it must adopt is the keymap, not the widget)

## Git workflow

Branch (operator confirm): `refactor/confirm-save-keymap`. `git commit -s` + push.

## Steps

### Step 1: Define the keymap

Model the bindings table on `SAVE_DISCARD_BINDINGS` (`save_discard_dialog.rs`, read the whole table for `Visibility`/glyph idioms): actions `Save`, `Cancel`, `Activate` (Enter), `FocusNext`/`FocusPrev`, `ScrollUp`/`ScrollDown`. Every key the old `match` handled maps 1:1 — enumerate the old arms first and write the table to cover exactly them (S/s, C/c, Esc, ↑/k/K, ↓/j/J, Tab/BackTab/←/→/h/l, Enter). Scroll keys get `Visibility::HiddenAlias` if the visible scroll hint comes from `SCROLL_HINT_KEYMAP` (match how other scrollable dialogs advertise scrolling — read `container_info.rs`'s hint composition).

### Step 2: Dispatch through it

Rewrite `handle_key` to look up the action and act: Save→`Commit(SaveChoice::Save)`, Cancel→`Cancel`, Activate→commit per focus, FocusNext/Prev→`self.focus = self.focus.next()/.prev()` (ButtonFocus), Scroll→`scroll_preview_by(±1)`. Delete the raw `match key.code`.

**Verify**: `cargo nextest run -p jackin-console` → all existing confirm-save tests pass UNCHANGED (semantics identical).

### Step 3: Derive the hints

Add `pub fn confirm_save_hint_spans(state: &ConfirmSaveState<M>) -> Vec<HintSpan>` = keymap hints + `SCROLL_HINT_KEYMAP.hint_spans_for_axes(state.scroll_axes())`. Point the footer-hints arm at it; delete the hand-assembled span list.

**Verify**: footer-hint tests updated only where ordering/glyphs legitimately changed by derivation; `cargo nextest run -p jackin-console` → pass.

## Test plan

- Existing confirm-save behavior tests unchanged through Step 2 — that is the proof of 1:1 key mapping.
- New: table test iterating every binding in `CONFIRM_SAVE_KEYMAP` asserting dispatch produces the same outcome as the pre-change behavior for that key (write it against the new API; the key list comes from the old match arms).
- New: `confirm_save_hint_spans` contains a save hint, a cancel hint, and scroll hints exactly when `scroll_axes()` is non-empty.

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0
- [ ] `rg 'match key.code' crates/jackin-console/src/tui/components/confirm_save.rs` → 0
- [ ] `rg 'CONFIRM_SAVE_KEYMAP' crates/jackin-console/src` → table + dispatch + hints
- [ ] `ConfirmSaveFocus: ButtonFocus` impl exists
- [ ] `plans/README.md` updated

## STOP conditions

- An old match arm does something not expressible as a single keymap action (compound behavior) — report the arm.
- Plan 003 not landed (no `ButtonFocus`) — implement cycling locally is NOT acceptable; wait or land 003 first.

## Maintenance notes

- Reviewer: diff of `handle_key` should show pure table dispatch; any residual `KeyCode::` literal is a miss.
- Future keybinding changes to confirm dialogs: check this keymap too until/unless a shared confirm-action table is factored (deferred — needs the Ctrl+Q-cancel decision applied here deliberately, which changes behavior and so is not part of this refactor).
