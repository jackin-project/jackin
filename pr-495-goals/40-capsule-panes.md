# Goal — Phase 4: Capsule pane chrome & scrollback

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

The reference look is the **Global mounts** scrollable block. The PTY body itself (`PaneBodyWidget`, ADR-004) stays custom — only the surrounding chrome, focus palette, scrollbar, and scrollback state are in scope. **Read `adr-004-pane-body-rendering.mdx` and `visual-design.mdx` first.**

The scrollbar-overflow gate is already fixed (`tail_vertical_thumb` returns `None` at no overflow — index "Already landed"). What remains: the gray focus palette, vertical-scroll predictability, and finishing the thumb reuse.

## Tasks

| ID | Status | Files / evidence | Helper | Verify | Acceptance |
|---|---|---|---|---|---|
| `CAP-1` | pending | `crates/jackin-capsule/src/tui/components/chrome.rs:189` — `PaneBorderWidget` uses `FocusPalette::CAPSULE_PANE` (gray ramp, `crates/jackin-tui/src/components/panel.rs:59-62`) instead of the shared `PHOSPHOR_GREEN` active / inactive panel border | `Panel`, `render_scrollable_block`, default `FocusPalette` | `cargo nextest run -p jackin-capsule` | Pane border/title use jackin's standard active/inactive green, matching Global mounts. Render test compares pane border colour + title style + thumb glyphs against the shared helper output, including split panes. |
| `CAP-2` | pending | Vertical scrollback is non-monotonic / flickers. Confirmed mechanisms: `input_dispatch.rs:363,419` (`scroll_by` on `filled>0`); `compositor.rs:161-162` (`filled=0` for alt-screen); `session.rs:846-850,694` (`feed_pty` → `scroll_to_live` resets offset); `compositor.rs:584` (`append_cursor_state` keyed on `scrollback_offset!=0`) | shared tail-scroll helper in `jackin_tui::scroll` | telemetry + `cargo nextest run -p jackin-capsule` | One wheel direction produces one visible direction; offset + visible top row move monotonically per burst; live PTY output does not reset the operator's scrollback view unless content is invalidated; cursor visibility and thumb follow the post-clamp offset. |
| `CAP-3` | pending (partial) | `crates/jackin-capsule/src/tui/view.rs:178` already uses `jackin_tui::scroll::tail_vertical_thumb` for geometry, but `:192-197` hand-paints the thumb cells | `jackin_tui::scroll`, `scrollable_panel` | `cargo nextest run -p jackin-capsule` | Thumb length/position come from shared helpers (already true); decide whether the custom cell-painting is acceptable for the PTY shell or should move into a reusable "scrollable panel shell" extracted to `jackin-tui`. Document the choice. |

## Detail

### `CAP-1` — adopt the shared green palette
Route the pane border and title through `Panel` (or a shared wrapper) using the standard active/inactive green, replacing `FocusPalette::CAPSULE_PANE`. Reuse the Global-mounts focus-transfer rule: click/wheel over a scrollable pane focuses it and the previous pane loses its active border in the same frame; focusing elsewhere removes it. Non-scrollable panes must not show a focused-scroll state. If `render_scrollable_block` cannot paint terminal cells directly, extract a reusable **scrollable panel shell** into `jackin-tui` that owns chrome/focus/scrollbar/hints and accepts `PaneBodyWidget` as the body — this is the preferred shape and benefits every future pane surface. Do not change the PTY stream body semantics.

### `CAP-2` — predictable vertical scrollback (telemetry first)
Per the project's telemetry rule, **add `cdebug!` instrumentation before changing behavior**: on every pane scroll/render log focused pane id, agent label, `alternate_screen`, content/scrollback length, viewport rows/cols, the tail offset the renderer used, thumb start/len, visible top row, and the cursor-visibility decision — all in the same frame. The existing log proves wheel input and PTY feed state separately but not the renderer's consumed scroll state. Ask the operator to rerun the repro with `--debug` and share the run id; fix from that evidence.

Then normalize the wheel path: decode SGR wheel buttons into a typed direction/axis; ignore horizontal wheel for vertical scrollback unless a real horizontal pane path exists; coalesce multiple wheel events per client frame/tick into one signed delta; apply through a single shared tail-scroll helper and clamp once; keep slice + thumb + hint + cursor visibility on that one post-clamp offset; and when live PTY output arrives while `scrollback_offset != 0`, do not reset the operator's view unless the backing content is invalidated in a defined way. Tests: a same-direction burst moves top row + offset monotonically; interleaved PTY output while scrolled does not flicker back to tail until the operator scrolls/jumps there.

### `CAP-3` — finish thumb reuse
The thumb geometry already uses the shared helper; only the cell painting is local. Either accept it (custom paint is reasonable for the terminal-cell shell) with a one-line comment naming the reason, or fold it into the `CAP-1` shell extraction so the scrollbar is drawn by shared code. Decide and document — do not leave it ambiguous.

## Done definition
- `CAP-1`: shared green palette + focus transfer; comparison render test green; split panes covered.
- `CAP-2`: telemetry shipped; wheel normalized; monotonic + no-flicker tests green.
- `CAP-3`: thumb path either shared or documented-custom.
