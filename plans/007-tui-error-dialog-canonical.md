# Plan 007: Rebuild ErrorPopup on the shared dialog shell and give it structured rows

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/components/error_dialog.rs crates/jackin-tui/src/components/dialog_layout.rs crates/jackin-tui/src/components/panel.rs crates/jackin-tui/src/components/button_strip.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/002-tui-component-contract.md (recommended first; reconcile if reversed)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

The docs mandate one error surface: "All error conditions surfaced to the operator must use the red-border `ErrorPopup`" (`dialogs.mdx` §"Error Surface — ErrorPopup Only"). But `ErrorPopup` itself is the only shared dialog that *forks* the canonical dialog skeleton — it hand-rolls the border block, the five-row layout, and a hand-painted OK button instead of composing `render_dialog_shell` + `dialog_inner_chunks` + `ButtonStrip` like `confirm_dialog` and `save_discard_dialog` do. It also cannot express structured content (label/value rows, hyperlinks), which is why the launch cockpit built a separate 483-line failure popup (ported in plan 008) and the capsule shows errors in a banned ephemeral banner (fixed in plan 009). This plan makes ErrorPopup compose the shared skeleton and adds an optional structured-rows mode so plans 008/009 have a real target.

## Current state

`crates/jackin-tui/src/components/error_dialog.rs`:

```rust
// error_dialog.rs:52-56
pub struct ErrorPopupState {
    pub title: String,
    pub message: String,
    cached_rows: Cell<Option<(u16, u16)>>,
}
// error_dialog.rs:89-127 (Widget impl) — hand-rolled fork:
let block = Block::default().borders(Borders::ALL)
    .border_style(Style::default().fg(DANGER_RED))
    .title(Span::styled(title, crate::theme::DANGER));
...
let chunks = Layout::default().direction(Direction::Vertical)
    .constraints([Length(1), Length(body_rows), Length(1), Length(1), Length(1)])
    .split(inner);                       // byte-equivalent of dialog_inner_chunks
...
Paragraph::new(Line::from(Span::styled("  OK  ",
    Style::default().bg(WHITE).fg(Color::Black).add_modifier(Modifier::BOLD))))
    .alignment(Alignment::Center).render(chunks[3], buf);   // hand-painted button
```

Entry points: `render_error_dialog` (`:155`, centers then delegates) → `render_error_dialog_in` (`:162`). Keymap: `ERROR_POPUP_KEYMAP` + `error_popup_hint_spans()` (`:44-49`) — already correct, keep.

The canonical skeleton in `crates/jackin-tui/src/components/dialog_layout.rs`:

```rust
// dialog_layout.rs:345 — the five slots (leading spacer/content/spacer/action/trailing)
pub fn dialog_inner_chunks(inner: Rect, content_rows: Option<u16>) -> [Rect; 5]
// dialog_layout.rs:380 — Clear + Panel block + returns inner
pub fn render_dialog_shell(frame: &mut Frame<'_>, area: Rect, title: Option<&str>) -> Rect
```

Constraint: `render_dialog_shell` hardcodes `PanelFocus::Focused` (PHOSPHOR_GREEN border) — the error dialog needs `DANGER_RED`. The shell needs a border-variant parameter (this is exactly `components.mdx` rule 5: "If an existing component does not yet accept a parameter that a new call site needs, add the parameter").

`ButtonStrip` (`button_strip.rs`): pads labels `"  {label}  "`, default 4-space gap; used by `confirm_dialog.rs:283`-area render and `save_discard_dialog.rs:165`-area render — those two renders are the composition exemplars to imitate.

Structured-rows target (what plan 008 must be able to express — from `crates/jackin-launch-tui/src/tui/components/failure_dialog.rs:40-72`, read at `a2ec1b237`): rows of `{ label: &str, value: String, copy_target: Option<...>, href: Option<String> }` — label/value pairs where a value may carry an OSC-8 hyperlink and a copy affordance.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui` then `cargo nextest run` | pass |
| Lookbook | regen + `--check docs/public/tui-lookbook` | exit 0 |
| Callers | `rg 'render_error_dialog|ErrorPopupState' crates/ --type rust` | survey |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/error_dialog.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs` (add border-variant param to `render_dialog_shell`)
- `crates/jackin-tui/src/components/panel.rs` (only if a `Panel` danger-border variant is the cleanest way to thread it)
- Lookbook error-dialog story + regenerated SVGs (visual change expected: OK button becomes a ButtonStrip button)
- Mechanical call-site updates for the shell signature change (`rg 'render_dialog_shell' crates/`)

**Out of scope**:
- Porting the launch failure popup (plan 008) or capsule banner (plan 009) — here you only build the capability.
- `ContainerInfoState`'s hyperlink handling — do not generalize from it; the rows API is defined here.
- Any change to confirm/save-discard visuals.

## Git workflow

Branch (operator confirm): `refactor/tui-error-dialog-canonical`. `git commit -s` + push. Update `docs/content/docs/reference/tui/dialogs.mdx` §Error Surface in the same PR (mention structured rows mode).

## Steps

### Step 1: Parameterize the shell border

Change `render_dialog_shell(frame, area, title)` to accept a border variant — smallest honest API: `render_dialog_shell(frame, area, title, DialogBorder::Default | DialogBorder::Danger)` (new two-variant enum in `dialog_layout.rs`; `Danger` = `DANGER_RED` border + `theme::DANGER` title style, matching the current error-dialog block exactly). Update existing callers (`rg 'render_dialog_shell' crates/`) to pass `DialogBorder::Default` — zero visual change for them.

**Verify**: `cargo nextest run` → pass; lookbook `--check` → 0 diffs so far.

### Step 2: Rebuild ErrorPopup render on the skeleton

Rewrite the error-dialog render body (currently `error_dialog.rs:89-127`) to: `render_dialog_shell(frame, area, Some(&title), DialogBorder::Danger)` → `dialog_inner_chunks(inner, Some(body_rows))` → message `Paragraph` in slot 1 → one-item `ButtonStrip` ("OK", focused) in slot 3. Keep `estimated_message_rows` sizing and `render_error_dialog`/`render_error_dialog_in` signatures. If plan 002 landed, this is the free-fn body; if not, do it inside the `Widget` impl — either way delete the hand-rolled `Block`/`Layout`/hand-painted span. Note the button visual will shift from hand-painted `"  OK  "` centered paragraph to ButtonStrip's identical `"  OK  "` chip — confirm identical padding (ButtonStrip pads `"  {label}  "`); if rendering differs by alignment, center the strip like `confirm_dialog`'s render does.

**Verify**: `cargo nextest run -p jackin-tui` → pass; regen lookbook — error-dialog SVG diff limited to (at most) button row centering; `--check` then exits 0.

### Step 3: Add structured rows mode

Extend `ErrorPopupState` with `pub rows: Vec<ErrorPopupRow>` (default empty), where:

```rust
pub struct ErrorPopupRow {
    pub label: &'static str,
    pub value: String,
    pub href: Option<String>,   // OSC-8 target; rendered as a terminal hyperlink
}
```

Render rows (label dim, value white, href-styled like existing link styling — reuse `theme::LINK_FG`) between the message and the button row; height math extends `estimated_message_rows` by `rows.len()`. Do NOT implement copy-targets/click hit-testing here — that stays surface-local in plan 008 (the popup exposes `pub fn row_value_rects(&self, inner: Rect) -> Vec<Rect>` so surfaces can hit-test without re-deriving geometry).

**Verify**: new unit tests below pass; `cargo nextest run` → pass.

### Step 4: Docs + story

- `dialogs.mdx` §"Error Surface": add one sentence — ErrorPopup supports structured label/value/href rows; long values use OSC-8 hyperlinks per the existing "Long values in dialogs" rule.
- Add a lookbook story `error-dialog/structured-rows` with 2 rows (one with href). Regen SVGs, add to the story docs page.

**Verify**: `cd docs && bun run build` → exit 0; lookbook `--check` → exit 0.

## Test plan

In `error_dialog.rs` tests (model on existing test module there):
- height math: `rows` extends the estimated height by row count.
- `row_value_rects` returns one rect per row, inside the dialog inner area, non-overlapping.
- rendering a state with rows places label text at expected buffer cells.
- regression: plain message-only popup renders with a DANGER_RED border cell and an OK chip in slot 3.

## Done criteria

- [x] fmt / clippy / `cargo nextest run` exit 0
- [x] `rg 'Layout::default' crates/jackin-tui/src/components/error_dialog.rs` → 0 matches (skeleton composed, not hand-rolled)
- [x] `rg '"  OK  "' crates/jackin-tui/src/components/error_dialog.rs` → 0 raw-painted matches (button via ButtonStrip)
- [x] `ErrorPopupRow` exported from `components.rs`
- [x] Lookbook regen + `--check` exit 0; diffs confined to error-dialog stories
- [x] `dialogs.mdx` updated; `plans/README.md` updated

## STOP conditions

- `render_dialog_shell`'s `Clear` + `Panel` behavior differs observably from the error dialog's current Clear+Block sequence (compare buffer output; if borders/title cells differ beyond color source, report).
- Existing `render_dialog_shell` callers need anything other than `DialogBorder::Default` appended.
- `estimated_message_rows` caching via `Cell` conflicts with plan 002's `&mut self` normalization — reconcile with whichever landed, note it in the PR.

## Maintenance notes

- Plans 008 (launch failure popup) and 009 (capsule spawn failure) consume this API — their needs are the acceptance test; if 008 finds the rows API insufficient (e.g. per-row copy badges), extend HERE, don't fork there.
- Reviewer: the border/title styling must remain byte-identical to today's (DANGER_RED, `theme::DANGER` title).
