# Goal — Phase 3: Overflow-derived scroll hints

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

Global rule (`docs/content/docs/reference/tui/navigation.mdx`): a scroll hint (`↑↓ scroll` / `←→ scroll`) may appear only when that axis actually overflows and its scrollbar is visible. The shared, correct implementation already exists — `scroll_hint_spans` / `dialog_scroll_axes` / `ScrollAxes` in `crates/jackin-tui/src/components/dialog_layout.rs`. Launch dialogs already use it. Two surfaces still emit static hints.

## Tasks

| ID | Status | Files / evidence | Helper | Verify | Acceptance |
|---|---|---|---|---|---|
| `SCR-1` | pending | `crates/jackin-capsule/src/tui/components/dialog/hint.rs:15-44` — `MAIN_VIEW_HINT` / `SCROLLBACK_HINT` are constants that always advertise `↑↓`, gated only on a `scrollback_active` boolean, never on focused-pane overflow | `ScrollAxes`; sibling `info_dialog_hint(axes)` at `hint.rs:88` is already axis-derived | `cargo nextest run -p jackin-capsule` | Capsule main/scrollback footer derives `↑↓`/`←→` from the focused pane's real content vs viewport overflow. A fit-content pane shows no scroll hint. |
| `SCR-2` | pending | `crates/jackin-console/src/tui/components/footer_hints.rs:150-193` gates the workspace block hint on focus booleans (`scroll_focused`, `show_horizontal_scroll`), not content overflow; `:374` hardcodes the trust-row `H/L scroll` | `scroll_hint_spans` / overflow facts | `cargo nextest run -p jackin-console` | Console workspace/list footer hints derive from real per-axis overflow. A focused-but-fitting block shows no scroll hint. |
| `SCR-3` | pending | Sweep for remaining literal hint strings | `scroll_hint_spans`, `ScrollAxes` | `rg "scroll" …footer/hint paths`; tests | An audit table of every scroll-hint producer with verdict (axis-derived vs static) and the fix or justification for each. No static `↑↓`/`←→`/`H/L scroll` string survives without an overflow gate. |

## Detail

### `SCR-1` — capsule main view
Replace the constant hints with an axis computation from the focused pane. The capsule already has an axis-derived helper for its info dialog (`info_dialog_hint(axes: ScrollAxes)` at `hint.rs:88`); extend the same pattern to the main/scrollback view. Derive vertical overflow from the focused pane's retained content height vs viewport height (the same gate that decides the pane scrollbar — see `CAP-3`), and horizontal from content width vs viewport width. A pane with no overflow emits no hint. Add a fit-content test asserting the footer has no scroll text.

### `SCR-2` — console workspace footer
The block hint follows focus state, so a focused block that fits its content still says `↑↓ scroll`. Route it (and the `H/L scroll` trust row at `:374`) through the same overflow facts the scrollbar uses. Where the footer currently receives booleans, give it the content/viewport extents or a `ScrollAxes` instead. Add tests for: fit-content (no hint), vertical-only, horizontal-only, both.

### `SCR-3` — sweep and prove
Enumerate every call site of `scroll_hint_spans` and `ScrollAxes`, and every literal `"scroll"` hint string across `crates/*/src`. Produce a short table in this file: producer → axis-derived? → action. This is the closeout that proves the rule holds project-wide, not just where it was already applied.

## Done definition
- `SCR-1`, `SCR-2`: axis-derived; fit-content tests green on each surface.
- `SCR-3`: audit table complete; no ungated static scroll hint remains.
