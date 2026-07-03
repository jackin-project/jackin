# Plan 002: One uniform component API for the shared jackin-tui crate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/ docs/content/docs/reference/tui/components.mdx`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/001-tui-docs-catalog-lookbook-truth.md (catalog must be truthful before the contract section is added to it)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

The shared `jackin-tui` component crate — consumed by 7 crates (capsule, console, root console, launch TUI, diagnostics, host, runtime) — has **four** render invocation shapes, **three** construction styles, and inconsistent widget-vs-free-fn packaging. There is no single answer to "how do I use a jackin-tui component"; new code copies whichever nearby style it sees, which is how style divergence spreads to the surface crates. This plan defines ONE canonical component contract, documents it in the canonical TUI design docs, and converges the shared crate's existing components onto it. Surface-crate call sites are updated only where a converged signature forces it.

## Current state

All in `crates/jackin-tui/src/components/` unless noted. The four render shapes today:

1. `impl Widget` (`render(self, area, buf)`): `hint_bar.rs:32`, `status_footer.rs:214`, `brand_header.rs:23`, `filter_input.rs:23`, `error_dialog.rs:89`, `text_input.rs:399`, `select_list.rs:285`, `modal_backdrop.rs:16`, `scrollable_panel.rs:557` (`FixedScrollbar`).
2. Inherent `.render(self, frame: &mut Frame, area)`: `tab_strip.rs:47`, `button_strip.rs:66`.
3. Inherent `.render(self, buf: &mut Buffer, area)` (note flipped arg order vs Widget): `scrollable_panel.rs:440` and `:476` (`ScrollableList`).
4. Free `render_x(frame, area, &State)`: `confirm_dialog.rs:283`, `save_discard_dialog.rs:165`, `status_popup.rs:30`, `container_info.rs:419`, `diff_view.rs:305` (takes `&mut DiffViewState`), `error_dialog.rs:155/162`, `text_input.rs:477/497`, `select_list.rs:392/469/550`, `hint_bar.rs:48/52`, `status_footer.rs:314`, `brand_header.rs:46`, `filter_input.rs:50`, `toast.rs:68`.

Packaging split: `TextInput`/`ErrorDialog`/`SelectList` each expose BOTH a borrow-state `Widget` (`text_input.rs:399`, `error_dialog.rs:89`, `select_list.rs:285`) AND `render_*` free fns, while the structurally identical `ConfirmState`/`SaveDiscardState`/`StatusPopupState`/`ContainerInfoState`/`DiffViewState` ship only `State` + free fn.

Key-event divergence:

```rust
// confirm_dialog.rs:202
pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool>
// save_discard_dialog.rs:134
pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveDiscardChoice>
// text_input.rs:339
pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String>
// select_list.rs:188
pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<usize>
// container_info.rs:215 and :229 — TWO entry points
pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<()>
pub fn handle_key_in_rect(&mut self, key: KeyEvent, dialog_rect: Rect) -> ModalOutcome<()>
// error_dialog.rs:69 — &self (interior mutability via Cell), not &mut self
pub fn handle_key(&self, key: KeyEvent) -> ModalOutcome<()>
```

`diff_view.rs` has no `handle_key` at all (bespoke `scroll_up`/`scroll_down`/`page_up`/`page_down` around lines 182–203). `ModalOutcome<T>` is defined at `lib.rs:63` (`Continue | Commit(T) | Cancel`). The TEA runtime contract in `src/runtime.rs` (`Dirty`, `NoEffect`, `SubscriptionPoll`) is implemented by zero shared components — it serves the surface-level update loops, not components; do not force components onto it.

Duplicate helper (verified byte-identical bodies, intentionally distinct semantics):

```rust
// panel.rs:113-117
pub fn unfocused_block<'a>() -> Block<'a> {
    Block::default().borders(Borders::ALL).border_style(PanelFocus::Unfocused.border_style())
}
// panel.rs:126-130
pub fn modal_block_inactive<'a>() -> Block<'a> {
    Block::default().borders(Borders::ALL).border_style(PanelFocus::Unfocused.border_style())
}
```

Design canon that binds this plan (quote, from `docs/content/docs/reference/tui/components.mdx`):
- Rule 1: "Every visual pattern that appears in more than one place must use one shared implementation."
- Rule 5: "Refactoring to enable reuse is not optional."
- Rule 7: "Component APIs follow the TUI boundary… When a component needs work done, it emits an outcome/message that update turns into a typed effect."

## The canonical contract (the target)

Add this as a new `### Component API contract` subsection in `components.mdx` and converge code to it:

1. **Stateful interactive component** = `pub struct XState` (owns UI state only) + `pub fn render_x(frame: &mut Frame<'_>, area: Rect, state: &XState)` (or `&mut XState` only when render must clamp scroll state — document per component why) + `pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<Self::Commit-equivalent>` on the state. No parallel `impl Widget` wrapper.
2. **Pure value widget** (no interaction, borrows or owns display data) = `impl Widget`, plus at most one thin `render_x(frame, area, ...)` convenience that delegates.
3. **Layout/primitive helpers** (rect math, scrollbars, blocks) = free functions; no widget types.
4. `handle_key` always takes `&mut self` and returns `ModalOutcome<T>`; scroll/mouse input goes through separately-named `on_mouse_*`/`handle_scroll` methods, never through a second `handle_key_*` variant.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Format | `cargo fmt --check` | exit 0 |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Shared crate tests | `cargo nextest run -p jackin-tui` | all pass |
| Full workspace tests | `cargo nextest run` | all pass |
| Lookbook regen check | `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` | exit 0 |
| Find call sites | `rg 'TextInput::new|ErrorDialog::new|SelectList::new|handle_key_in_rect|modal_block_inactive|ScrollableList' crates/` | (survey tool) |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/*.rs` (signature convergence)
- `crates/jackin-tui/src/components.rs` (re-exports)
- Call-site updates in `crates/jackin-capsule/`, `crates/jackin-console/`, `crates/jackin-launch-tui/`, `crates/jackin/src/console/`, `crates/jackin-tui-lookbook/` — mechanical signature fixes ONLY
- `docs/content/docs/reference/tui/components.mdx` (new contract subsection)
- `docs/content/docs/reference/getting-oriented/codebase-map.mdx` (only if module structure changes — it should not)

**Out of scope** (do NOT touch):
- `crates/jackin-tui/src/runtime.rs` — the TEA loop contract is a different layer; components do not implement it.
- Any behavior change: colors, layout, key semantics, focus order. This plan is signature/packaging convergence only.
- `error_dialog.rs` internal render body (plan 007 rebuilds it on the dialog shell — here you only touch its `handle_key` receiver and remove its redundant `Widget` wrapper IF plan 007 has not landed; if 007 landed first, reconcile with its shape).
- `text_input.rs` cursor-style unification (plan 004).
- The five focus enums (plan 003).

## Git workflow

- Branch (operator confirms first): `refactor/tui-component-contract`.
- Conventional Commits + DCO + push each commit: `git commit -s -m "refactor(tui): converge shared components on one API contract" && git push`.
- TUI-touching PRs must update the matching `docs/content/docs/reference/tui/` page in the same PR (CLAUDE.md TUI rule) — Step 1 covers this.

## Steps

### Step 1: Write the contract into the design docs

Add the `### Component API contract` subsection (content from "The canonical contract" above, adapted to docs prose) under `## Component Reuse — Hard Rule` in `docs/content/docs/reference/tui/components.mdx`.

**Verify**: `grep -n 'Component API contract' docs/content/docs/reference/tui/components.mdx` → 1 match.

### Step 2: Kill the duplicate block helper

In `panel.rs`, delete `modal_block_inactive` (lines 126–130) and re-point its callers to `unfocused_block`, preserving the stack-dialog doc comment by merging it into `unfocused_block`'s docs ("also used for background modals in a dialog stack — exactly one PHOSPHOR_GREEN border visible at a time").

**Verify**: `rg 'modal_block_inactive' crates/` → no matches; `cargo nextest run -p jackin-tui` → pass.

### Step 3: Converge inherent renders onto the canonical shapes

- `tab_strip.rs:47` and `button_strip.rs:66`: these are pure value widgets — convert `.render(self, frame, area)` to `impl Widget for TabStrip`/`ButtonStrip` (`render(self, area, buf)`), keep `button_strip.rs:74 line()` as-is, and update call sites (`rg 'TabStrip|ButtonStrip' crates/` to enumerate; callers switch from `x.render(frame, area)` to `frame.render_widget(x, area)`).
- `scrollable_panel.rs:440/476` (`ScrollableList::render`/`render_with_block` taking `(buf, area)`): flip to `(area, buf)` via `impl Widget` (and a `render_with_block` keeping explicit args but in `(area, buf, block)` order), so no API in the crate takes `(buf, area)`.

**Verify**: `rg 'fn render\(self, buf' crates/jackin-tui/src` → no matches; `cargo nextest run -p jackin-tui` and workspace `cargo clippy --all-targets --all-features -- -D warnings` → pass.

### Step 4: Remove the redundant Widget wrappers on stateful components

Delete `impl Widget` + wrapper structs `TextInput` (`text_input.rs:388-399` region), `SelectList` (`select_list.rs:253-285` region), `ErrorDialog` (`error_dialog.rs:78-127`) IF their only production callers are the sibling `render_*` free fns — verify with `rg 'TextInput::new|SelectList::new|ErrorDialog::new' crates/ --type rust` first. Fold the widget body into the corresponding `render_*` free fn. If a surface crate constructs the widget directly, convert that call site to the free fn.

**Verify**: `rg 'struct TextInput<|struct SelectList<|struct ErrorDialog<' crates/jackin-tui/src` → no matches; `cargo nextest run` (workspace) → pass; lookbook `--check` → exit 0 (stories may need the free-fn form — update stories, SVG output must stay identical; if SVGs change, you altered behavior — STOP).

### Step 5: Normalize `handle_key`

- `error_dialog.rs:69`: change `pub fn handle_key(&self, …)` to `&mut self`, replace the internal `Cell` with a plain field, update callers.
- `container_info.rs:229`: fold `handle_key_in_rect` into a single `handle_key(&mut self, key)` plus a separately-named `set_viewport(dialog_rect)` (or `handle_key_with_viewport` → decide by reading how the two entry points differ: open `container_info.rs:215-260`; the `_in_rect` variant only adds rect-aware scroll clamping — so give the state a stored viewport set during render, and delete the second entry point).
- `diff_view.rs`: add `pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<()>` on `DiffViewState` that maps Up/Down/PgUp/PgDn (and `j`/`k` if other scrollable components map them — check `container_info.rs` handle_key for the precedent) onto the existing `scroll_up`/`scroll_down`/`page_up`/`page_down`, returning `ModalOutcome::Continue` (Esc → `Cancel`). Migrate the launch caller (`rg 'scroll_up|page_down' crates/jackin-launch-tui/src/tui/run.rs`) to it.

**Verify**: `rg 'fn handle_key\(&self' crates/jackin-tui/src` → no matches; `rg 'handle_key_in_rect' crates/` → no matches; `cargo nextest run` → pass.

### Step 6: Re-export audit + full sweep

Ensure `components.rs` re-exports exactly the surviving public API (no dangling names). Run the full gate: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo nextest run && cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook`.

## Test plan

- No new behavior → no new behavior tests. The existing `jackin-tui` unit tests + capsule/console snapshot tests + lookbook SVG `--check` ARE the regression net; all must stay green with **zero SVG diffs**.
- Add one compile-time-style test (or doc test) in `crates/jackin-tui/src/tests.rs` asserting the contract examples compile: construct `ConfirmState`, call `handle_key`, match `ModalOutcome` — model it after existing tests in that file.

## Done criteria

- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest run` all exit 0
- [ ] `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` exits 0 with no regenerated diffs
- [ ] `rg 'fn render\(self, buf' crates/jackin-tui/src` → 0 matches
- [ ] `rg 'handle_key_in_rect|modal_block_inactive' crates/` → 0 matches
- [ ] `rg 'fn handle_key\(&self,' crates/jackin-tui/src` → 0 matches
- [ ] `components.mdx` contains the Component API contract subsection
- [ ] `plans/README.md` status row updated

## STOP conditions

- Any lookbook SVG changes after Step 3/4 — signature work must be pixel-neutral.
- A surface crate uses `TextInput`/`SelectList`/`ErrorDialog` widget types in a way the free fn cannot express (e.g. embedding in a generic `Box<dyn Widget>`) — report, do not redesign.
- `container_info.rs` `handle_key_in_rect` turns out to do more than rect-aware scroll clamping.
- Plan 007 (error-dialog rebuild) landed first and reshaped `error_dialog.rs` — reconcile by applying only the `&mut self` rule to whatever shape exists, and note it.

## Maintenance notes

- Future components must match the contract subsection added in Step 1; reviewers should reject PRs introducing a new render shape.
- Plans 003 (focus), 004 (text_input/diff_view drift), 007 (error dialog) build on this; land this first.
- Deferred deliberately: making components implement `runtime::Component` — that contract is for surface update loops, not widgets.
