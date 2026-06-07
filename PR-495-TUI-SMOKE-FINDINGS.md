# PR #495 TUI Smoke Findings

This file is the handoff checklist for the remaining TUI issues found during the
operator's manual `--debug` smoke of PR #495 on branch `feature/tui-architecture`.

Use this as the target for a follow-up `/goal` command:

```text
/goal Follow PR-495-TUI-SMOKE-FINDINGS.md and fix all findings.
```

## Executive Summary

Fix the root causes, not nine isolated screenshots. The findings collapse into
three work groups:

1. **Shared Debug info/dialog contract is incomplete.** The row model exists, but
   render shell, footer/status preservation, hover, copy affordances, scroll, and
   hints are still wired per surface.
2. **Modal/footer layering is fragmented.** Capsule and launch still have
   full-screen blank-backdrop paths that cover status/footer chrome; console is
   closer to the intended reserved-footer model.
3. **Capsule scrollback/scrollbar rendering is unstable.** Pane scrollbar
   visibility is gated on current scrollback offset, and wheel scrollback emits
   full redraws even for saturated no-op events.
4. **Scrollable pane text selection is incomplete.** Selection copies on
   mouse-up, but the visible selection disappears immediately, there is no copied
   feedback, and drag selection does not auto-scroll beyond the viewport.

The correct outcome is a smaller number of shared primitives with stronger tests,
not more per-surface special cases.

## Ground Rules

- Agent codename for this lane: Angela.
- Read `COORDINATION.md` before editing, committing, or pushing.
- Stay on `feature/tui-architecture`; do not create a new branch.
- Do not rewrite history or force-push unless `COORDINATION.md` explicitly records it.
- Laris owns DCO, Codebook, and back-history work. Do not interfere with that lane.
- Do not remove `cargo-audit`.
- Sign off any new commit with `git commit -s` and push immediately when the fix is complete.
- Do not mutate host-side user state silently. Use the PR-scoped config/home paths from the PR template.
- Do not mark roadmap/checklist boxes `[x]` from inspection. A completed checklist item must include command output or diagnostics run ids.
- Defect 54 hardware/session checks stay `[ ]` until real run ids are captured.

Current observed checkout while this file was written:

- Branch: `feature/tui-architecture`
- Local head: `f9dafc244fb23907044bfd147713c0fe2b4eccc9`

The branch may advance. Before fixing, fetch, read `COORDINATION.md`, and verify
the current branch/head rather than assuming this exact commit is still current.

## Source Of Truth

Read these before fixing:

- `COORDINATION.md`
- `AGENTS.md`
- `docs/content/docs/reference/roadmap/post-restructure-fixes.mdx`
- `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`
- `docs/content/docs/reference/tui/index.mdx`
- `docs/content/docs/reference/tui/navigation.mdx`
- `docs/content/docs/reference/tui/dialogs.mdx`
- `docs/content/docs/reference/tui/components.mdx`

The canonical TUI design docs already exist under `docs/content/docs/reference/tui/`.
Extend those pages only after the implementation and verification prove the new
Debug info/dialog contract. Do not create a parallel TUI design page.

## Run Evidence

Operator smoke run:

- Run id: `jk-run-533476`
- Host diagnostics JSONL:
  `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.jsonl`
- JSONL `container_started` event points to capsule log:
  `/Users/donbeave/.jackin-pr-495/data/jk-paje1he3-thearchitect/state/multiplexer.log`
- Docker build log:
  `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.docker-build.log`

Important evidence commands:

```sh
RUN_JSONL=/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.jsonl

rg -n 'capsule_log|multiplexer|error|panic|resize|frame|ghost|render|t_parse|changed_rows|changed_cells|bytes_out|debug|dialog|scroll|mouse|key' "$RUN_JSONL"
rg -n '"capsule_log"|"run_log"|"diagnostics"' "$RUN_JSONL"
rg -n 'bottom-chrome|wheel dispatch|scrollback-movement|render: kind=full|render: kind=partial' \
  /Users/donbeave/.jackin-pr-495/data/jk-paje1he3-thearchitect/state/multiplexer.log
```

Observed log facts:

- The run JSONL does contain a capsule log pointer even though the helper printed
  an empty `capsule_log=` field.
- The capsule log repeatedly records both `bottom-chrome: site=ratatui` and
  `bottom-chrome: site=raw-full`, meaning chrome is being drawn through two
  paths in the same frame family.
- Scrollback wheel events produce `render: kind=full reason=scrollback-movement`
  repeatedly, including saturated no-op wheel events where `before=9 after=9`.
- The run JSONL summary contains a large `cockpit-dialog-mouse` count, and the
  early log lines show many modal mouse move / scroll events during the launch
  cockpit dialog.

## Target Architecture

The fix must converge Debug info and dialog behavior across the whole application:

- One shared Debug info data model.
- One shared Debug info dialog renderer contract.
- One shared row order and label vocabulary.
- One shared scroll implementation for vertical and horizontal overflow.
- One shared copy/hover hit-test model.
- One shared hint/footer contract.
- Status bars remain visible when the underlying screen had a status bar before
  opening the dialog.
- Surface-specific code may only provide available facts and store surface-local
  state such as scroll offsets, copied row, and hovered row.

Known shared primitives that should be reused or extended:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`
- `crates/jackin-tui/src/components/hint_bar.rs`
- `crates/jackin-tui/src/components/scrollable_panel.rs`
- `crates/jackin-tui/src/scroll.rs`
- `crates/jackin-console/src/tui/components/modal_rects.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/components/modal_layout.rs`

Avoid adding parallel dialog/backdrop/scroll/copy implementations. If a helper is
missing, add it to the shared component layer and route all surfaces through it.

## Canonical Debug Info Contract

All Debug info surfaces must use this contract. A surface may omit rows whose
data is unavailable; it must not change row order, labels, scroll behavior, copy
behavior, or footer/status behavior.

Canonical title:

- `Debug info`

Canonical row order:

1. `Container ID`
2. `jackin version`
3. `jackin-capsule`
4. `Role`
5. `Agent`
6. `Target`
7. `Run ID`
8. `Diagnostics log`

Canonical row semantics:

- `Run ID` is always the bare run id, for example `jk-run-b93735`. It is never a
  `.jsonl` path.
- `Diagnostics log` is the full diagnostics JSONL path. It is copyable and uses
  an OSC 8 `file://` hyperlink when the terminal path is known.
- `Container ID`, `Run ID`, and `Diagnostics log` are copyable whenever present.
- Other rows are informational unless a later shared contract explicitly marks
  them copyable.

Canonical copy affordance:

- Each copyable row must render an explicit, shared copy affordance on screen.
  The preferred shape is a small shared copy action/glyph/button aligned with the
  row value, plus footer text naming the active copy behavior. The weaker
  "click the value if you already know it is clickable" behavior is not enough.
- The value text itself remains clickable and hoverable.
- Hover feedback applies only to copyable value cells and the copy affordance,
  not the whole row label or blank dialog area.
- Keyboard copy must be consistent across surfaces. If Enter copies the currently
  selected/default copy target on one surface, every Debug info surface must
  expose the same model or the footer must explicitly describe the surface's
  different copy action. Prefer one shared model.

Canonical scroll behavior:

- Debug info uses both-axis dialog body scrolling.
- A horizontal scrollbar appears whenever any rendered row is wider than the
  viewport.
- A vertical scrollbar appears whenever row count exceeds the viewport.
- Scroll hints advertise only axes that actually overflow.
- Copy hit-testing, hover hit-testing, copied-row feedback, and OSC 8 hyperlink
  overlays must follow both scroll axes.

Canonical status/footer behavior:

- If the underlying screen had a status/footer bar before Debug info opened, that
  status/footer bar remains visible.
- Debug info is centered inside the content area that excludes reserved
  status/footer rows.
- Hints live in the reserved footer/hint area. Do not render floating hint lines
  under the dialog.

## Canonical Scrollable Text Selection Contract

This contract applies to scrollable terminal/pane content where the operator can
drag-select displayed text and copy it to the clipboard. The pane content is not
editable through jackin's selection layer, so selection has exactly one purpose:
copying displayed text. It must not imply editing, deletion, replacement, paste,
or mutation of pane content.

External practice references:

- W3C Selection API defines selection as the mechanism that lets users select a
  portion of content for copy, paste, and editing operations, and models the
  selection as a range with direction that user action can change:
  <https://www.w3.org/TR/selection-api/>.
- W3C Selection API user interactions say user agents should allow users to
  change the active selection and must not clear a non-empty selection just
  because the user clicks a non-editable region:
  <https://www.w3.org/TR/selection-api/#user-interactions>.
- MDN describes `Selection` as the range selected by the user, with an anchor
  where mouse selection starts and a focus where the mouse is released:
  <https://developer.mozilla.org/en-US/docs/Web/API/Selection>.
- W3C Pointer Events lists native text selection as a normal default action of
  `mousedown`, alongside drag/drop and scroll/pan behavior:
  <https://www.w3.org/TR/pointerevents/#the-mousedown-event>.
- W3C WAI technique G149 points back to user-agent accessibility expectations
  that selection/focus states are visibly highlighted:
  <https://w3c.github.io/wcag/techniques/general/G149>.

Design rule:

- The content is read-only from the selection system's point of view. The only
  supported selection action is copy.
- Do not add edit/delete/replace/cut/paste behavior to this selection model.
  Keystrokes still go to the pane normally after clearing any persisted
  selection.
- A completed drag selection remains visibly selected after mouse-up.
- Mouse-up copies the selected text to the clipboard and shows visible feedback
  that the selection was copied.
- Copied feedback must be visible in the standard chrome, not as noisy log
  output. Prefer a short footer/status message such as `Selection copied` plus,
  when practical, the selected byte/line count.
- The persisted selection is cleared by an explicit deselect action:
  - click outside the selected range or on non-selectable chrome;
  - begin typing/sending input to the pane;
  - begin a new selection;
  - close/clear the pane/session;
  - use an explicit cancel/dismiss key if one is added and shown in the footer.
- A simple mouse-up after selecting must not clear the selection.
- The selection model stores anchor and focus positions in content coordinates,
  not screen coordinates, so it survives scroll offset changes and terminal
  redraws until explicitly cleared.
- Selection rendering follows the visible viewport: only visible selected cells
  are highlighted, but the stored range may extend outside the viewport.
- Drag selection inside a scrollable pane auto-scrolls when the pointer is held
  near or beyond the top/bottom viewport edge:
  - dragging upward scrolls toward older content and extends the selection;
  - dragging downward scrolls toward newer content and extends the selection;
  - auto-scroll stops at content bounds;
  - the scroll rate may be stepped or distance-sensitive, but it must be stable,
    bounded, and testable.
- Auto-scroll during selection must not route the same event to the pane PTY.
- Selection must coexist with normal scrollback:
  - wheel scrolling after a persisted selection keeps the selected range selected
    in content coordinates;
  - typing clears selection before sending the key to the pane;
  - clicking ordinary content outside the selected range clears selection unless
    that click starts a new selection.
- Footer/hints must advertise selection/copy behavior only when relevant. For
  example, during or after a selection: `drag select`, `copied`, or
  `click/typing clears selection`, using shared hint/status vocabulary.

## Root Cause Groups

| Group | Findings | Shared fix direction |
| --- | --- | --- |
| Debug info contract | 4, 5, 6, 7, 9 | Centralize Debug info shell, rows, copy affordance, hover, scroll, footer hints, and status preservation in `jackin-tui`; surface code only supplies facts/state. |
| Dialog/footer shell | 1, 7, 8 | Introduce or extend shared modal layer helpers that receive content area + reserved footer/status area separately. Remove full-frame backdrop paths from status-preserving surfaces. |
| Capsule scroll rendering | 2, 3 | Use shared scrollability math for pane scrollbar visibility and suppress/full-reduce redraws for no-op scrollback wheel events. |
| Scrollable text selection | 2, 3, 10 | Store pane selections in content coordinates, keep selection visible after copy, show copied feedback, auto-scroll during edge drag, and clear selection only on explicit deselect/input/new selection. |

## Dependency Map

- Lock the shared Debug info contract before replacing console/launch/capsule
  renderers.
- Fix footer/status layout before finalizing build-log spacing, because build-log
  spacing should use the same footer/hint stack.
- Fix scroll helper usage before changing pane scrollbar behavior, otherwise
  scrollbar visibility and wheel dispatch can drift again.
- Fix pane selection coordinate storage before implementing drag auto-scroll,
  because auto-scroll must extend the selection in content coordinates rather
  than screen coordinates.
- Fix no-op wheel redraw before collecting performance/log evidence.
- Update TUI docs only after code and tests prove the final contract.
- Update roadmap/checklist boxes only after the required command output or live
  run ids exist.

## What Not To Do

- Do not add another Debug info renderer.
- Do not hide status/footer rows to make centering simpler.
- Do not leave console, launch, and capsule with different Debug info behavior.
- Do not solve flicker by hiding scrollbars, dropping input events, or suppressing
  debug logs.
- Do not clear a completed text selection immediately after copying it.
- Do not implement pane selection in screen coordinates; it must survive
  scrolling and redraws until explicitly cleared.
- Do not mark Defect 54 done with headless tests or code inspection.
- Do not silently weaken copy requirements to "the value is clickable if the
  operator guesses it".
- Do not introduce a broad dialog rewrite that ignores the observed smoke path.
  Keep the refactor scoped to shared primitives needed by these findings.

## Evidence To Preserve And Re-check

Use these anchors while fixing:

```sh
rg -n 'render_capsule_ratatui_frame|render_container_info_on_blank|DialogRatatuiSnapshot::DebugInfo|render_build_log_dialog|wheel dispatch|scrollback-movement|render_text_input_dialog|Run ID|Diagnostics log' crates
rg -n 'Run ID.*jsonl|Run ID.*diagnostics|render_container_info_on_blank|bottom-chrome: site=raw-full|bottom-chrome: site=ratatui' crates
```

Post-fix evidence expectations:

- No status-preserving surface should call a full-frame blank Debug info renderer.
- A `Run ID` row must never contain `.jsonl`.
- Saturated scrollback wheel events must not create `render: kind=full
  reason=scrollback-movement`.
- Shared Debug info tests must prove copy/hover/scroll behavior after horizontal
  and vertical scroll.
- Pane selection tests must prove selection remains visible after mouse-up/copy,
  clears on click/type/new selection, and auto-scrolls when drag-selecting beyond
  the viewport.
- Any remaining local dialog renderer must have a documented structural reason.

## Findings Checklist

### 1. Build Log Overlay Footer Spacing Is Wrong

Status: open.

Observed behavior:

- In the Docker build overlay, the hint row (`↑↓ scroll PgUp/PgDn page Esc close`)
  sits directly above the status/footer row (`Building Docker image... ... jk-run-533476`).
- The expected blank spacer after hints and before the status bar is missing.

Repro steps:

1. Launch PR #495 with `--debug`.
2. Open the Docker build log overlay during image build.
3. Inspect the bottom rows.

Expected behavior:

- Build log body, then the hint row, then a spacer, then the status/footer row.
- Spacing follows the same TUI chrome convention used elsewhere.

Actual behavior:

- Hint row is immediately adjacent to the status/footer row.

Relevant files:

- `crates/jackin-launch/src/tui/components/build_log_dialog.rs`
- `crates/jackin-launch/src/tui/components/footer.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/hint_bar.rs`

Starting anchors:

- `build_log_box_area`
- `render_build_log_dialog`
- `render_footer`
- `render_hint_bar`

Evidence:

- `build_log_box_area()` reserves only two rows below the box.
- `render_build_log_dialog()` places `hint_area` on the second-last row and
  `footer_area` on the last row.

Suspected root cause:

- Build log overlay manually composes the bottom rows instead of using a shared
  footer/hint stack with the standard spacer policy.

Blocks checklist:

- Blocks Defect 54 visual smoke polish.
- Related to the earlier footer spacing design rule.

Acceptance:

- Build log overlay bottom chrome matches the standard status/hint spacing.
- Add/adjust tests or snapshots that prove the spacer row exists.
- Verify on a real terminal after rebuild.

Close when:

- A focused render test/snapshot proves the hint row and footer/status row are
  separated according to the shared footer stack.
- Real smoke confirms the Docker build overlay no longer places hints directly
  against the status bar.

### 2. Capsule Pane Scrollbar Is Missing Until Scrollback Offset Changes

Status: open.

Observed behavior:

- A pane with content longer than the visible area sometimes shows no scrollbar.
- The scrollbar appears only after scrolling, and even then it is unstable.

Repro steps:

1. Start a real capsule session under `--debug`.
2. Open a pane whose content exceeds available height.
3. Observe whether the right-side scrollbar is present before manually scrolling.
4. Scroll up/down and observe flicker/reactivity.

Expected behavior:

- If pane content is scrollable, the scrollbar is visible and stable.
- Scrollbar visibility is based on overflow, not on whether the pane is currently
  scrolled away from the live tail.

Actual behavior:

- The pane scrollbar is gated on scrollback offset and only appears while
  actively scrolled back.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/session.rs`
- `crates/jackin-capsule/src/tui/render.rs`
- `crates/jackin-tui/src/scroll.rs`
- `crates/jackin-tui/src/components/scrollable_panel.rs`

Starting anchors:

- `apply_pane_scrollbar`
- `tail_vertical_thumb`
- `scrollback_filled`
- `scrollback_offset`

Evidence:

- `apply_pane_scrollbar()` uses tail-scroll rendering.
- Current logic gates on `filled > 0 && offset > 0`, so a scrollable pane at the
  live tail does not display a scrollbar.

Suspected root cause:

- Capsule pane scrollbars use a local tail-relative special case instead of the
  shared scrollability/overflow helpers.

Blocks checklist:

- Blocks Defect 54 resize/scrollback/zero-ghosting smoke.
- Reopens scrutiny around shared scroll helper adoption.

Acceptance:

- Scrollbar appears whenever pane content is scrollable.
- Scrollbar remains stable at live tail and while scrolled back.
- Scrollbar visibility, hit-testing, wheel/key scroll, and render math use one
  shared helper path or a documented shared adapter.
- Add regression coverage for scrollable-at-tail and scrolled-back states.

Close when:

- Tests cover a scrollable pane at live tail and a pane scrolled back.
- Live smoke confirms the scrollbar is visible before scrolling and remains
  stable after scrolling.
- The implementation uses shared scroll math or a clearly documented adapter over
  shared scroll math.

### 3. Capsule Scrollback Wheel Causes Full-Screen Flicker

Status: open.

Observed behavior:

- Wheel scrolling a capsule pane causes visible full-screen flicker.
- Flicker occurs even when the scroll offset is already saturated and does not
  change.

Repro steps:

1. Start a multi-pane capsule session under `--debug`.
2. Scroll a pane up/down with the mouse wheel.
3. Continue scrolling after the pane has reached max scrollback.
4. Watch for full-screen flicker.

Expected behavior:

- Wheel events that do not change scroll offset should not redraw.
- Wheel events that do change scroll offset should repaint only the needed region.
- Bottom chrome should not be redrawn through competing paths.

Actual behavior:

- Every wheel event returns a full redraw for `scrollback-movement`.
- Saturated no-op wheel events still full-redraw.

Relevant files:

- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/daemon/compositor.rs`
- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/render.rs`

Starting anchors:

- `wheel dispatch: jackin-scrollback`
- `scroll_by`
- `compose_full_redraw`
- `wheel_scrollback_redraw_reason`
- `compose_direct_dirty_pane_frame`

Evidence:

- Capsule log lines around the wheel repro show:
  - `wheel dispatch: jackin-scrollback ... before=9 filled=9`
  - `wheel dispatch: jackin-scrollback ... after=9`
  - `render: kind=full reason=scrollback-movement`
- Earlier resize/render logs show paired `bottom-chrome: site=ratatui` and
  `bottom-chrome: site=raw-full`.

Suspected root cause:

- Wheel dispatch does not compare before/after offset before requesting redraw.
- Scrollback movement forces `compose_full_redraw()` instead of using the partial
  direct-grid patch path when possible.
- Bottom chrome ownership is split between Ratatui and raw append paths.

Blocks checklist:

- Blocks Defect 54 resize/scrollback/zero-ghosting.
- Blocks live performance run-id confidence for bytes-on-wire and present-frame
  behavior.

Acceptance:

- No-op scroll events do not request redraw.
- Offset-changing scroll events avoid whole-screen flicker.
- Logs prove no saturated scrollback event creates a full redraw.
- Add focused tests for no-op wheel dispatch and scrollback redraw reason.
- Validate in a real `--debug` capsule run and record the run id.

Close when:

- A unit test proves saturated wheel scroll does not request a redraw.
- A focused integration/compositor test proves an offset-changing scroll does not
  redraw unrelated chrome unless required.
- A live `--debug` run id includes log evidence for saturated wheel events with no
  full redraw.

### 4. Debug Info Dialog Hides Capsule Status Bar

Status: open.

Observed behavior:

- Opening Debug info inside the capsule replaces the whole screen with a blank
  modal backdrop.
- The capsule status bar is not displayed.

Repro steps:

1. Start a capsule session under `--debug`.
2. Open Debug info from the capsule status/chrome.
3. Observe whether the status bar remains visible.

Expected behavior:

- If the screen had a status bar before opening Debug info, that status bar
  remains visible.
- Debug info is an overlay over the content area, not a full-screen replacement
  that covers status/footer chrome.

Actual behavior:

- The capsule modal path paints a backdrop over `frame.area()` and returns before
  rendering `StatusBarWidget`.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`
- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`

Starting anchors:

- `render_capsule_ratatui_frame`
- `StatusBarWidget`
- `DialogBackdrop`
- `DialogRatatuiSnapshot::DebugInfo`
- `render_container_info_on_blank`

Evidence:

- `render_capsule_ratatui_frame()` treats `view.dialog_open` as screen-owning:
  render backdrop over full frame, render dialog, return.
- `DialogRatatuiSnapshot::DebugInfo` calls `render_container_info_on_blank()`.
- `render_container_info_on_blank()` paints `ModalBackdrop` over the full area.

Suspected root cause:

- The capsule inherited a legacy screen-owning modal model that conflicts with
  the current footer/status design decision.

Blocks checklist:

- Blocks Defect 54 visual smoke.
- Violates `docs/content/docs/reference/tui/dialogs.mdx` status/footer rules.

Acceptance:

- Capsule Debug info preserves status bar/chrome when opened from a screen with
  status bar/chrome.
- Backdrop excludes reserved status/footer rows.
- Tests prove dialog-open capsule frames still render status bar rows.

Close when:

- A capsule render test proves Debug info and status bar are visible in the same
  frame.
- No capsule Debug info path paints `ModalBackdrop` over the full terminal area
  when the status bar is expected.
- Live smoke confirms status bar remains visible under Debug info.

### 5. Debug Info Has Multiple Visual Variants Across Surfaces

Status: open.

Observed behavior:

- Console Debug info, launch Debug info, and capsule Debug info do not look and
  behave like one component.
- Hints, footer behavior, backdrop behavior, horizontal placement, status bar
  preservation, and copy affordances differ.

Observed variants:

1. Host console Debug info: fewer rows, footer/status visible, run chip visible.
2. Capsule Debug info with Codex: container/capsule rows, no status bar, floating
   hint under dialog.
3. Capsule Debug info with Claude: same row family but different placement/hint
   behavior.
4. Launch Debug info during cockpit: blank/fullscreen backdrop differs again.

Expected behavior:

- One shared Debug info dialog across jackin'.
- The only difference between surfaces is which rows are available.

Actual behavior:

- Shared row model exists, but each surface chooses its own shell, backdrop,
  hints, footer, and input wiring.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-console/src/tui/components/container_info.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/components/modal_layout.rs`
- `crates/jackin-launch/src/tui/components/container_info_dialog.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`

Starting anchors:

- `DebugInfo::into_state`
- `debug_run_info_state`
- `launch_container_info_state`
- `Dialog::container_info_state`
- `render_container_info`
- `render_container_info_on_blank`

Evidence:

- `DebugInfo` already defines canonical row order and labels.
- Console renders `render_container_info()` into a modal area and relies on
  reserved footer hints.
- Launch and capsule use `render_container_info_on_blank()` and paint a full
  blank backdrop.
- Capsule local `DialogRatatuiSnapshot::DebugInfo` calls the blank renderer.

Suspected root cause:

- The shared data model was extracted, but the shared dialog shell and modal
  lifecycle contract were not.

Blocks checklist:

- Blocks Defect 54 Debug info and live smoke validation.

Acceptance:

- All Debug info surfaces call one shared top-level render/layout path or a
  shared path with explicit surface options.
- Row order, title, scrollbars, hover, copy, hints, and status/footer behavior
  are consistent.
- Visual regression tests or snapshots cover console, launch, and capsule
  variants.

Close when:

- Console, launch, and capsule tests assert the same canonical row order for the
  rows each surface can provide.
- Render tests or snapshots prove the same title, shell, scrollbar behavior,
  copy affordance, hover state, and footer/status policy.
- Remaining surface-specific code is limited to fact assembly and state storage.

### 6. Debug Info Run ID And Diagnostics Log Must Be Semantically Correct

Status: open.

Observed behavior:

- One console Debug info screenshot showed `Run ID` as a full diagnostics JSONL
  path while `Diagnostics log` showed the same path.

Expected behavior:

- `Run ID` is the bare run id, for example `jk-run-b93735`.
- `Diagnostics log` is the full diagnostics JSONL path.

Actual behavior:

- At least one observed screen displayed the diagnostics path in both rows.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-console/src/tui/components/container_info.rs`
- `crates/jackin/src/console/tui/run.rs`
- `crates/jackin-launch/src/tui/components/container_info_dialog.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`
- `crates/jackin-capsule/src/container_context.rs`

Starting anchors:

- `DebugInfo { run_id, diagnostics_log_path }`
- `debug_run_info_state`
- `run.run_id()`
- `run.path()`
- `diagnostics.run_id`
- `diagnostics.run_log_display`

Evidence:

- `DebugInfo` comments explicitly state `run_id` is bare and
  `diagnostics_log_path` is absolute path.
- Console builder currently accepts `run_id` and `log_path` separately; verify
  all call sites pass those values correctly in live/current code.

Suspected root cause:

- A surface or stale path may be passing `run.path()` where `run.run_id()` is
  expected, or an old binary/session may have been used during the screenshot.

Blocks checklist:

- Blocks Debug info trust and copy affordance acceptance.

Acceptance:

- Tests assert `Run ID` row is bare across console, launch, and capsule builders.
- Tests assert `Diagnostics log` row is path/hyperlink/copyable.
- Live smoke confirms row values are correct.

Close when:

- `rg -n 'Run ID.*jsonl|Run ID.*diagnostics'` finds no rendered/test output
  indicating a path in the Run ID row, except negative tests.
- Tests fail if a builder passes a diagnostics path as `run_id`.
- Live Debug info screenshots/logged state show bare Run ID plus full Diagnostics
  log path.

### 7. Dialogs Must Preserve Existing Status/Footer Bars

Status: open.

Observed behavior:

- Debug info hides the capsule status bar.
- Build log overlay owns the full launch screen and composes its own bottom rows.
- Some dialogs render floating hints instead of using reserved footer chrome.

Expected behavior:

- If a screen had a status/footer bar before a dialog opened, the dialog must
  preserve it.
- Dialog hints live in the reserved footer/hint area, not inside or under the
  dialog body.
- Full-screen overlays that intentionally hide the underlying surface must still
  respect the standard footer/hint/status layout.

Actual behavior:

- Capsule and launch blank-backdrop helpers cover full frame areas.
- Console follows the desired model more closely: modal content in modal rect,
  hints in reserved footer.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-launch/src/tui/view.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/view/frame.rs`

Starting anchors:

- `dialog_backdrop`
- `render_launch_frame`
- `render_capsule_ratatui_frame`
- `prepare_visible_modal`
- `modal_outer_rect`
- `footer_hint_spans`

Evidence:

- `docs/content/docs/reference/tui/dialogs.mdx` says status/hint rows are
  inviolable and modal backdrop must not cover footer.
- `docs/content/docs/reference/tui/navigation.mdx` says hints are footer-only,
  never internal dialog lines.

Suspected root cause:

- There is no shared modal-layer helper that receives content area and footer
  area separately for all surfaces.

Blocks checklist:

- Blocks Defect 54 visual smoke.

Acceptance:

- Shared dialog layer computes modal rects inside content area, excluding
  reserved footer/status rows.
- Capsule, launch, and console obey the same footer preservation rule.
- Tests prove the footer/status rows remain visible under each relevant dialog.

Close when:

- Every status-preserving modal path receives a content area that excludes
  reserved footer/status rows.
- Backdrop render tests prove reserved rows are not covered.
- Hints for modals are rendered by the reserved footer/hint layer, not as
  floating lines below dialog boxes.

### 8. Context7 API Key Prompt And Other Dialogs Must Use The Same Dialog System

Status: open.

Observed behavior:

- The Context7 API key prompt has its own visual shape and spacing.
- Capsule text-input dialogs and picker dialogs also have local renderer logic.

Repro steps:

1. Trigger MCP/Context7 API key setup prompt.
2. Compare it with Debug info, capsule rename prompt, capsule picker, and console
   modal shapes.

Expected behavior:

- All dialogs use the shared dialog system.
- Surface-specific code supplies content/state; shared code supplies shell,
  spacing, backdrop, footer/hints, scroll, and input affordances.

Actual behavior:

- Launch prompt uses shared `render_text_input()` but own backdrop/geometry.
- Capsule has a local `render_text_input_dialog()` in `dialog_widgets.rs`.
- Capsule filter/info dialogs build their own panels and hint behavior.

Relevant files:

- `crates/jackin-launch/src/tui/components/prompts.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/text_input.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`
- `crates/jackin-tui/src/components/panel.rs`

Starting anchors:

- `draw_text_prompt`
- `text_prompt_rect`
- `dialog_backdrop`
- `render_text_input`
- `render_text_input_dialog`
- `DialogRatatuiSnapshot::TextInputDialog`

Evidence:

- `draw_text_prompt()` calls launch-local `dialog_backdrop()` and local
  `text_prompt_rect()`.
- Capsule has a private `render_text_input_dialog()` instead of routing through
  the shared text-input dialog widget.

Suspected root cause:

- Shared low-level widgets exist, but dialog shell/layout is still fragmented.

Blocks checklist:

- Blocks the operator's reusability requirement for the TUI architecture epic.

Acceptance:

- Context7 prompt, launch text prompts, capsule text input, and console text
  input use a shared text-input dialog path or documented shared adapter.
- Remove or reduce local duplicate renderers where possible.
- Tests/snapshots show consistent shell/spacing/focus/hints.

Scope boundary:

- Required in this goal: fix the Context7/launch prompt path and capsule
  text-input/dialog paths that share the observed inconsistent shell behavior.
- Required audit: enumerate all dialog variants and record any remaining
  surface-local renderer with a structural reason.
- Not required: a broad unrelated rewrite of every picker if the shared
  footer/status/Debug info contract can be completed without it.

Close when:

- A repo-wide search shows no duplicate text-input dialog renderer unless it is
  documented as an adapter over shared primitives.
- The Context7 prompt and capsule text input share shell, spacing, and footer
  behavior with the shared dialog system.

### 9. Debug Info Must Have Full Shared Interaction Contract

Status: open.

Observed behavior:

- Horizontal scroll, hover color, copy behavior, and copy hints exist in pieces
  but are buggy and inconsistent.
- Copy affordances are not consistently visible for all copyable values.

Expected behavior:

- Debug info displays a horizontal scrollbar whenever any row exceeds available
  width.
- The scroll hint advertises horizontal scroll only when horizontal overflow
  exists.
- Copyable values include Container ID, Run ID, and Diagnostics log whenever
  present.
- Copyable values are hoverable; hovering over the value text changes its color
  slightly and reliably.
- The dialog visibly tells the operator how to copy each value.
- Click-to-copy works on copyable values.
- Keyboard copy works consistently according to the shared footer/hint contract.
- OSC 8 hyperlink overlay for diagnostics log follows the visible horizontally
  scrolled slice.

Copy affordance requirement:

- Do not satisfy this with invisible click targets alone. Render an explicit
  shared copy affordance for copyable rows. The footer must also explain the copy
  action available on the current surface.

Actual behavior:

- Existing implementation has shared pieces, but each surface wires mouse move,
  copy, scroll, and hint state separately.
- Capsule hover redraw currently composes a full dialog overlay redraw.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-launch/src/tui/subscriptions.rs`
- `crates/jackin/src/console/tui/input/mouse.rs`
- `crates/jackin/src/console/effects.rs`
- `crates/jackin-capsule/src/daemon/mouse_input.rs`
- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`

Starting anchors:

- `ContainerInfoState`
- `ContainerInfoRow::copyable`
- `mark_copied`
- `set_hovered_row`
- `copy_payload_at`
- `value_placements`
- `hyperlink_overlay`
- `debug_info_hint_spans`

Evidence:

- `ContainerInfoState` already stores `DialogBodyScroll`, copied row, and
  hovered row.
- `copy_payload_at()` and `value_placements()` already account for scroll axes.
- `hyperlink_overlay()` accounts for the visible horizontally scrolled slice.
- Console, launch, and capsule each have separate hover/copy routing.

Suspected root cause:

- Interaction behavior is not centralized with the renderer contract.
- Footer hint vocabulary does not yet express per-row copy affordances clearly
  enough for all surfaces.

Blocks checklist:

- Blocks Debug info smoke acceptance.
- Blocks operator confidence in diagnostics collection.

Acceptance:

- Shared tests cover horizontal overflow, horizontal scrollbar visibility,
  horizontal hint gating, hover hit-test after horizontal/vertical scroll, copy
  hit-test after scroll, copied-row feedback, and diagnostics hyperlink overlay.
- Console, launch, and capsule tests prove they all preserve and update
  hover/copy/scroll state correctly.
- Live smoke confirms all copyable values can be copied and hovered.

Close when:

- The explicit copy affordance is visible for Container ID, Run ID, and
  Diagnostics log whenever those rows exist.
- Hover state changes the visible value/affordance color reliably and only on
  copyable cells.
- Mouse click and keyboard copy paths copy the same payload for the same row.
- Tests cover copy/hover after horizontal scroll and after vertical scroll.
- Live smoke confirms copy works for Container ID, Run ID, and Diagnostics log.

### 10. Scrollable Pane Text Selection Must Persist, Copy, And Auto-scroll

Status: open.

Observed behavior:

- In a capsule pane, text selection is copyable and copies to the clipboard on
  mouse-up.
- The visible selection disappears immediately after the operator finishes
  selecting.
- There is no visible confirmation that the selected text was copied.
- Drag-selecting beyond the top or bottom of the visible scrollable pane does not
  auto-scroll the pane to extend the selection.

Repro steps:

1. Start a capsule session with enough pane content to overflow vertically.
2. Drag-select text inside the pane.
3. Release the mouse button.
4. Observe whether the selection remains highlighted and whether copied feedback
   appears.
5. Start selecting near the bottom and drag below the visible pane edge.
6. Start selecting near the top and drag above the visible pane edge.

Expected behavior:

- The pane selection model is read-only: select + copy only, with no edit/delete
  semantics.
- Selection remains highlighted after mouse-up.
- Mouse-up copies selection to clipboard and shows a visible `Selection copied`
  style confirmation in shared chrome/status.
- Clicking unrelated content/chrome or starting to type clears the persisted
  selection.
- Starting a new selection replaces the previous selection.
- Dragging beyond the top/bottom edge auto-scrolls the scrollable pane and
  extends the selection in the drag direction.
- Auto-scroll stops at content bounds and does not flicker or full-redraw the
  whole screen unnecessarily.

Actual behavior:

- Selection highlight only exists during active drag selection.
- The selected area disappears after mouse-up.
- Copy success is silent.
- Selection does not auto-scroll beyond the visible area.

Relevant files:

- `crates/jackin-capsule/src/daemon/mouse_input.rs`
- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/daemon/compositor.rs`
- `crates/jackin-capsule/src/session.rs`
- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/render.rs`
- `crates/jackin-tui/src/scroll.rs`

Starting anchors:

- `Selection`
- `select`
- `clipboard`
- `copy`
- `mouse drag`
- `MouseEventKind::Drag`
- `MouseEventKind::Up`
- `scrollback_offset`
- `scroll_by`

Evidence:

- Operator observed correct clipboard copying but no persistent selection or
  visible copied feedback.
- Operator observed that drag selection does not auto-scroll when selecting past
  the visible viewport.

Suspected root cause:

- The current selection path treats selection as an active drag overlay only, not
  as a persistent content-coordinate range.
- Copy feedback is not connected to the standard footer/status chrome.
- Drag selection does not own an edge-auto-scroll ticker/path.

Blocks checklist:

- Blocks Defect 54 live smoke polish.
- Blocks expected behavior for scrollable pane selection.

Acceptance:

- Selection supports copy only; it never edits, deletes, cuts, replaces, or
  pastes pane content.
- Selection range is stored in content coordinates with anchor/focus semantics.
- Mouse-up copies selected text and leaves selection visibly highlighted.
- A visible copied confirmation appears in standard chrome/status.
- Clicking unrelated content/chrome clears the selection.
- Typing clears the selection before forwarding the key to the pane.
- Starting a new selection replaces the old selection.
- Wheel scrolling after selection keeps the same content range selected when it
  remains visible.
- Dragging beyond top/bottom viewport edges auto-scrolls and extends selection.
- Auto-scroll is bounded, stable, and does not send drag-scroll input to the PTY.

Close when:

- Tests cover mouse-up copy with persisted selection.
- Tests cover clear-on-click, clear-on-type, and replace-on-new-selection.
- Tests cover selection rendering after scroll offset changes.
- Tests cover upward and downward auto-scroll while drag-selecting beyond the
  pane viewport.
- Live smoke confirms persistent highlight, copied feedback, deselect behavior,
  and edge auto-scroll in a real scrollable capsule pane.

## Ordered Fix Plan

Do not start implementation until the operator says issue collection is complete
or explicitly asks to fix the current set.

### Phase 1 - Lock The Shared Debug Info Contract

- Define the shared Debug info contract in `jackin-tui`:
  - canonical rows and labels;
  - copyable rows;
  - hover state;
  - copied-row state;
  - both-axis scroll state;
  - scroll axes/hints derived from actual overflow;
  - explicit copy affordance rendering;
  - status/footer preservation expectations.
- Add focused unit/snapshot tests in `jackin-tui` for:
  - bare Run ID vs Diagnostics log path;
  - Container ID, Run ID, Diagnostics log copy targets;
  - horizontal overflow and scrollbar;
  - horizontal hint appears only on horizontal overflow;
  - hover color changes only on copyable value cells;
  - copy hit-test follows both scroll axes;
  - copy affordance placement follows both scroll axes;
  - hyperlink overlay follows both scroll axes.

### Phase 2 - Replace Per-Surface Debug Info Shells

- Route console, launch, and capsule Debug info through the same shared
  render/layout contract.
- Remove or deprecate `render_container_info_on_blank()` if it cannot preserve
  status/footer rows. If a blank backdrop is still needed, make it accept a
  content-only area and never cover reserved chrome.
- Make launch and capsule compute modal rects against the content area, not the
  full terminal.
- Keep only surface-specific data assembly and state storage outside
  `jackin-tui`.

### Phase 3 - Footer And Hint Unification

- Move Debug info hints into the reserved footer/status area on all surfaces.
- Ensure hints use `HintSpan` and the shared hint renderer.
- Add copy hints that clearly identify copyable values.
- Render a shared explicit copy affordance for each copyable Debug info row.
- Ensure scroll hints are generated by `scroll_hint_spans()` from actual
  overflow axes.
- Fix build-log overlay spacing through the same footer/hint layout primitive.

### Phase 4 - Dialog System Reuse Beyond Debug Info

- Audit launch prompts, Context7 API key prompt, capsule text input, capsule
  pickers, console modals, and info dialogs.
- Replace duplicate local shell/backdrop/text-input renderers with shared
  `jackin-tui` helpers where practical.
- If a renderer must remain surface-local, document the structural reason and
  keep row spacing, footer/hints, and status preservation identical.

### Phase 5 - Capsule Scrollbar And Flicker Fix

- Change pane scrollbar visibility from `offset > 0` to real scrollability.
- Route pane scrollbar math through shared scroll helpers or a documented shared
  adapter.
- In wheel dispatch, compare scrollback offset before/after and skip redraw when
  unchanged.
- Prefer partial/redraw-minimal frame composition for scrollback movement where
  technically possible.
- Eliminate or explain the dual bottom-chrome draw path (`ratatui` plus
  `raw-full`) so chrome does not flicker.

### Phase 6 - Scrollable Pane Selection

- Introduce or extend a pane selection model that stores anchor/focus in content
  coordinates.
- Keep selection visible after mouse-up and copy.
- Add a copied feedback path through standard capsule chrome/status.
- Clear selection on explicit deselect, typing, pane close/clear, or new
  selection.
- Add edge auto-scroll during active drag selection.
- Ensure selection auto-scroll and scrollback wheel use the same scroll bounds.

### Phase 7 - Docs

- Update `docs/content/docs/reference/tui/dialogs.mdx` with the Debug info
  component contract after code and tests pass. Include row order, copyable rows,
  explicit copy affordance, hover behavior, horizontal scroll, and footer/status
  preservation.
- Update `docs/content/docs/reference/tui/components.mdx` if the reusable
  component catalog changes. Add the shared Debug info/dialog shell if it becomes
  a named component.
- Update `docs/content/docs/reference/tui/navigation.mdx` only if hint/copy/hover
  rules need more explicit wording. Keep scroll-hint rules there.
- Add the scrollable pane text-selection contract to the TUI docs. The best home
  is `docs/content/docs/reference/tui/navigation.mdx` because this is input,
  selection, scroll, and visible feedback behavior.
- Do not mark roadmap/checklist items complete until the required command output
  or run ids exist.

### Phase 8 - Verification

Run focused tests first, then broad gates:

```sh
cargo fmt --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked -E 'not binary(dind_e2e)'
```

Run docs gates if TUI docs are edited:

```sh
cd docs
bun run build
bun run check:repo-links
bunx tsc --noEmit
bun test
```

Run a real operator smoke after rebuilding:

- Capsule smoke build and real multi-pane `--debug` session.
- Debug info on console, launch, and capsule.
- Horizontal scroll in Debug info with a long diagnostics path.
- Hover and copy Container ID, Run ID, and Diagnostics log.
- Build log overlay footer spacing.
- Pane scrollbar visible at live tail and while scrolled back.
- Wheel scroll no-op does not flicker or full redraw.
- Pane text selection remains visible after copy.
- Pane copied feedback appears in standard chrome/status.
- Clicking elsewhere and typing clear the persisted selection.
- Drag-selecting past the top/bottom edge auto-scrolls and extends selection.

Capture run ids and relevant log lines before marking any Defect 54 checklist
item complete.

## External References Used For Selection Rules

- W3C Selection API: <https://www.w3.org/TR/selection-api/>
- W3C Selection API user interactions: <https://www.w3.org/TR/selection-api/#user-interactions>
- MDN Selection API: <https://developer.mozilla.org/en-US/docs/Web/API/Selection_API>
- MDN Selection object: <https://developer.mozilla.org/en-US/docs/Web/API/Selection>
- W3C Pointer Events `mousedown`: <https://www.w3.org/TR/pointerevents/#the-mousedown-event>
- W3C WAI G149 highlighting technique: <https://w3c.github.io/wcag/techniques/general/G149>

## Do Not Mark Done Yet

These stay open until real evidence exists:

- Defect 54 live smoke ledger.
- Defect 58 manual resize/ghosting smoke.
- Defect 59 B.5 source-folder end-to-end smoke.
- Defect 60 final roadmap sweep.
- Defect 63 deferred license ruling.
- DCO/back-history lane unless Laris/operator says it is complete.
