# Plan 003: One focus taxonomy — shared button-focus cycling + documented focus layers

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/components/ docs/content/docs/reference/tui/`
> On any in-scope drift, compare "Current state" excerpts to live code; mismatch = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/002-tui-component-contract.md
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

Focus is modeled several ways in the shared crate: `FocusOwner<Tab>` for screen-level tab/content ownership, `PanelFocus` for border styling, per-dialog semantic enums (`ConfirmFocus`, `SaveDiscardFocus`), a raw `focused: usize` on `ButtonStrip`, and raw bools on `TabStrip`. The per-dialog semantic enums are **good** type design (Yes/No and Save/Discard/Cancel as named states) and must stay. What is duplicated is the *behavior around them*: every button dialog re-implements next/prev cycling and enum→ButtonStrip-index mapping by hand, and nothing documents which focus layer a new component should use. This plan extracts one `ButtonFocus` behavior contract, wires the existing dialogs through it, and writes the three-layer focus taxonomy into the design docs so the next component has an obvious answer.

## Current state

- `crates/jackin-tui/src/components/focus_owner.rs:13` —
  ```rust
  pub enum FocusOwner<Tab> { #[default] TabBar, Content(Tab) }
  ```
  Screen-level owner: tab bar vs a tab's content block. Doc comment: green underline/border derivation "from this single value rather than from scattered bools."
- `crates/jackin-tui/src/components/panel.rs:12` —
  ```rust
  pub enum PanelFocus { Unfocused, Focused, FocusedScrollable }
  ```
  Pure border-style selector (`border_style()` maps to `PHOSPHOR_GREEN`/`PHOSPHOR_DARK`).
- `crates/jackin-tui/src/components/confirm_dialog.rs:119` — `pub enum ConfirmFocus { Yes, No }`, stored as `ConfirmState.focus` (`:126`). Its `handle_key` (`:202`) hand-implements Left/Right/Tab toggling between the two variants.
- `crates/jackin-tui/src/components/save_discard_dialog.rs:112` — `pub enum SaveDiscardFocus { Save, Discard, Cancel }`, `SaveDiscardState.focus`; default Cancel ("so accidental Enter does not discard work" — preserve this). Its `handle_key` (`:134`) hand-implements 3-way cycling.
- `crates/jackin-tui/src/components/button_strip.rs:40-58` — `ButtonStrip { focused: usize, .. }` with builder `focused(usize)`. Each dialog's render maps its semantic enum to this index by hand.
- `crates/jackin-tui/src/components/tab_strip.rs:36` — `focused: bool` builder (legitimate: strip-level highlight, not element focus).
- `crates/jackin-tui/src/components/status_footer.rs:20` — `StatusFooterHover { left, usage, right, right_debug: bool }` — this is **hover**, not focus; out of scope (the allow-attribute above it already documents why it is bools).
- Console `ConfirmSaveState` (`crates/jackin-console/src/tui/components/confirm_save.rs:74`) hand-implements the same two-button toggle — it is migrated by plan 014, but design the trait here so 014 can use it.

Design canon: `components.mdx` Core rule 5 ("Refactoring to enable reuse is not optional") and rule 1 (one shared implementation per repeated pattern).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Format / lint | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui` then `cargo nextest run` | pass |
| Lookbook drift | `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` | exit 0 |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/focus_owner.rs` (add the `ButtonFocus` trait + cycling helpers here — it is the focus module)
- `crates/jackin-tui/src/components/confirm_dialog.rs`, `save_discard_dialog.rs` (implement the trait; replace hand-rolled cycling)
- `crates/jackin-tui/src/components.rs` (re-exports)
- `docs/content/docs/reference/tui/navigation.mdx` (focus-taxonomy subsection)

**Out of scope**:
- `StatusFooterHover` — hover, not focus.
- `TabStrip.focused: bool` — strip-level highlight flag, correct as-is.
- Replacing the semantic enums with indices — explicitly rejected; they encode impossible-states-unrepresentable and stay.
- Console/capsule dialogs (plan 014 adopts the trait for `ConfirmSaveState`).

## Git workflow

Branch (operator confirm first): `refactor/tui-focus-taxonomy`. `git commit -s`, push after every commit. TUI docs page updated in same PR (hard rule).

## Steps

### Step 1: Add the `ButtonFocus` contract

In `focus_owner.rs`, add:

```rust
/// Focus behavior shared by every button-row dialog: a closed ring of
/// semantic focus states with a stable ButtonStrip index.
pub trait ButtonFocus: Copy + Eq {
    const RING: &'static [Self];
    /// Index into the dialog's ButtonStrip items.
    fn index(self) -> usize {
        Self::RING.iter().position(|f| f == &self).unwrap_or(0)
    }
    fn next(self) -> Self { /* ring rotate right */ }
    fn prev(self) -> Self { /* ring rotate left */ }
}
```

(Implement `next`/`prev` via `RING` position arithmetic; keep it dependency-free.)

**Verify**: `cargo nextest run -p jackin-tui` → pass (with the unit tests from the Test plan).

### Step 2: Implement for the two dialog enums and delete hand-rolled cycling

- `impl ButtonFocus for ConfirmFocus { const RING: &'static [Self] = &[Self::Yes, Self::No]; }` — then in `ConfirmState::handle_key` (`confirm_dialog.rs:202`) replace the manual Left/Right/Tab toggle arms with `self.focus = self.focus.next()` / `.prev()`. Key semantics must not change: enumerate the current arms first and map them 1:1.
- Same for `SaveDiscardFocus` with `RING = &[Save, Discard, Cancel]` in `save_discard_dialog.rs:134`. Default focus stays `Cancel` (constructor untouched).
- In both render fns, replace the hand-written enum→index match with `.focused(state.focus.index())` when building `ButtonStrip`.

**Verify**: `cargo nextest run -p jackin-tui` → pass; lookbook `--check` → exit 0 (zero SVG diffs — behavior-neutral).

### Step 3: Document the focus taxonomy

In `docs/content/docs/reference/tui/navigation.mdx`, add a `### Focus layers` subsection stating the three layers and when to use each:
1. `FocusOwner<Tab>` — screen-level: which region owns input.
2. Per-dialog semantic enum implementing `ButtonFocus` — element-level focus inside a button dialog; never a raw index or bool.
3. `PanelFocus` — derived border styling only; computed from layers 1–2, never stored as independent state.

**Verify**: `grep -n 'Focus layers' docs/content/docs/reference/tui/navigation.mdx` → 1 match; `cd docs && bun run build` → exit 0.

## Test plan

New tests in `confirm_dialog.rs`/`save_discard_dialog.rs` test modules (model after existing tests in those files):
- ring cycling: `Cancel.next() == Save`-style assertions for both enums, full ring round-trip.
- `index()` mapping matches the ButtonStrip item order used in each render fn.
- key-semantics regression: for each key the old `handle_key` matched (Left/Right/Tab/BackTab/h/l where present), assert same resulting focus as before (enumerate from the pre-change code you read in Step 2).

Verification: `cargo nextest run -p jackin-tui` → all pass including new tests.

## Done criteria

- [x] `cargo fmt --check`, clippy `-D warnings`, `cargo nextest run` all exit 0
- [x] `rg 'ButtonFocus' crates/jackin-tui/src` → trait + 2 impls
- [x] No hand-written focus-toggle `match` remains in the two `handle_key` fns (cycling goes through `next`/`prev`)
- [x] Lookbook `--check` exits 0 with zero SVG diffs
- [x] `navigation.mdx` has the Focus layers subsection
- [x] `plans/README.md` updated

## STOP conditions

- Current `handle_key` arms do something other than pure toggling (e.g. a key both moves focus and commits) — report the arm, do not guess.
- Any lookbook SVG changes.
- Plan 002 not landed and its signature changes conflict — land 002 first.

## Maintenance notes

- Plan 014 (`ConfirmSaveState`) must implement `ButtonFocus` instead of its hand-rolled toggle.
- Reviewers: check no dialog stores both a semantic focus enum AND a parallel index/bool.
- Deferred: unifying `FocusOwner` with the console's screen-level focus state — surface-level concern, revisit after plans 012/015.
