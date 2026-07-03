# Plan 010: One hint renderer — capsule hints through shared hint_bar styling, wrapped not truncated

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-capsule/src/tui/components/chrome.rs crates/jackin-tui/src/components/hint_bar.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none hard (coordinate with plan 005 glyphs — flexible order)
- **Category**: tech-debt + bug (separator color drift is visible today; truncation violates the no-hidden-keys rule)
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

The workspace has exactly two hint renderers for one shared `HintSpan` vocabulary: the shared `hint_bar` (console, launch, host overlay) and a hand-painted capsule fork. The fork already drifted — the `·` separator renders `PHOSPHOR_DARK` green in the capsule vs the canonical gray (`theme::BORDER`) everywhere else — and its single-row truncation silently drops active keys on narrow terminals, violating the documented "exhaustive — no hidden keys" rule that the console honors by wrapping. This plan extracts the span→style mapping into one shared function, fixes the color drift, and makes the capsule hint area wrap like every other surface.

## Current state

The shared renderer, `crates/jackin-tui/src/components/hint_bar.rs`:

```rust
// hint_bar.rs:57-70 — THE canonical style map
pub fn line(spans: &[HintSpan<'_>]) -> Line<'static> {
    let key = crate::theme::BOLD_WHITE;  let text = crate::theme::GREEN;
    let dim = crate::theme::DIM;         let sep = crate::theme::BORDER;   // gray
    ... Key→key, DynKey→key, Text→" {t}" text, Dyn→" {t}" dim,
        Sep→" · " sep, GroupSep→"   " raw
}
// hint_bar.rs:81+ — wrapped_lines(spans, width) -> Vec<Line>, wrapped_height(...)
//   NOTE: wrapped_lines re-declares the same four styles internally (:93-96) —
//   a second in-file copy of the map; unify it onto the extracted fn too.
```

The capsule fork, `crates/jackin-capsule/src/tui/components/chrome.rs`:

```rust
// chrome.rs:391-423 — truncate_spans_to_cols: greedy group-boundary prefix,
//   returns empty if even the first group overflows
// chrome.rs:427-436 — render_hint_spans_row: centered single row, 2-col side pads,
//   drops overflow groups entirely
// chrome.rs:444-448 — duplicated style map with the drift:
let key_style  = Style::default().fg(color(jackin_tui::WHITE)).add_modifier(Modifier::BOLD);
let text_style = Style::default().fg(color(jackin_tui::PHOSPHOR_GREEN));
let dyn_style  = Style::default().fg(color(jackin_tui::PHOSPHOR_DIM));
let sep_style  = Style::default().fg(color(jackin_tui::PHOSPHOR_DARK));   // ← drift: canon is BORDER gray
```

Capsule constraint: it paints via `buf.set_string` into a compositor buffer with a host-palette remap `color(rgb)` (from `crates/jackin-capsule/src/tui/host_colors.rs`, a re-export of the shared query) — it cannot simply call `frame.render_widget(HintBar…)` today because of the color remap and the centered/bottom-anchored geometry (`BRANCH_CONTEXT_BAR_ROWS` math at `chrome.rs:432-441`).

Docs canon: `navigation.mdx` §"Navigation hints (exhaustive — no hidden keys)"; §hint vocabulary specifies `Sep` is a **gray** `" · "`. `dialogs.mdx` §"Hints / Footer Bar": "The in-container capsule follows the same contract via its own exhaustive `Dialog::footer_hint_spans` rendered in the bottom chrome."

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui -p jackin-capsule` then `cargo nextest run` | pass |
| Lookbook | regen + `--check` | exit 0 |

## Scope

**In scope**:
- `crates/jackin-tui/src/components/hint_bar.rs` — extract the style map into a public fn parameterized by a color-remap hook
- `crates/jackin-capsule/src/tui/components/chrome.rs` — delete the duplicated map; render via the shared fn; wrap instead of truncate
- Capsule bottom-chrome height plumbing (`crates/jackin-tui/src/components/bottom_chrome.rs` constants and/or capsule `layout.rs`) — read `bottom_chrome.rs` first; it is pure rect layout with `BOTTOM_CHROME_ROWS=3`
- Capsule snapshot tests that pin the old sep color / truncation

**Out of scope**:
- Hint *builders* (`Dialog::footer_hint_spans`, console `footer_hints/*`) — vocabulary is already shared and correct.
- `StatusFooter`/branch bar (plan 011).
- Glyph spellings (plan 005).

## Git workflow

Branch (operator confirm): `refactor/capsule-hint-renderer`. `git commit -s` + push. Update `navigation.mdx` hint section in same PR (capsule now wraps; note the styling single-source).

## Steps

### Step 1: Extract the canonical span→style mapping

In `hint_bar.rs`, add:

```rust
/// The one span→styled-span mapping for hint rendering. `remap` lets
/// compositor surfaces (capsule) translate palette colors to host colors;
/// pass `|c| c` everywhere else.
pub fn styled_hint_spans(
    spans: &[HintSpan<'_>],
    remap: impl Fn(Color) -> Color,
) -> Vec<Span<'static>>
```

Body = the map from `line()` (Key/DynKey → BOLD_WHITE remapped, Text → GREEN, Dyn → DIM, Sep → `" · "` BORDER, GroupSep → `"   "` raw). Rewrite `line()` AND the internal copy in `wrapped_lines` (`:93-96`) to call it — after this the file has exactly one style map.

**Verify**: `cargo nextest run -p jackin-tui` → pass; lookbook `--check` → 0 diffs (identity remap is behavior-neutral).

### Step 2: Point the capsule at it (fixes the sep color)

In `chrome.rs`, delete the four local style consts and the per-span match inside `render_hint_spans_row`; build spans via `styled_hint_spans(visible, |c| color_remap(c))` where the remap maps each shared theme Color through the capsule's `color()` — note the current code remaps `Rgb` inputs, not `Color`; read `crates/jackin-capsule/src/tui/host_colors.rs` and adapt: if `color()` takes `Rgb`, add a small `remap_color(Color) -> Color` in the capsule that matches known theme colors to their host equivalents the same way current call sites do. The painted output must equal today's EXCEPT the separator becomes BORDER gray (the fix). Keep `buf.set_string` painting and centering math.

**Verify**: `cargo nextest run -p jackin-capsule` — snapshot diffs confined to separator-color cells; update them as the intended fix.

### Step 3: Wrap instead of truncate

Replace the truncation path: compute `wrapped_height(spans, width)` (shared, `hint_bar.rs:77`) and render `wrapped_lines`' output rows into the hint area. This requires the capsule bottom chrome to grant a variable-height hint region: read how `BOTTOM_CHROME_ROWS` flows (`crates/jackin-tui/src/components/bottom_chrome.rs`, capsule `layout.rs`, `view.rs` bottom-chrome calls) and thread `hint_rows: u16` through the layout so the pane body shrinks accordingly. Delete `truncate_spans_to_cols` when nothing references it. Cap wrap at 3 rows (matches console behavior bounds; if console has a different cap, read `render_wrapped_hint_bar` usage in `crates/jackin-console/src/tui/view.rs:353-365` and match it).

**Verify**: new tests below; `cargo nextest run` → pass.

## Test plan

- `hint_bar.rs`: test `styled_hint_spans` with identity remap equals `line()` spans (style + content per variant).
- Capsule: test narrow-width hint rendering — at a width where the old code dropped a group, assert all groups now appear across ≥2 rows (drive through the chrome render into a `Buffer`; model on existing chrome tests — `rg 'mod tests' crates/jackin-capsule/src/tui/components/chrome.rs`).
- Capsule: separator cell style equals remapped BORDER gray.
- Update snapshots pinned to the old single-row layout deliberately, one commit, so the diff is reviewable.

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0
- [ ] `rg 'PHOSPHOR_DARK' crates/jackin-capsule/src/tui/components/chrome.rs` → 0 (in hint context)
- [ ] `rg 'truncate_spans_to_cols' crates/` → 0
- [ ] Exactly one span→style map in the workspace: `rg -l 'HintSpan::Sep =>' crates/` → only `hint_bar.rs`
- [ ] Lookbook `--check` exits 0
- [ ] `navigation.mdx` updated; `plans/README.md` updated

## STOP conditions

- The capsule compositor's byte-level snapshot tests (daemon/compositor) assert fixed bottom-chrome height in the socket protocol — variable height would change the protocol surface. Report; Step 3 may need to land as its own PR after an operator decision, with Steps 1–2 (styling single-source + color fix) still shipping.
- The host-color remap cannot express a shared theme color (no mapping exists for BORDER gray in the capsule palette query) — report rather than inventing a color.
- Any lookbook SVG diff (shared-crate behavior must not change).

## Maintenance notes

- After this, hint styling changes happen in one function; reviewers reject any new per-surface style map.
- Plan 005 (glyphs) touches neighboring literals — re-run its survey grep after merging both.
- Deferred: making the capsule render through `HintBar` the widget (needs Frame plumbing in the compositor path); the styling fn is the pragmatic single source.
