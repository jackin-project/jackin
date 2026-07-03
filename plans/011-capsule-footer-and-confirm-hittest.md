# Plan 011: Capsule footer through shared StatusFooter; shared confirm-button hit-test

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-capsule/src/tui/components/ crates/jackin-tui/src/components/status_footer.rs crates/jackin-tui/src/components/confirm_dialog.rs crates/jackin-tui/src/components/button_strip.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: none hard (order-flexible with 010; both touch `chrome.rs` — coordinate)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: DONE

## Why this matters

Two independent forks in the capsule chrome. (1) The white status/footer bar: the shared `StatusFooter` widget paints left|usage|container|debug-chip and its code comment *promises* "the operator sees the same chip on every surface" — yet the capsule hand-paints the same bar with its own constants, including hover colors that exist nowhere in the theme (`Rgb(225,245,255)`/`Rgb(0,55,140)`), so hover feedback visibly differs per surface and every footer fix lands twice. (2) The capsule's ConfirmAction click handler re-derives Yes/No button geometry from local string literals that must stay accidentally identical to `ButtonStrip`'s padding — a silent-misclick trap. Fix: extend the shared widget with what the capsule genuinely needs (clickable left segment + hover), render through it; expose one shared button hit-test.

## Current state

Capsule fork, `crates/jackin-capsule/src/tui/components/chrome.rs`:

```rust
// chrome.rs:187-191
const BAR_BG: Color = color(jackin_tui::WHITE);
const BAR_FG: Color = color(jackin_tui::BLACK);
const BAR_LINK_FG: Color = color(jackin_tui::LINK_BLUE);
const BAR_HOVER_BG: Color = Color::Rgb(225, 245, 255);   // ← not in any theme
const BAR_HOVER_FG: Color = Color::Rgb(0, 55, 140);      // ← not in any theme
// chrome.rs:299-320 render_branch_bar_row(buf, area, branch, usage_status_label,
//   pull_request, pull_request_loading, debug_run_id, instance_id_label, hover_target)
//   — hand-paints the full bar via branch_context_bar_layout
```

The capsule already shares the *geometry*: `crates/jackin-capsule/src/tui/components/branch_context_bar.rs:85-92` calls the shared `status_right_group_layout(term_cols, StatusRightGroup { usage, container, run_id })` — only the *paint* is forked.

Shared widget, `crates/jackin-tui/src/components/status_footer.rs`:

```rust
// :145-150  StatusFooter::new(left: &'a str) — left is a PLAIN &str (no click/hover
//           modeling for it beyond StatusFooterHover.left)
// :214      impl Widget for StatusFooter
// :255-262  debug chip: "identical to the console's render_debug_bar so the operator
//           sees the same chip on every surface. Inverted on hover (white bg, red
//           text)" — capsule's fork uses different hover colors, breaking this.
```

The capsule bar's extras the shared widget lacks: branch/PR left segment with loading state (`" Resolving PR · {b} "` / `" Branch · {b} "` at `branch_context_bar.rs:80-83`), per-chunk click regions (`branch_context_bar_hit`), instance-id label, hover per `HoverTarget`.

Confirm hit-test fork, `crates/jackin-capsule/src/tui/components/dialog.rs:995-1020`:

```rust
if let Self::ConfirmAction { kind, .. } = self {
    const YES_LABEL: &str = "  Yes  ";
    const GAP: &str = "    ";
    const NO_LABEL: &str = "  No  ";
    ...
    let button_row = box_row + height.saturating_sub(2);
    // manual yes_start/yes_end/no_start/no_end column math
```

The same dialog renders via shared `render_confirm_dialog` → `ButtonStrip` (pads `"  {label}  "`, default gap `"    "` — `button_strip.rs:50,91` area). Neither `confirm_dialog.rs` nor `button_strip.rs` exposes a hit-test today. Risk case: the data-loss confirm renders through a taller details path (`confirm_dialog.rs:389` area) while the click handler assumes flat `height-2`.

Docs canon: `components.mdx` rule 5 (extend, add the parameter); `chrome.mdx` (status bar rules).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui -p jackin-capsule` then `cargo nextest run` | pass |
| Lookbook | regen + `--check` | exit 0 (status-footer story may change if left-segment API alters defaults — it must not) |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/status_footer.rs` — extend: structured left segment (text + optional link/loading style + hover flag), theme-token hover colors
- `crates/jackin-tui/src/theme.rs` — name the two hover colors (`FOOTER_HOVER_BG`, `FOOTER_HOVER_FG`) IF the design decision is to keep them; see Step 1 decision
- `crates/jackin-capsule/src/tui/components/chrome.rs` — delete bar painting; render `StatusFooter`
- `crates/jackin-capsule/src/tui/components/branch_context_bar.rs` — keep hit-region derivation, source rects from the shared layout
- `crates/jackin-tui/src/components/confirm_dialog.rs` — add `pub fn confirm_button_hit(dialog: Rect, state: &ConfirmState, col: u16, row: u16) -> Option<bool>` (true=Yes) built on ButtonStrip metrics
- `crates/jackin-tui/src/components/button_strip.rs` — expose button rect metrics (`pub fn button_rects(&self, area: Rect) -> Vec<Rect>`)
- `crates/jackin-capsule/src/tui/components/dialog.rs:995-1020` — replace literal math with the shared hit-test

**Out of scope**:
- Hint row rendering (plan 010).
- Console/launch footers (already on the shared widget).
- `HoverTarget` model semantics.

## Git workflow

Branch (operator confirm): `refactor/capsule-footer-shared`. `git commit -s` + push. Update `chrome.mdx` status-bar section in same PR (hover colors now uniform + tokenized).

## Steps

### Step 1: Decide hover colors once (design decision, pre-made)

The shared chip hovers `DEBUG_AMBER`/inverted; the capsule invented light-blue. **Decision: the shared widget's existing hover behavior wins** (it is the documented "same chip on every surface" contract). The capsule's two `Rgb` literals are deleted, not tokenized. If the capsule's non-chip left-segment hover needs a highlight, use existing tokens (`LINK_FG_HOVER`, `TAB_BG_INACTIVE_HOVER`) — no new colors.

### Step 2: Extend `StatusFooter` with a structured left segment

Replace `left: &'a str` with a small `FooterLeft<'a>` (default-compatible): `text: &'a str`, `kind: Plain | Link` (Link styles like the container link `LINK_BLUE`), keeping `StatusFooter::new(&str)` as a convenience for Plain. Existing callers unchanged (`rg 'StatusFooter::new' crates/`). Hover for left already exists (`StatusFooterHover.left`).

**Verify**: `cargo nextest run` → pass; lookbook `--check` → 0 diffs.

### Step 3: Render the capsule bar through `StatusFooter`

In `chrome.rs`, rewrite `render_branch_bar_row` to build `StatusFooter` (left = the branch/PR string from `branch_context_bar_layout`, right group = usage/container/run_id it already computes, hover mapped from `HoverTarget`) and render it into the row (widget render into `buf` — `StatusFooter` implements `Widget`, so `footer.render(row_area, buf)` works in the compositor path; colors: the capsule composites shared-theme colors through its host palette — read how `chrome.rs` currently remaps and apply the same at the boundary, or confirm the compositor accepts theme colors directly by checking what the tab strip (already shared) does in the same file). Delete `BAR_BG/BAR_FG/BAR_LINK_FG/BAR_HOVER_BG/BAR_HOVER_FG`. Keep `branch_context_bar_hit` for click regions, deriving chunk rects from `status_right_group_layout` + the left width — no second painted layout.

**Verify**: `rg 'BAR_HOVER|Rgb\(225, 245, 255\)|Rgb\(0, 55, 140\)' crates/jackin-capsule/src` → 0; `cargo nextest run -p jackin-capsule` → snapshot diffs confined to hover-color cells (the intended fix) — update deliberately.

### Step 4: Shared confirm hit-test

Add `button_rects` to `ButtonStrip` (compute the same x-offsets `render`/`line()` produce — single source: refactor render to use `button_rects` internally so paint and hit-test cannot diverge). Add `confirm_button_hit(dialog_area, state, col, row) -> Option<bool>` in `confirm_dialog.rs` that accounts for the details/data-loss variant's taller layout (read `confirm_dialog.rs` render paths around `:283` and `:389` — the button row position must come from the same `dialog_inner_chunks` slot the render uses). Replace `dialog.rs:995-1020`'s literal math with a call to it, mapping `Some(true)→ConfirmedAction(*kind)`, `Some(false)→Dismiss`, `None` inside dialog→`Consume`.

**Verify**: `rg 'YES_LABEL|"  Yes  "' crates/jackin-capsule/src` → 0; new tests below pass; `cargo nextest run` → pass.

## Test plan

- `button_strip.rs`: `button_rects` matches painted cells — render into a Buffer, assert each labeled button's cells fall inside its rect (both default and custom gap).
- `confirm_dialog.rs`: hit-test on the data-loss (details) variant — click at the rendered Yes cell returns `Some(true)` (this is the live-drift risk the old code had).
- Capsule: existing bar snapshot tests updated once for hover colors; a click-region test asserting `branch_context_bar_hit` regions align with the shared layout chunks.

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0
- [ ] `rg 'Rgb\(' crates/jackin-capsule/src/tui/components/chrome.rs` → 0
- [ ] Capsule bar painted by `StatusFooter` (grep for the render call)
- [ ] `rg 'YES_LABEL' crates/` → 0
- [ ] Lookbook `--check` exits 0
- [ ] `chrome.mdx` updated; `plans/README.md` updated

## STOP conditions

- The compositor path cannot render a ratatui `Widget` into its buffer where the bar row lives (only raw `set_string` supported) — report; fallback is `StatusFooter` exposing a `line()`-style span builder like `ButtonStrip::line()`, but that API addition needs a moment of design (keep paint + widget single-sourced).
- `StatusFooter` cannot express the capsule's instance-id label placement without changing console/launch output — report the layout conflict.
- Capsule PTY snapshot fixtures assert exact bar bytes beyond color (layout shifts) — report scope.

## Maintenance notes

- Reviewer: `ButtonStrip::render` must consume `button_rects` internally — if paint and hit-test are computed twice, the plan failed its point.
- Plan 010 also edits `chrome.rs`; whichever lands second rebases carefully around the bottom-chrome rows.
- Deferred: click support for console footer left segment (console has no clickable left today; the `FooterLeft` kind covers it when needed).
