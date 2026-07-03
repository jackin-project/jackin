# Plan 004: Fix in-crate drift — one text-input cursor style, diff_view on the keymap + palette

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/components/text_input.rs crates/jackin-tui/src/components/diff_view.rs crates/jackin-tui/src/theme.rs crates/jackin-tui/src/keymap.rs`
> On mismatch with "Current state" excerpts: STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (independent of 002; if 002 landed, respect its signatures)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

Two verified drifts live *inside* the shared crate — the very place that defines the standard. (1) The canonical single-value input box renders with two different cursor styles and two different border stacks depending on entry point, so the "same meaning, same shape" contract breaks inside one component. (2) `diff_view` hand-builds its hint spans, bypassing `SCROLL_HINT_KEYMAP` (already spelling `PgUp PgDn` while other surfaces write `PgUp/PgDn`), and hardcodes raw `Color::Rgb`/`Color::Red`/`Color::Green` outside the theme palette. Fixing these in the shared crate removes drift that every consumer inherits.

## Current state

`crates/jackin-tui/src/components/text_input.rs` — two render paths for the one canonical input box (`components.mdx` §"One input-box dialog for every single-value prompt"):

```rust
// text_input.rs:458-465 — path A (render_input_value, used by TextInput/render_text_input)
let base_style = crate::theme::GREEN.bg(INPUT_BG_DIM);           // dim band
let cursor_style = Style::default()
    .bg(WHITE).fg(Color::Black)
    .add_modifier(Modifier::SLOW_BLINK);                          // white blinking cursor
// text_input.rs:532-535 — path B (render_input_value_from_parts, used by
// render_labeled_text_input_dialog at :497)
let cursor_style = Style::default()
    .fg(Color::Black).bg(PHOSPHOR_GREEN)
    .add_modifier(Modifier::BOLD);                                 // green bold cursor, NO band
```

Border stacks also differ: path A hand-rolls `Block::default().borders(ALL)` (around `text_input.rs:408`, with `.style(bg(INPUT_BG_DIM))` at `:433`), path B uses the shared `Panel` (`text_input.rs:506`).

`crates/jackin-tui/src/components/diff_view.rs`:

```rust
// diff_view.rs:15-18 — palette bypass
const DIFF_REMOVED_BG: Color = Color::Rgb(60, 20, 20);
const DIFF_ADDED_BG:  Color = Color::Rgb(20, 50, 20);
// plus Color::Red / Color::Green used around :231-232
// diff_view.rs:341-351 — hand-built hints
pub fn diff_view_hint_spans() -> Vec<crate::HintSpan<'static>> {
    vec![ HintSpan::Key("↑↓"), HintSpan::Text("scroll"), HintSpan::Sep,
          HintSpan::Key("PgUp PgDn"), HintSpan::Text("page") ]
}
```

The keymap infrastructure that should serve this: `crates/jackin-tui/src/keymap.rs:534` `pub static SCROLL_HINT_KEYMAP: Keymap<ScrollHintAxis>` with `Keymap::hint_spans_for_axes(&self, axes: ScrollAxes)` at `:283`; its doc (`:511-531`) says "Use via `Keymap::hint_spans_for_axes`" — it exists precisely to eliminate duplicate hint gating.

Theme: all palette tokens are `pub const` in `crates/jackin-tui/src/theme.rs` (e.g. `DANGER_RED:62`, `BORDER_GRAY:60`, `WARNING_YELLOW:68`); the convention is a named `_RGB` const + `color()` wrapper — follow it for new tokens.

Consumers of the input renderers: `render_text_input` (launch `prompts.rs`, others) and `render_labeled_text_input_dialog` (capsule `dialog_widgets.rs`, console modals). Enumerate live callers with `rg 'render_text_input|render_labeled_text_input_dialog' crates/ --type rust`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Format / lint | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui` then `cargo nextest run` | pass |
| Lookbook regen | `cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` then `-- --check docs/public/tui-lookbook` | exit 0 |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/text_input.rs`
- `crates/jackin-tui/src/components/diff_view.rs`
- `crates/jackin-tui/src/theme.rs` (new diff tokens)
- `docs/public/tui-lookbook/` regenerated SVGs (text-input and diff-view stories will legitimately change)
- Snapshot test updates in any crate whose snapshots pin the old cursor style (`cargo nextest run` will name them)

**Out of scope**:
- Any `handle_key`/signature work (plan 002).
- Capsule/console/launch hint builders (plans 005, 010).
- `error_dialog.rs` (plan 007).

## Git workflow

Branch (operator confirm): `fix/tui-shared-drift`. `git commit -s`, push per commit. This changes visible pixels → update `docs/content/docs/reference/tui/dialogs.mdx` §"Text input dialogs use shared widgets" only if it names the cursor style (check; likely no edit needed).

## Steps

### Step 1: Unify the input cursor/band/border

Decision (do not re-litigate): **path B's cursor (PHOSPHOR_GREEN bg, BOLD, no blink) + path A's band (`INPUT_BG_DIM`) + `Panel` border** — green cursor matches the phosphor brand; the dim band aids affordance; `Panel` is the shared border primitive. Implementation:
1. Make `render_input_value_from_parts` (`text_input.rs:532`) the single value renderer: give it the `INPUT_BG_DIM` band from path A and keep its green-bold cursor.
2. Rewrite `render_input_value` (`:458`) as a thin delegate to it (or delete it and call `_from_parts` directly — pick whichever keeps the public surface unchanged).
3. Replace path A's hand-rolled `Block` (`:408`, `:433`) with `Panel` the way `:506` does.

**Verify**: `rg 'SLOW_BLINK' crates/jackin-tui/src/components/text_input.rs` → 0 matches; `cargo nextest run -p jackin-tui` → pass; regen lookbook → text-input SVGs change (expected), `--check` then exits 0.

### Step 2: diff_view hints through the keymap

Replace the body of `diff_view_hint_spans` (`diff_view.rs:341-351`) with a composition over `SCROLL_HINT_KEYMAP.hint_spans_for_axes(...)` using the vertical-axes value (read `keymap.rs:283` and the `ScrollAxes` type for the exact call; diff view scrolls vertically + pages). The output spans must come from the registry — no `HintSpan::Key` literals left in `diff_view.rs`.

**Verify**: `rg 'HintSpan::Key' crates/jackin-tui/src/components/diff_view.rs` → 0 matches; `cargo nextest run -p jackin-tui` → pass (update the function's unit test expectations to the registry glyphs — the registry's grouped `↑↓/j/k`-style output is the new correct value).

### Step 3: diff_view colors into the theme

In `theme.rs`, add (following the `_RGB` + `color()` idiom): `DIFF_REMOVED_BG`, `DIFF_ADDED_BG` (values 60,20,20 / 20,50,20 — unchanged), and `DIFF_REMOVED_FG`, `DIFF_ADDED_FG` replacing raw `Color::Red`/`Color::Green` — for FG use the existing palette if suitable tokens exist (`DANGER_RED:62` for removed; check whether `PHOSPHOR_GREEN` reads correctly for added — if the current raw `Color::Green` is visually distinct from `PHOSPHOR_GREEN`, keep the ANSI value but as a named theme token). Point `diff_view.rs` at the tokens; delete the local consts.

**Verify**: `rg 'Color::Rgb|Color::Red|Color::Green' crates/jackin-tui/src/components/diff_view.rs` → 0 matches; full sweep: fmt, clippy, `cargo nextest run`, lookbook regen + `--check` all exit 0.

## Test plan

- Update existing `diff_view_hint_spans` test to registry output (Step 2).
- Add one test: both input entry points produce the same cursor style — render both paths into a `Buffer` for a fixed value/cursor and assert the styled cell at the cursor position matches (model buffer-assertion style on existing tests in `text_input.rs`'s test module).
- All snapshot/SVG updates reviewed as intentional (cursor + diff colors only).

## Done criteria

- [x] fmt / clippy `-D warnings` / `cargo nextest run` exit 0
- [x] `rg 'SLOW_BLINK' crates/jackin-tui/src` → 0
- [x] `rg 'HintSpan::Key' crates/jackin-tui/src/components/diff_view.rs` → 0
- [x] `rg 'Color::(Rgb|Red|Green)' crates/jackin-tui/src/components/diff_view.rs` → 0
- [x] Lookbook `--check` exits 0 after regen; only text-input/diff-view SVGs changed
- [x] `plans/README.md` updated

## STOP conditions

- Callers depend on the blink modifier semantically (grep hits for `SLOW_BLINK` outside text_input.rs in a consumer assertion).
- `ScrollAxes`/`hint_spans_for_axes` cannot express "scroll + page" for diff_view without new keymap API — report; do not extend the keymap ad hoc.
- SVG diffs appear in stories other than text-input/diff-view.

## Maintenance notes

- After this, the shared crate has zero raw `Color::` literals in `diff_view` and one input cursor; plan 006 (theme sweep) finishes the remaining hardcoded color sites crate-wide.
- Reviewer: eyeball the two changed SVG previews — cursor legibility on the dim band is the one judgment call.
