# PR #495 TUI Smoke Findings

This file is the handoff checklist for the remaining TUI issues found during the
operator's manual `--debug` smoke of PR #495 on branch `feature/tui-architecture`.

Use this as the target for a follow-up `/goal` command:

```text
/goal Follow PR-495-TUI-SMOKE-FINDINGS.md and fix all findings.
```

## Executive Summary

Fix the root causes, not ten isolated screenshots. As of the 2026-06-08
operator re-smoke, the F1-F10 TUI convergence work is closed with both focused
tests and live evidence from `jk-run-aa0e87`. The code has converged on shared
primitives for the main classes: Debug info row/copy/scroll semantics,
status-preserving dialog placement, shared build-log bottom chrome, capsule
scrollback redraw tiering, and content-coordinate pane selection.

The F1-F10 findings collapsed into four work groups, all now closed by
`jk-run-aa0e87` plus the focused tests listed below:

1. **Shared Debug info/dialog contract.** The shared row model, copy
   affordances, hover, both-axis scroll, and status-preserving placement have
   focused tests, and the live smoke covered console, launch, and capsule
   routing/visual parity.
2. **Modal/footer layering.** Capsule and launch no longer use the stale
   full-frame Debug info backdrop path, build-log overlays use the shared
   bottom-chrome stack, and the live smoke verified status/footer chrome remains
   visible.
3. **Capsule scrollback/scrollbar rendering.** Pane scrollbars gate on retained
   scrollback (`filled > 0`), use shared `tail_vertical_thumb` math, suppress
   saturated wheel redraws, and route offset-changing scrollback through the
   non-clear frame path. The live multiplexer log proves the operator-observed
   flicker class is gone for this run.
4. **Scrollable pane text selection.** Focused tests cover retained highlight,
   content-coordinate copy, top-right toast feedback, clear triggers, and
   bidirectional edge auto-scroll; the live smoke verified the behavior end to
   end.

The correct outcome remains a smaller number of shared primitives with stronger
tests, not more per-surface special cases.

## Execution Checklist

This checklist manages the whole goal. Work the items strictly one by one, in
the order below (it follows the Ordered Fix Plan phase order): implement the
fix, run its focused tests, capture the live evidence the finding requires,
and only then tick the box and move to the next item. Do not batch checkmarks
and do not start a later item while an earlier one is unverified.

Every checkmark here must propagate immediately — in the same commit — to the
matching Defect 64 box in
`docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`,
with the evidence note (command output and/or `--debug` run id) pasted there
per the Defect 54 pattern. When a tick changes the program status story
(first item done, last item done, a phase fully closed), update the Round 3 /
Defect 64 wording in
`docs/content/docs/reference/roadmap/post-restructure-fixes.mdx` in the same
commit too. A box checked here but not propagated is an incomplete item.

- [x] F6 — Run ID / Diagnostics log semantics correct across all builders
  (Phase 1; shared contract tests in `jackin-tui`).
- [x] F9 — Debug info shared interaction contract: copy affordances, hover,
  both-axis hit-tests, hyperlink overlay (Phases 1, 3).
- [x] F4 — capsule Debug info preserves status bar and reserved chrome
  (Phase 2).
- [x] F5 — one shared Debug info shell across console, launch, and capsule
  (Phase 2).
- [x] F7 — every status-preserving dialog computes rects against the content
  area; reserved rows never covered (Phases 2, 3).
- [x] F1 — build-log overlay bottom chrome on the shared hint/spacer/status
  stack (Phase 3).
- [x] F8 — text-input prompts and remaining dialog families on the shared
  dialog system, or documented exceptions (Phase 4).
- [x] F2 — pane scrollbar gates on real scrollability, shared scroll math
  (Phase 5).
- [x] F3 — no-op wheel events skip redraw; scrollback movement leaves the
  clear-tier (Phase 5).
- [x] F10 — persistent content-coordinate pane selection with copied feedback
  and edge auto-scroll (Phase 6).
- [x] TUI docs updated with the proven contracts (Phase 7). Evidence:
  `docs/content/docs/reference/tui/chrome.mdx`,
  `docs/content/docs/reference/tui/dialogs.mdx`, and
  `docs/content/docs/reference/tui/navigation.mdx`, with
  `docs/content/docs/reference/capsule/index.mdx` aligned to the capsule
  exception, document the shared Container/Debug info dialog contract,
  status-preserving overlays,
  copy-success toast feedback outside the hint/footer rows, and read-only
  content-coordinate pane selection. `bun run build`,
  `bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test` from
  `docs/` all exit 0.
- [x] Convergence metrics from the refactor map hold on fresh sweeps
  (app-wide definition of done).
- [x] Final re-smoke: one `--debug` session exercising all ten findings; run
  id and key log excerpts recorded here and in Defect 64.

**Final smoke evidence (2026-06-08):** operator verified every F1-F10 visual
check in live run `jk-run-aa0e87` and reported "everything looks good." The run
used the current capsule binary and current branch head:
`/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/diagnostics/runs/jk-run-aa0e87.jsonl:68`
records `0.6.0-dev_2187510`, and
`/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/jk-zr6f77yy-thearchitect/state/multiplexer.log:7`
records `feature/tui-architecture` at
`2187510b37edb582eb2a3c7745929220d63a3c3a`. The JSONL
`container_started` event at line 3689 points to the capsule log. Key log
anchors: bottom chrome `raw-full`/`dialog` paths at capsule log lines 12, 23,
261, and 270; no bad full-redraw hits for
`render: kind=full reason=(scrollback-movement|status-change|selection-repaint|dialog-change)|\\x1b\\[2J`;
direct-grid patch render metrics with `t_parse_us`, `changed_rows`, and
`changed_cells` at lines 28-30, 45, 81, 139, 168; saturated scrollback wheel
no-ops at lines 13496-13646. Focused tests and sweeps listed below remain the
code-level regression proof.

- `cargo test -p jackin-tui container_info --locked` — 12 passed after adding
  Run-ID-first row-order coverage, shared default keyboard-copy payload,
  Enter-does-not-dismiss coverage,
  both-axis copy hit-test and hyperlink overlay coverage, and after retiring the
  stale full-background `render_container_info_on_blank()` helper.
- `rg -n "render_container_info_on_blank|blank_render_clears_full_background" crates docs` — no hits.
- `cargo test -p jackin-console container_info --locked` — 1 passed; proves the
  console Debug info state keeps Run ID bare and Diagnostics log copyable +
  hyperlinked.
- `cargo test -p jackin-console footer_hints --locked` — 19 passed; proves the
  console Debug info footer advertises `↵ copy value`, `Esc dismiss`, and
  `click copy value` through the shared footer-hint surface.
- `cargo test -p jackin container_info_enter_copies_default_value_without_dismissing --locked`
  — exits 0; proves the console key-handler path copies the shared default
  Debug info value on Enter and keeps the dialog open.
- `cargo test -p jackin-launch container_info --locked` — 4 passed; proves the
  launch Debug info state keeps Run ID bare, hides run rows outside debug mode,
  and preserves the status footer in the focused render test.
- `cargo test -p jackin-capsule container_info --locked` — 20 passed; proves the
  capsule Debug info state keeps Run ID bare, supports shared default keyboard
  copy, copy feedback, horizontal wheel scroll, and unsupported-axis no-op
  behavior.
- `cargo test -p jackin-capsule debug_dialog_keeps_status_bar_visible --locked`
  — 1 passed; proves Debug info and both capsule status-bar rows render in the
  same frame.
- `cargo test -p jackin-capsule view --locked` — 9 passed; includes the Debug
  info/status-bar test plus dialog bottom-chrome tests.
- `cargo test -p jackin-launch launch_debug_info_keeps_status_footer_visible --locked`
  — 1 passed; proves launch Debug info preserves the status footer.
- `cargo clippy -p jackin-capsule --all-targets --all-features --locked -- -D warnings`
  — exits 0 after strengthening the status-preservation render test.
- `rg -n "render_container_info_on_blank|blank_render_clears_full_background|frame\\.render_widget\\(DialogBackdrop, frame\\.area\\(\\)\\)" crates/jackin-capsule/src crates/jackin-tui/src/components crates/jackin-launch/src/tui crates/jackin/src/console/tui -g '*.rs' -g '!**/tests.rs'`
  — no production hits; the stale full-background Debug info helper and
  production full-frame capsule dialog backdrop are absent.
- `rg -n 'Run ID.*jsonl|Run ID.*diagnostics|render_container_info_on_blank|blank_render_clears_full_background' crates`
  — only negative-test assertion messages remain; no production renderer or
  state builder puts a diagnostics path in a `Run ID` row and the stale blank
  renderer is absent.
- `cargo test -p jackin-tui bottom_chrome --locked` — 2 passed; proves the
  shared bottom-chrome helper reserves body, hint, spacer, and footer rows and
  collapses only rows that do not fit.
- `cargo test -p jackin-launch build_log --locked` — 11 passed; includes the
  Docker build-log overlay spacer/footer render test plus the existing
  scroll/wheel/drag/tail-offset coverage.
- `cargo test -p jackin-launch --locked` — 34 passed after routing launch
  pre-cockpit prompts and the launch failure popup through the shared
  `bottom_chrome_areas()` body/hint rows. New focused coverage proves the
  Context7-style text prompt renders hints in the shared hint row with separate
  spacer/footer rows, and the failure popup keeps the status footer visible
  while its dismiss/copy hints live above the spacer.
- `cargo clippy -p jackin-tui --all-targets --all-features --locked -- -D warnings`
  — exits 0.
- `cargo clippy -p jackin-launch --all-targets --all-features --locked -- -D warnings`
  — exits 0.
- `cargo fmt --check` — exits 0 after adding the shared bottom-chrome helper.
- `rg -n "BUILD_LOG_BOTTOM_ROWS|BUILD_LOG_HINT_ROW_FROM_BOTTOM|BUILD_LOG_FOOTER_ROW_FROM_BOTTOM|area\\.height\\.saturating_sub\\(2\\)|area\\.height\\.saturating_sub\\(3\\)" crates/jackin-launch/src/tui/components/build_log_dialog.rs crates/jackin-tui/src/components/bottom_chrome.rs`
  — no hits, proving the build-log overlay no longer owns local bottom-row
  constants or stale two-row height math.
- `cargo test -p jackin-capsule retained_scrollback_draws_scrollbar_at_live_tail --locked`
  — 1 passed; proves retained scrollback paints a pane scrollbar at live tail.
- `cargo test -p jackin-capsule apply_action_wheel_noops_at_scrollback_boundary --locked`
  — 1 passed; proves saturated scrollback wheel events do not request redraw.
- `cargo test -p jackin-capsule apply_action_wheel_scrolls_scrollback --locked`
  — 1 passed; proves offset-changing wheel scroll moves the scrollback offset.
- `rg -n "compose_full_redraw\\([^\\n]*(wheel_scrollback|ScrollbackMovement)|wheel_scrollback_redraw_reason\\(" crates/jackin-capsule/src/daemon crates/jackin-capsule/src/tui crates/jackin-capsule/tests`
  — only the redraw-vocabulary helper and test remain; daemon dispatch no longer
  composes full redraws for wheel scrollback.
- `cargo test -p jackin-launch container_info --locked` — 4 passed.
- `cargo test -p jackin-launch build_log --locked` — 11 passed.
- `cargo test -p jackin-capsule container_info --locked` — 20 passed.
- `cargo test -p jackin-capsule debug_dialog_keeps_status_bar_visible --locked` — 1 passed.
- `cargo test -p jackin-capsule apply_action_wheel --locked` — 2 passed.
- `cargo test -p jackin-capsule scrollbar --locked` — 5 passed.
- `cargo test -p jackin-capsule selection --locked` — 21 passed after moving
  pane selection rows from screen coordinates to retained-content coordinates,
  projecting highlights into the current viewport, and copying from the full
  scrollback+live content snapshot.
- `cargo test -p jackin-capsule selection --locked` — 21 passed after routing
  selection repaint through the no-clear diff frame path, with assertions that
  start, motion, edge auto-scroll, finalize, click-clear, and type-clear
  selection frames do not emit `ESC[2J`.
- `cargo test -p jackin-capsule
  selection_copy_toast_keeps_status_and_bottom_chrome_rows_free --locked` — 1
  passed after constraining the `Selection copied` toast to the pane/content
  overlay area, proving the copied feedback remains visible without occupying
  the status rows or hint/spacer/footer rows.
- `cargo test -p jackin-capsule selection --locked` — 25 passed after adding
  the downward edge-auto-scroll regression to the pane-selection suite; the same
  focused suite also covers the status/bottom-chrome toast placement regression.
- `cargo test -p jackin-capsule selection --locked` — 25 passed after delaying
  pane selection activation until real drag motion. The suite now covers a plain
  pane click arming but not selecting
  (`apply_action_pane_primary_press_only_arms_selection_for_shell`), drag motion
  promoting that pending anchor
  (`pane_button_motion_promotes_pending_selection`), and release without drag
  clearing the pending anchor without repainting or copying
  (`mouse_release_without_drag_clears_pending_selection`).
- `cargo test -p jackin-tui labeled_text_input_dialog --locked` — 1 passed.
- `cargo test -p jackin-tui text_input_prompt_rect --locked` — 1 passed.
- `cargo test -p jackin-tui text_input --locked` — 2 passed after deleting
  the unused raw-ANSI `render_text_input_dialog()` duplicate and leaving the
  shared `TextInputState`/`render_text_input` and
  `render_labeled_text_input_dialog` paths as the only text-input dialog
  renderers.
- `cargo test -p jackin-launch text_prompt --locked` — 2 passed after the
  duplicate renderer removal, proving launch text prompt commit/skip behavior
  still runs through the shared text-input state path.
- `rg -n "render_text_input_dialog|TextInputDialogRect|fn .*text.*input.*dialog|pub fn render_.*text_input" crates -g '*.rs'`
  — no production source hits for the removed duplicate renderer; only the
  shared `render_text_input` and `render_labeled_text_input_dialog` functions
  remain.
- Dialog inventory audit (2026-06-08) — source dispatch now accounts for every
  dialog family without a second text-input or Debug-info shell:
  - Console `Modal` variants (`TextInput`, `Confirm`, `SaveDiscardCancel`,
    `ErrorPopup`, `ContainerInfo`, `StatusPopup`) route to shared
    `jackin-tui` renderers in `crates/jackin/src/console/tui/components/modal.rs`.
    Picker/form/File Browser variants remain surface adapters over shared
    `render_dialog_shell`, `SelectList`/`ScrollableList`, or form-specific
    state because their content and input routing are console-owned.
  - Settings/global-mount modal families in
    `crates/jackin/src/console/tui/components/settings.rs` use the same shared
    text-input, confirm, picker, File Browser, and auth-form render paths as
    root console modals.
  - Launch build log, failure popup, pre-cockpit prompt, and Debug info are the
    launch-specific families. Build log and failure popup now reserve rows via
    `bottom_chrome_areas()`, prompts call the shared text/select/confirm/error
    renderers inside `dialog_backdrop()`, and Debug info renders through
    `render_container_info()`.
  - Capsule `DialogRatatuiSnapshot::TextInputDialog` renders through
    `render_labeled_text_input_dialog()`, `DialogRatatuiSnapshot::DebugInfo`
    renders through `render_container_info()`, and the remaining capsule
    command/picker/info-row snapshots are terminal-specific adapters that still
    use shared dialog primitives where they overlap.
- `cargo test -p jackin-console list_geometry --locked` — 4 passed after
  routing list-name horizontal scroll clamping through
  `jackin_tui::components::scrollable_panel::clamp_scroll_offset`.
- `rg -n "scroll_[xy]\\s*=|\\.scroll_[xy]\\s*=|scrollback_offset\\s*=|\\.scrollback_offset\\s*=" crates/jackin/src crates/jackin-console/src crates/jackin-launch/src crates/jackin-capsule/src -g '*.rs' | rg -v "tests|test_|jackin-tui|session.rs|state/manager.rs|layout/list.rs|message.rs|update.rs|let mut scroll_[xy]|scroll_x = 0u16|scroll_y = u16::try_from|scrollback_offset = session.scrollback_offset|scrollback_offset ==|scrollback_offset,|scrollback_offset\\)"`
  — no hits after the remaining console list-name clamp moved to the shared
  scroll helper.
- `cargo test -p jackin-capsule rename_tab --locked` — 5 passed.
- `cargo test -p jackin-tui toast --locked` — 2 passed after making the shared
  toast anchor at the top-right and proving `Selection copied` renders outside
  the hint/footer rows.
- `cargo test -p jackin-tui dialog_layout --locked` — 11 passed.
- `cargo test -p jackin-capsule container_info --locked` — 20 passed after
  routing capsule raw key/wheel dialog scrolling through `DialogBodyScroll`.
- `cargo test -p jackin-capsule wheel --locked` — 15 passed after routing
  capsule raw key/wheel dialog scrolling through `DialogBodyScroll`.
- `cargo test -p jackin-capsule typed_input_snaps_scrollback_to_live_without_screen_erase --locked` — 1 passed after routing typed-input scrollback snap through the
  no-clear diff frame path, with an assertion that the snap repaint does not
  emit `ESC[2J`.
- `cargo test -p jackin-capsule hover --locked` — 5 passed after routing
  Debug-info copy-target hover through `compose_dialog_overlay_frame()` and
  adding a regression that asserts the hover repaint does not emit `ESC[2J`.
- `cargo test -p jackin-capsule apply_action_mouse_chrome_update_sets_pointer_shape --locked` — 1 passed after routing chrome hover/status repaint through the
  no-clear diff frame path, with an assertion that mouse chrome hover does not
  emit `ESC[2J`.
- `cargo test -p jackin-capsule focus --locked` — 16 passed after routing
  keyboard and mouse focus-change repaint through the no-clear diff frame path,
  with assertions that both focus paths do not emit `ESC[2J`.
- `cargo test -p jackin-capsule
  pending_status_change_uses_no_clear_diff_frame --locked` — 1 passed after
  routing queued status-only invalidations through a pending diff redraw instead
  of `pending_full_redraw`, proving the frame does not emit `ESC[2J`.
- `cargo test -p jackin-capsule
  pending_full_redraw_takes_precedence_over_status_diff --locked` — 1 passed,
  proving geometry/layout full redraws still override queued status diffs and
  keep the clear-tier path where it is required.
- `cargo test -p jackin-tui render_selected_lines_in_area --locked` — 4
  passed after routing filtered-picker rows through `ScrollableList`; selected
  backgrounds now fill the content width and stop before the scrollbar gutter.
- `cargo test -p jackin-console picker --locked` — 105 passed, covering the
  console picker families that consume the shared selected-line renderer
  (`op_picker`, GitHub picker, role picker, provider/source/scope picker tests).
- `cargo test -p jackin-console file_browser --locked` — 52 passed after
  moving the File Browser listing onto `ScrollableList::render_with_block`;
  focused render tests now prove the selected-row cursor, full-content-width
  highlight, selection-follow viewport, border-owned scrollbar behavior, and
  no-wrap clamped wheel selection movement.
- `cargo test -p jackin-tui scrollable_panel --locked` — 33 passed after
  adding the reusable blocked-list renderer that draws selectable rows inside a
  framed panel while replacing the right border with the scrollbar when needed.
- `cargo test -p jackin file_browser_wheel --locked` — 4 passed after adding
  shared File Browser modal wheel capture for editor, create-prelude, and
  settings mounts owners, including a saturated-at-edge case that proves wheel
  input does not leak through to background scrolling.
- `cargo test -p jackin
  mouse_drag_tests::settings_auth_source_folder_wheel_scrolls_modal_selection --locked`
  — 1 passed, proving the auth source-folder File Browser uses the same modal
  wheel capture path.
- `rg -n "request_full_redraw\\(status_change_redraw_reason|compose_full_redraw\\(status_change_redraw_reason" crates/jackin-capsule/src -g '*.rs'`
  — no source hits after moving status-only refreshes out of the clear tier.
- `cargo test -p jackin-capsule clear_pane --locked` — 2 passed after routing
  direct and palette clear-pane repaint through the no-clear diff frame path,
  with assertions that both paths still send `Ctrl+L` to the focused PTY and do
  not emit `ESC[2J`.
- `cargo test -p jackin-capsule direct_actions_map_to_visible_frame_plans
  --locked` — 1 passed after changing `Action::ClearFocusedPane` from
  clear-tier to diff-tier in the action frame plan.
- `cargo test -p jackin-capsule
  palette_route_redraw_reason_only_repaints_terminal_actions --locked` — 1
  passed after preserving the palette route reason contract for `PaneClear`.
- `cargo test -p jackin-capsule apply_action_open --locked` — 5 passed after
  routing direct command-palette, rename-dialog, and agent-picker opening or
  closing through the no-clear overlay frame path, with assertions that these
  overlay transitions do not emit `ESC[2J`.
- `cargo test -p jackin-capsule
  dialog_action_frame_plan_keeps_copy_feedback_overlay_scoped --locked` — 1
  passed after splitting pure dialog repaint/back-navigation/drill-down actions
  onto the no-clear overlay path while keeping command/spawn/confirmed terminal
  actions on the full path.
- `cargo test -p jackin-capsule
  dialog_action_frame_plan_keeps_copy_feedback_overlay_scoped --locked` — 1
  passed after reclassifying terminal dialog actions away from
  `Full(DialogChange)`: command actions are overlay-tier, spawn-as-tab is
  `TabSwitch`, spawn-as-split is `LayoutChange`, close confirmations are
  `SplitClose`, and exit confirmation is `SessionExit`.
- `cargo test -p jackin-capsule apply_action_dialog --locked` and
  `cargo test -p jackin-capsule apply_action_dismiss_closes_top_dialog --locked`
  — 3 focused tests passed after adding runtime assertions that consumed dialog
  input and dialog dismiss/back-navigation do not emit `ESC[2J`.
- `cargo test -p jackin-capsule palette_routes_map_to_visible_frame_plans
  --locked` — 1 passed after replacing the old palette-route redraw-reason
  helper with a shared frame-plan helper: sub-dialog routes use no-clear overlay
  frames, clear-pane uses a no-clear diff frame, and tab/zoom routes stay on
  geometry-tier full frames.
- `cargo test -p jackin-capsule palette_close --locked` and
  `cargo test -p jackin-capsule
  apply_action_palette_new_tab_pushes_agent_picker --locked` — 5 focused tests
  passed after routing command-palette New tab and Close sub-dialog transitions
  through the shared frame planner with assertions that the overlay transitions
  do not emit `ESC[2J`.
- `cargo test -p jackin-capsule
  apply_dialog_spawn_agent_provider_picker_uses_overlay_frame_without_screen_erase
  --locked` — 1 passed after making the multi-provider `SpawnAgent` branch
  return a no-clear overlay frame when it opens `ProviderPicker`, while keeping
  the zero/one-provider spawn path on the existing terminal-action route.
- `cargo test -p jackin-capsule prefix_ --locked` — 16 passed after adding
  prefix-dispatch regressions proving New tab and Palette use no-clear overlay
  frames, Move focus and Clear pane use no-clear diff frames, and only explicit
  Prefix Redraw remains in the clear-tier.
- `cargo test -p jackin-capsule
  frame_plans_keep_diff_tier_reasons_out_of_full_redraws --locked` — 1 passed
  after adding a planner invariant that rejects any `Full(...)` frame plan whose
  reason is not in the clear-tier set (`FirstAttach`, `Resize`, `TabSwitch`,
  `LayoutChange`, `SplitClose`, `ZoomChange`, `SessionExit`, `ExplicitRedraw`).
- `cargo test -p jackin-capsule selection --locked` — 25 passed in the
  2026-06-08 fresh audit, confirming the focused content-coordinate selection,
  retained highlight, copy toast, clear-trigger, plain-click-does-not-select,
  and edge-auto-scroll regressions still pass while interaction-lane picker/list
  work is in flight.
- `cargo test -p jackin-capsule
  frame_plans_keep_diff_tier_reasons_out_of_full_redraws --locked` — 1 passed
  in the 2026-06-08 fresh audit.
- `cargo test -p jackin-capsule
  pending_status_change_uses_no_clear_diff_frame --locked` — 1 passed in the
  2026-06-08 fresh audit.
- `cargo test -p jackin-capsule reset_clear_home_resets_style_before_erasing --locked`
  — 1 passed after routing raw attach-entry and first-attach pre-clear erases
  through `RESET_CLEAR_HOME` (`ESC[0m ESC[2J ESC[H]`). This closes the
  code-level path where a raw `ESC[2J` could inherit a previous tab/pane
  background colour and momentarily paint green/gray blocks before the Ratatui
  full frame landed.
- `cargo test -p jackin-capsule full_screen_clear_resets_style_before_erasing --locked`
  — 1 passed in the same audit, confirming the SocketBackend full-screen clear
  still resets SGR before erase.
- `rg -n "Full\\(FullRedrawReason::(DialogChange|PaletteOverlay|PaneClear|FocusChange|StatusChange|ScrollbackMovement|SelectionRepaint)\\)" crates/jackin-capsule/src/tui/update.rs crates/jackin-capsule/src/tui/update/tests.rs`
  — no hits after the planner invariant landed.
- `rg -n "compose_full_redraw\\(FullRedrawReason::(DialogChange|PaletteOverlay|PaneClear|FocusChange|StatusChange|ScrollbackMovement|SelectionRepaint)\\)" crates/jackin-capsule/src/daemon/input_dispatch.rs crates/jackin-capsule/src/daemon/mouse_input.rs crates/jackin-capsule/src/daemon/compositor.rs`
  — no hits for direct full-redraw calls with diff-tier reasons in production
  dispatch/compositor code.
- `rg -n "Full\\(FullRedrawReason::(DialogChange|PaletteOverlay|PaneClear|FocusChange|StatusChange|ScrollbackMovement|SelectionRepaint)\\)|compose_full_redraw\\(FullRedrawReason::(DialogChange|PaletteOverlay|PaneClear|FocusChange|StatusChange|ScrollbackMovement|SelectionRepaint)\\)|request_full_redraw\\(status_change_redraw_reason|compose_full_redraw\\(status_change_redraw_reason" crates/jackin-capsule/src -g '*.rs' -g '!**/tests.rs'`
  — no production hits in the 2026-06-08 fresh audit; remaining direct
  diff-tier clear calls are confined to tests that intentionally exercise the
  old full-render compositor helper.
- `rg -n "scrollback_offset\\s*=|\\.scrollback_offset\\s*=" crates/jackin-capsule/src/session.rs crates/jackin-capsule/src/daemon crates/jackin-capsule/src/tui --glob '*.rs'`
  — remaining writes are confined to `Session::scroll_by`,
  `Session::reset_scrollback_view`, and `Session::clamp_scrollback_offset`;
  daemon/tui code only reads the offset or calls session methods.
- `cargo test -p jackin-capsule scrollback --locked` — 12 passed after routing
  `scroll_to_live`, pane clear, and PTY `ScrollbackClear` through the single
  `Session::reset_scrollback_view` helper.
- `cargo test -p jackin-capsule clear_pane --locked` — 3 passed after the same
  scrollback reset centralization.
- `cargo clippy -p jackin-tui -p jackin-capsule --all-targets --all-features
  --locked -- -D warnings` — clean after the toast placement update.
- `cargo clippy -p jackin-capsule --all-targets --all-features --locked -- -D warnings` — clean after the Debug-info hover overlay routing fix.
- `docs/content/docs/reference/tui/chrome.mdx`,
  `docs/content/docs/reference/tui/dialogs.mdx`, and
  `docs/content/docs/reference/tui/navigation.mdx` now document
  status-preserving overlays, copy-success toast feedback outside the hint bar,
  and read-only content-coordinate pane selection rules.
- `docs/content/docs/reference/capsule/index.mdx` no longer says every capsule
  dialog hides all chrome; it names Debug info as the status-preserving
  exception and links to the shared dialog contract.
- Docs gates after the TUI design-doc update: `bun run build`,
  `bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test` from
  `docs/` all exit 0.

## Ground Rules

- Stay on `feature/tui-architecture`; do not create a new branch.
- Do not rewrite history or force-push without explicit operator authorization.
- Do not remove `cargo-audit`.
- Sign off any new commit with `git commit -s` and push immediately when the fix
  is complete. Per `AGENTS.md` Commit Attribution, every agent-authored commit
  also carries the agent's `Co-authored-by` trailer (for Claude Code:
  `Co-authored-by: Claude <noreply@anthropic.com>`; for Codex:
  `Co-authored-by: Codex <codex@openai.com>`).
- Use Conventional Commits types: findings fixes are `fix:`/`refactor:` with
  `test:` and `docs:` where the change is test-only or docs-only.
- Do not mutate host-side user state silently. Use the PR-scoped config/home
  paths from the PR template (see "Environment And Smoke Harness" below).
- Do not mark roadmap/checklist boxes `[x]` from inspection. A completed
  checklist item must include command output or diagnostics run ids.
- Defect 54 hardware/session checks stay `[ ]` until real run ids are captured.

Current observed checkout while this file was last extended:

- Branch: `feature/tui-architecture`
- Local head: `8b92b3c56611c8670afd41ca6bfd9bc1435a9beb`
  (`docs: add PR 495 TUI smoke handoff` — the commit that first added this file;
  later extensions of this file may still be uncommitted in the working tree)

The branch may advance. Before fixing, fetch and verify the current branch/head
rather than assuming this exact commit is still current.

## Source Of Truth

Read these before fixing:

- `AGENTS.md`
- `.github/AGENTS.md` — jackin-capsule smoke-test mandate, PR merge rules
- `PULL_REQUESTS.md` — Verify-locally block shape
- `TESTING.md` — nextest invocation conventions
- `docs/content/docs/reference/roadmap/post-restructure-fixes.mdx`
- `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`
- `docs/content/docs/reference/tui/index.mdx`
- `docs/content/docs/reference/tui/navigation.mdx`
- `docs/content/docs/reference/tui/dialogs.mdx`
- `docs/content/docs/reference/tui/components.mdx`

The canonical TUI design docs already exist under `docs/content/docs/reference/tui/`.
Extend those pages only after the implementation and verification prove the new
Debug info/dialog contract. Do not create a parallel TUI design page.

## Environment And Smoke Harness

Everything in this section is prerequisite for reproducing any finding and for
capturing the Defect 54 evidence.

### PR-scoped host paths

jackin' resolves its host directories from two environment variables, read in
`crates/jackin-core/src/paths.rs:33-34`:

- `JACKIN_HOME_DIR` — overrides `~/.jackin` (data, cache, roles, diagnostics).
- `JACKIN_CONFIG_DIR` — overrides `~/.config/jackin` (config.toml, workspaces).

The PR template (`.github/PULL_REQUEST_TEMPLATE.md`, Checkout block) isolates
smoke runs with:

```sh
export JACKIN_CONFIG_DIR="$JACKIN_PR_TEST_DIR/.config/jackin"
export JACKIN_HOME_DIR="$JACKIN_PR_TEST_DIR/.jackin"
```

The evidence run for this file used `JACKIN_HOME_DIR=/Users/donbeave/.jackin-pr-495`
(hence the `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/...` paths in
"Run Evidence"). Reuse the same root for follow-up smokes so old and new run ids
sit side by side.

### Capsule binary resolution trap (read before any capsule-side fix)

The capsule binary inside the container is resolved by `ensure_available` in this
order (`.github/AGENTS.md`, jackin-capsule section):

1. `JACKIN_CAPSULE_BIN=/path` env override — used directly, no cache.
2. Cache hit at `~/.jackin/cache/jackin-capsule/<version>/linux-<arch>/`.
3. Download from the GitHub preview/rolling release tag.

**If you change any `crates/jackin-capsule/` code and smoke without exporting a
freshly built binary, the container will happily run the cached or released
capsule and your fix will be invisible.** Findings 2, 3, 4, 5, 9, and 10 all
have capsule-side components. Before every capsule smoke run:

```sh
eval "$(cargo run --bin build-jackin-capsule -- --export)"
```

This one-shot build prints and `eval`-exports `JACKIN_CAPSULE_BIN` pointing at
the just-built Linux binary. Re-run it after every capsule-side code change.

### Smoke commands

Every smoke invocation must include `--debug` (AGENTS.md rule; it captures
external commands and `[jackin debug ...]` instrumentation into the run JSONL
and prints the run id at start). Debug mode already suppresses the intro — do
not add `--no-intro`.

```sh
# Console-first smoke (preferred):
cargo run --bin jackin -- console --debug

# Load-path smoke (only when the finding needs the load CLI path):
cargo run --bin jackin -- load the-architect . --debug
```

Prefer the `the-architect` role over `agent-smith` when a role choice is needed.

### Snapshot/test infrastructure notes

- `insta` (with the `filters` feature) is a dev-dependency of `crates/jackin`
  and `crates/jackin-capsule`. `crates/jackin-tui` currently asserts on rendered
  `Buffer` contents manually (see `container_info/tests.rs`); either pattern is
  acceptable for the new tests — follow the crate you are editing.
- `dind_e2e` tests are feature-gated (`#![cfg(feature = "e2e")]` at
  `crates/jackin/tests/dind_e2e.rs:7`). `--all-features` enables that feature,
  which is why the local verification command excludes the binary explicitly
  (see Phase 8). CI runs the full suite only after building the capsule and
  exporting `JACKIN_CAPSULE_BIN` (`.github/workflows/ci.yml`, nextest job).

## Run Evidence

Operator smoke run:

- Run id: `jk-run-533476`
- Host diagnostics JSONL (3,099 lines):
  `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.jsonl`
- JSONL `container_started` event (line 3078) points to capsule log
  (139,563 lines, ~42 MB):
  `/Users/donbeave/.jackin-pr-495/data/jk-paje1he3-thearchitect/state/multiplexer.log`
- Docker build log (clean build, exit 0, all 53 stages OK, no warnings):
  `/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.docker-build.log`

### How to read the run JSONL

Each line is one event: `{"ts_ms", "run_id", "trace_id", "kind", "message"}`
plus optional `"detail"`, `"stage"`, `"span_id"`. **`detail` is a JSON-encoded
string, not a nested object** — quotes inside it are escaped, so a plain
`rg '"capsule_log"'` matches nothing (the raw bytes are `\"capsule_log\"`).
That is why an earlier helper printed an empty `capsule_log=` field even though
the pointer exists. Use `jq ... | fromjson` instead:

```sh
RUN_JSONL=/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-533476.jsonl

# Capsule log path (works for any run id):
jq -r 'select(.kind=="container_started") | .detail | fromjson | .capsule_log' "$RUN_JSONL"

# End-of-run summaries (event counts, stage durations, exit reason):
jq -c 'select(.kind=="run_summary" or .kind=="exit_summary")' "$RUN_JSONL"

# Raw greps still work for non-detail fields:
rg -n 'capsule_log|error|panic|resize|render|dialog|scroll|mouse|key' "$RUN_JSONL"
rg -n 'bottom-chrome|wheel dispatch|scrollback-movement|render: kind=full|render: kind=partial' \
  /Users/donbeave/.jackin-pr-495/data/jk-paje1he3-thearchitect/state/multiplexer.log
```

### Quantified log facts (verified against the files above)

| Fact | Value | Where |
| --- | --- | --- |
| `bottom-chrome: site=ratatui` lines | 6,850 | multiplexer.log |
| `bottom-chrome: site=raw-full` lines | 6,850 (1:1 paired with ratatui) | multiplexer.log |
| `render: kind=full reason=scrollback-movement` | 3,442 | multiplexer.log |
| `render: kind=partial` | 7,343 (all `reason=pty-output`) | multiplexer.log |
| `wheel dispatch: jackin-scrollback` lines | 6,882 (= ~3,441 events; each event logs a `before=` and an `after=` line) | multiplexer.log |
| Wheel events → full redraws ratio | ≈1:1 (3,441 events vs 3,442 full redraws) | derived |
| `cockpit-dialog-mouse` events | 2,700 (run_summary `event_counts`; first occurrences at JSONL lines 72–74 with `container_info_open=true`) | jk-run-533476.jsonl |
| Errors/panics in either log | 0 | both |
| `container_started` event | JSONL line 3078 | jk-run-533476.jsonl |
| `exit_summary` / `run_summary` | JSONL lines 3090 / 3091 ("1 agent still in the Construct; boundary outro skipped") | jk-run-533476.jsonl |

Saturated no-op wheel event followed by a full redraw and both chrome sites
(multiplexer.log lines 2476–2480):

```text
2476: wheel dispatch: jackin-scrollback session=2 row=25 col=86 button=64 delta=3 before=9 filled=9
2477: wheel dispatch: jackin-scrollback session=2 after=9
2478: render: ratatui-frame damage=full panes=1 pane_screens=1
2479: bottom-chrome: site=ratatui term=149x39 frame_area=149x39 hint_y=36 sep_y=37 branch_bar_y=38 panes=1
2480: bottom-chrome: site=raw-full term=149x39 branch_bar_row=38 hint_row=36 debug_chip=jk-run-533476
```

Paired ratatui + raw-full chrome draw in the same frame family (lines 2415–2416):

```text
2415: bottom-chrome: site=ratatui term=149x39 frame_area=149x39 hint_y=36 sep_y=37 branch_bar_y=38 panes=1
2416: bottom-chrome: site=raw-full term=149x39 branch_bar_row=38 hint_row=36 debug_chip=jk-run-533476
```

Observed log facts in prose:

- The run JSONL does contain a capsule log pointer (line 3078) even though the
  helper printed an empty `capsule_log=` field — see the JSON-encoded `detail`
  note above for why the naive grep missed it.
- The capsule log records `bottom-chrome: site=ratatui` and
  `bottom-chrome: site=raw-full` in lockstep (6,850 each), meaning chrome is
  drawn through two paths in the same frame family. A third site,
  `site=dialog`, exists in code (`crates/jackin-capsule/src/tui/view.rs:104`)
  for dialog-open frames.
- Scrollback wheel events produce `render: kind=full reason=scrollback-movement`
  essentially once per event (3,442 full redraws for ~3,441 wheel events),
  including saturated no-op wheel events where `before=9 after=9`.
- The run JSONL records 2,700 `cockpit-dialog-mouse` events, and the early log
  lines (72–74) show modal mouse move/scroll events with
  `container_info_open=true` during the launch cockpit dialog — the Debug info
  dialog was absorbing a mouse-move flood.

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

- `crates/jackin-tui/src/components/container_info.rs` — `DebugInfo` model,
  `ContainerInfoState`, render + hit-test + hyperlink helpers
- `crates/jackin-tui/src/components/dialog_layout.rs` — `DialogBodyScroll`
  (line 44), `scroll_hint_spans()` (line 284)
- `crates/jackin-tui/src/components/hint_bar.rs` — `render_hint_bar()` (line 78)
- `crates/jackin-tui/src/components/status_footer.rs` — `render_status_footer()`
  (line 163; owns the standard hint/spacer/status stack)
- `crates/jackin-tui/src/components/modal_backdrop.rs` — `ModalBackdrop` (line 14)
- `crates/jackin-tui/src/components/text_input.rs` — `render_text_input()` (line 385)
- `crates/jackin-tui/src/components/scrollable_panel.rs`
- `crates/jackin-tui/src/scroll.rs` — shared scroll math incl.
  `ScrollState::scroll_by` (line 79) and `tail_vertical_thumb` (line 439)
- `crates/jackin-tui/src/geometry.rs` — `HintSpan` (line 32; note: the type
  lives here, not in hint_bar.rs)
- `crates/jackin-console/src/tui/components/modal_rects.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/components/modal_layout.rs` —
  `modal_outer_rect` (line 16)

Avoid adding parallel dialog/backdrop/scroll/copy implementations. If a helper is
missing, add it to the shared component layer and route all surfaces through it.

### Console crate duality (orientation note)

Two console trees exist and both are real: `crates/jackin-console` is the
extracted crate the `jackin` binary depends on
(`jackin-console = { path = "../jackin-console" }` in `crates/jackin/Cargo.toml`),
and `crates/jackin/src/console/` is the in-binary half that bridges to it (TUI
migration pending, per the AGENTS.md TUI code location table). Findings touch
both: e.g. `debug_run_info_state` lives in
`crates/jackin-console/src/tui/components/container_info.rs:11`, while the modal
layout used by the console lives in
`crates/jackin/src/console/tui/components/modal_layout.rs`. Do not "fix" the
duality in this goal; just put any new shared code in `crates/jackin-tui`.

### Known duplication hotspots (verified, DRY-rule targets)

- Two `render_footer` implementations:
  `crates/jackin-console/src/tui/view.rs:71` and
  `crates/jackin-launch/src/tui/components/footer.rs:27`.
- A capsule-local hint-span builder: `footer_hint_spans` at
  `crates/jackin-capsule/src/tui/components/dialog.rs:1317`.
- Two scroll-by implementations: shared `ScrollState::scroll_by`
  (`crates/jackin-tui/src/scroll.rs:79`) vs capsule
  `Session::scroll_by(i32)` (`crates/jackin-capsule/src/session.rs:673`,
  with `scrollback_filled` at line 705).
- Capsule `DialogBackdrop` is already a re-export of the shared widget
  (`pub use jackin_tui::components::ModalBackdrop as DialogBackdrop;` —
  `crates/jackin-capsule/src/tui/components/chrome.rs:195`), so the backdrop
  *widget* is shared; the bug is full-area usage, not a parallel widget.

## Canonical Debug Info Contract

All Debug info surfaces must use this contract. A surface may omit rows whose
data is unavailable; it must not change row order, labels, scroll behavior, copy
behavior, or footer/status behavior.

Primary ordering rule:

- If `Run ID` is available, it is row 1. This is true on every surface,
  including launch/capsule surfaces that also know a container id, versions,
  role, agent, target, or diagnostics path. Debug-mode rows are not appended
  after container rows when the run id is known; the shared model prepends the
  bare `Run ID` first and then renders the remaining known facts in canonical
  order.

Canonical title:

- `Debug info`

Canonical row order (source of truth: `DebugInfo::into_state`,
`crates/jackin-tui/src/components/container_info.rs:122-145`):

1. `Run ID` — copyable
2. `Container ID` — copyable
3. `jackin version`
4. `jackin-capsule`
5. `Role`
6. `Agent`
7. `Target`
8. `Diagnostics log` — copyable + OSC 8 hyperlink

Canonical row semantics:

- `Run ID` is always the bare run id, for example `jk-run-b93735`. It is never a
  `.jsonl` path.
- `Diagnostics log` is the full diagnostics JSONL path. It is copyable and uses
  an OSC 8 `file://` hyperlink when the terminal path is known.
- `Container ID`, `Run ID`, and `Diagnostics log` are copyable whenever present
  (the `.copyable()` builder on `ContainerInfoRow`,
  `crates/jackin-tui/src/components/container_info.rs:54`).
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
- `Run ID` is always the top row whenever available, including on surfaces that
  also know a container id, role, agent, target, and version rows.
- The shared default keyboard-copy target is the first copyable row in canonical
  row order (`Run ID` whenever present). Enter copies that value and keeps Debug
  info open so copied-row feedback can render; Esc/q dismiss.

Canonical scroll behavior:

- Debug info uses both-axis dialog body scrolling.
- A horizontal scrollbar appears whenever any rendered row is wider than the
  viewport.
- A vertical scrollbar appears whenever row count exceeds the viewport.
- Scroll hints advertise only axes that actually overflow
  (`debug_info_hint_spans(axes)`,
  `crates/jackin-tui/src/components/container_info.rs:335`, fed by
  `scroll_hint_spans`, `crates/jackin-tui/src/components/dialog_layout.rs:284`).
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
- Copied feedback must be visible as a transient overlay/toast near the
  top-right of the visible surface. It is not noisy log output and never belongs
  in the hint/footer row. The hint row is only for currently available actions;
  copy success is state feedback.
- The toast is non-modal: it must not take focus, hide the retained selection,
  alter footer hints, or block input. It appears after a successful selection
  copy, remains briefly on a deterministic timer, and expires on its own.
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
- Footer/hints must advertise only available actions. Copy success feedback is
  state feedback, so it belongs in the transient overlay/toast layer instead.

## Binding Design Rules (verbatim)

These published rules are the pass/fail review criteria for findings 1, 4, 5, 7,
8, 9, and 10. Quoted from the docs so a reviewer can judge a diff without
opening the site:

From `docs/content/docs/reference/tui/dialogs.mdx`:

> "The reserved status/hint rows at the bottom of every screen are inviolable:
> no modal, dialog, or border may draw onto them."

> "All modal rects are computed against the **content area** (full terminal
> minus footer height), not the full terminal area."

> "The modal backdrop is rendered over the screen minus the reserved footer
> rows, so the modal-aware footer stays visible beneath the dialog."

> "A modal is not a special case — when one is open, its keys replace the screen
> keys in the **same reserved footer rows**. There is no floating hint bar under
> a dialog; hints always live in the fixed footer."

From `docs/content/docs/reference/tui/navigation.mdx`:

> "All keyboard hint text belongs in the footer bar at the bottom of the screen.
> Dialogs, modals, and overlays **must not** render their own internal hint
> line."

> "A full-screen overlay that hides the footer still renders its hint as the
> bottom row of the screen, outside its box — not as an internal line of the
> box."

> "A surface may show `↑↓ scroll` only when its content overflows vertically,
> `←→ scroll` only when it overflows horizontally" — derived from the "same
> `is_scrollable` gate the scrollbar uses".

> Clickable targets must look clickable: every element that performs an action
> on click must expose both the hand pointer on hover and a hover style change.

From `docs/content/docs/reference/tui/components.mdx`:

> "The 'Debug info' dialog is a single reusable component, not a per-surface
> reimplementation. Every surface that shows it — the console manager, the
> launch cockpit, and the in-container capsule — builds it from the shared
> `DebugInfo` model."

The console-side mechanism the docs name for footer preservation is
`prepare_visible_modal()` (`crates/jackin/src/console/tui/layout/prepare.rs:43`),
which subtracts `footer_height` before centering modals. Launch and capsule
have no equivalent today — that is the structural gap behind findings 4 and 7.

## Application-Wide Refactor Map

The ten findings are instances of six application-wide patterns. Per the
"Fix the whole class, never one instance" rule in
`docs/content/docs/reference/roadmap/post-restructure-fixes.mdx`, the fix for
each finding must migrate every site in the corresponding inventory below, not
only the site the operator's screenshot happened to show. This map is the
verified current-state inventory (five repo sweeps, 2026-06-08); treat each
list as the migration work-list for the matching phases.

### R1 — Dialog shell, backdrop, and chrome preservation (F1, F4, F5, F7, F8)

Scale: 47 dialog-like surfaces (capsule 9, console 35 including the
SettingsEnv/GlobalMount/SettingsAuth sub-modal families, launch 3) and
24 modal-geometry functions outside `jackin-tui` (console 15 in
`modal_rects.rs` + file-browser rects, launch 8 popup/failure rects, capsule 1
`Dialog::box_rect` at `dialog.rs:1255`).

Per-surface current state:

- Capsule: the 9 `Dialog` variants render through one local shell
  (`render_dialog_ratatui`, `dialog_widgets.rs:337`) over shared
  `dialog_layout`/`Panel` widgets. The original divergence was at the *frame*
  layer: `render_capsule_ratatui_frame` painted `DialogBackdrop` over
  `frame.area()` and returned before the top `StatusBarWidget`
  (`STATUS_BAR_ROWS = 2`, `status_bar.rs:52`). Current code renders
  `StatusBarWidget` first, paints `DialogBackdrop` only below the reserved
  status rows, then renders the dialog in that content area. The raw bottom
  chrome (branch bar + hints) remains a documented structural exception appended
  by the compositor via `render_capsule_dialog_bottom_chrome` (`view.rs:99`)
  unless `blank_background`; it is cached and re-emitted only when needed.
- Console: every modal rect derives from `modal_rects.rs` +
  `prepare_visible_modal` (footer height subtracted — the model to
  generalize). One sweep flagged console modal hints as floating-internal,
  which contradicts the dialogs.mdx modal-aware-footer rule; verify per modal
  during the Phase 4 audit instead of trusting either claim.
- Launch: `dialog_backdrop()` (`dialog.rs:13`) now derives its body and hint
  rows from `bottom_chrome_areas()`, so cockpit overlays keep the footer row
  reserved. Build-log and failure popup paths also use the same bottom-chrome
  body/hint/footer split; remaining launch-local rect helpers are structural
  sizing adapters over shared dialog primitives.

Target: keep one shared modal layer in `jackin-tui` that takes content area and
reserved chrome area as separate inputs. Any surviving local rect function must
stay a thin structural adapter with a one-line justification, not a parallel
top-level modal implementation. The old capsule full-frame Debug-info early
return is retired and must not reappear.

### R2 — Bottom-chrome stack (F1, F7)

Scale: 6 footer renderers, 23 hint builders (17 const tables + 6 functions),
5 height constants, 16 render call sites (14 Ratatui + 2 raw ANSI), 8 local
bottom-row math sites.

Spacer-policy divergence (the F1 class):

- Console: compliant — `footer_height()` (`jackin-console/src/tui/view.rs:66`)
  enforces a +1 spacer row above hints; `render_footer` (`view.rs:71`) splits
  it explicitly.
- Capsule raw path: compliant — `CAPSULE_HINT_SEPARATOR_ROWS = 1`
  (`layout.rs:16`) encodes the same gap.
- Launch: fixed at the code/test level — build-log, pre-cockpit prompt, and
  failure-popup paths derive body/hint/footer placement from
  `bottom_chrome_areas()`. The original divergent build-log path composed a
  tight 2-row hint+status stack with no spacer; keep that regression covered by
  `cargo test -p jackin-launch build_log --locked` and live smoke.

Target: one shared bottom-chrome stack primitive (hint rows + spacer +
status footer) built on `bottom_chrome_areas`, `render_status_footer`
(`status_footer.rs:163`), and `render_hint_bar` (`hint_bar.rs:78`). The capsule
raw emitter (`render_hint_row`, `dialog/hint.rs:120`) stays as the one
documented non-Ratatui adapter but derives row offsets from the same height
constants. Hint builders consolidate on `HintSpan` vocabulary; floating-internal
hint rows go to zero.

### R3 — Scroll unification (F2, F3)

Scale: 19 shared scroll APIs in `jackin-tui/src/scroll.rs` (including
`is_scrollable`:102, `TailScroll`:64, `mouse_scroll_delta`:167,
`offset_for_track_position`:335, `tail_vertical_thumb`:439) plus
`DialogBodyScroll` (`dialog_layout.rs:44`) and the scrollable-panel helpers;
12 files already consume them; ~35 scroll-state sites still hold local state;
11 wheel-dispatch sites (3 through shared helpers, 2 mutating scroll fields
directly, 6 local dispatch).

Migration buckets:

- Obvious-yes (mechanical): capsule dialog wheel arms mutate
  `scroll.scroll_x/scroll_y` directly (`input_dispatch.rs:344,354`;
  `dialog.rs:645-657`) — replace with `DialogBodyScroll::on_mouse_scroll*`;
  console `confirm_save.rs:46-158` single-axis offset → `DialogBodyScroll`;
  console `focus.rs:34-57` cursor-follow math → shared follow helper;
  `list_geometry.rs:35` manual clamp → `clamp_offset_u16`.
- Needs-adapter: console editor/settings/workspaces plan-based scrolling
  (`EditorScrollFocusPlan`, `SettingsScrollFocusPlan`,
  `WorkspaceListScrollFocusPlan` in the respective `update.rs`) — keep the
  plan shape, route offset arithmetic through shared helpers.
- Already shared, keep: capsule `Session::scroll_by`/`clamp_scrollback_offset`
  wrap `TailScroll`; launch build-log metrics call `scroll::max_offset`;
  lookbook uses `scroll_selectable_list`/`apply_mouse_scroll_u16`.
- F2's fix point: `apply_pane_scrollbar` gate (`view.rs:322-324`) switches
  from `offset > 0` to the same `is_scrollable` gate the hints rule requires.

### R4 — Mouse, hover, and copy interaction (F9)

Scale: 6 mouse-dispatch entry functions across 3 surfaces; 11 hover
implementations (1 shared `HoverTracker`, `hover_tracker.rs:21-97`, plus 10
surface-specific); 13 copy-related sites — OSC 52 encoding is already shared
(`encode_osc52_clipboard_write`, `jackin-tui/src/lib.rs:299`) but 8 trigger
sites and 4 `mark_copied` feedback wirings are per-surface; 7 hit-test
helpers (1 shared point-in-rect + 6 domain-specific); pointer-shape (OSC 22)
plumbing exists on all three surfaces (`PointerShape`, capsule
`app.rs:53-108`; console `mouse.rs:270-290`; launch
`subscriptions.rs:216-285`).

Target: a shared dialog-interaction controller in `jackin-tui` that owns
hover-row derivation, copy hit-testing, copied-badge state, and pointer-shape
selection for copyable rows, so console/launch/capsule mouse entry points
translate coordinates and delegate instead of re-implementing the
move/click/copy state machine three times.

### R5 — Pane text selection (F10)

Original implementation (all capsule): `SelectionState` stored anchor/end in
0-based grid coordinates relative to the pane inner rect — screen-relative,
exactly the F10 root cause; lifecycle actions `StartSelection`/
`SelectionMotion`/`FinalizeSelection` (`input_dispatch.rs`,
`mouse_input.rs`); extraction `selection_text()` and highlight projection used
the visible screen snapshot.

Current focused-test state: `SelectionState` rows are retained-content
coordinates (scrollback oldest-first, then live screen), `visible_selection()`
projects that range into the current viewport for highlighting, and
`render_content_snapshot()` copies from the full scrollback+live content
snapshot. Pane primary press stores only a pending anchor until button motion
leaves the anchor cell, so plain pane clicks do not flash selection chrome or
arm clipboard copy. `cargo test -p jackin-capsule selection --locked` passes 25
tests, including content-row start under scrollback, pending press-vs-drag
selection behavior, persisted highlight, clear-on-click/type, upward and
downward edge auto-scroll, and `Selection copied` feedback constrained to the
pane/content overlay area so the status rows and hint/spacer/footer rows stay
reserved for screen chrome. Live smoke still has to confirm the same behavior in
a real capsule session.

Remaining target: live-smoke the focused-test behavior in a real capsule
session and capture run id/log evidence before ticking F10. If the smoke finds
edge-auto-scroll cadence or selection-copy extraction gaps, fix them in the same
content-coordinate model rather than returning to screen-relative rows.

### R6 — Redraw-tier classification (F3)

Verified starting architecture: all 15 `FullRedrawReason` variants
(`update.rs:17`) originally routed through `compose_full_redraw`
(`compositor.rs:56`), which calls `terminal.clear()` (`compositor.rs:67`) —
emitting `ESC[2J` and forcing full recomposition for non-PTY actions including
wheel scrollback, dialog hover, selection drag repaint, focus change, and the
status ticker. Focused fixes have since moved real scrollback wheel movement to
partial pane frames, typed-input scrollback snap to the no-clear diff frame
path, Debug-info copy-target hover and chrome hover/status repaint to the
no-clear overlay path, selection repaint to a generic no-clear diff frame path,
keyboard/mouse focus repaint to the no-clear diff frame path, and queued
status-only refreshes to a pending diff-redraw slot;
`cargo test -p jackin-capsule hover --locked` now proves dialog hover repaint
does not emit `ESC[2J`,
`cargo test -p jackin-capsule apply_action_mouse_chrome_update_sets_pointer_shape --locked`
proves chrome hover repaint does not emit `ESC[2J`, and
`cargo test -p jackin-capsule selection --locked` proves selection
start/motion/edge-scroll/finalize/clear repaint does not emit `ESC[2J`.
`cargo test -p jackin-capsule focus --locked` proves keyboard and mouse focus
repaint do not emit `ESC[2J`.
`cargo test -p jackin-capsule typed_input_snaps_scrollback_to_live_without_screen_erase --locked`
proves typed-input scrollback snap repaint does not emit `ESC[2J`.
`cargo test -p jackin-capsule pending_status_change_uses_no_clear_diff_frame --locked`
proves queued status refresh repaint does not emit `ESC[2J`, while
`cargo test -p jackin-capsule pending_full_redraw_takes_precedence_over_status_diff --locked`
proves geometry/layout full redraws still override queued status diffs.
Remaining diff-tier routes still need the convergence sweep before F3 can be
ticked. The bottom chrome is cached (`last_bottom_chrome`,
`compositor.rs:345`) and re-emitted only on change, so the original visible
flicker came from unconditional clears, not chrome duplication. Console and
launch render loops are plain full-frame Ratatui draws relying on cell diffing
— no clear, no flicker class.

Target tiers:

- Clear-tier (geometry truly invalidated): `FirstAttach`, `Resize`,
  `LayoutChange`, `SplitClose`, `ZoomChange`, `SessionExit`,
  `ExplicitRedraw` (operator Ctrl-L semantics).
- Diff-tier (recompose without `terminal.clear()`, let the cell diff emit the
  delta): `ScrollbackMovement`, `DialogChange`, `SelectionRepaint`,
  `PaletteOverlay`, `FocusChange`, `PaneClear`, `StatusChange`.
- No-op tier: saturated wheel events and hover moves that change no state
  skip composition entirely (the F3 before/after offset comparison).

### Convergence metrics (app-wide definition of done)

The refactor is complete when these counts hold, verified by fresh sweeps:

- Modal-geometry functions outside `jackin-tui`: 24 → 0, or each survivor
  carries a one-line structural justification comment.
- Fresh 2026-06-08 sweep status: the production modal-geometry survivor sweep
  still lists the local console/launch/File Browser geometry adapters plus one
  test-only manufactured modal area, and each production survivor now carries a
  `Structural exception` comment naming why that geometry remains local (state-
  dependent size adapter, File Browser child overlay, or in-binary console
  bridge) instead of a parallel top-level modal implementation. Verification:
  `rustfmt --check` on the touched modal geometry files exits 0, and
  `rg -n "Structural exception" ...` finds the expected production
  justifications.
- Floating-internal dialog hint rows: 0 (navigation.mdx rule holds
  everywhere).
- Bottom-chrome stacks: 6 renderers → 1 shared stack + the documented capsule
  raw adapter, both reading the same height constants.
- Fresh 2026-06-08 sweep status: launch build-log, pre-cockpit prompt, and
  failure-popup paths now derive body/hint placement from
  `bottom_chrome_areas()`. `cargo test -p jackin-launch --locked` exits 0 with
  34 tests, including focused prompt and failure-popup bottom-chrome
  regressions.
- Fresh 2026-06-08 final convergence audit: `cargo test -p jackin-console
  footer_hints --locked` exits 0 (20 passed), proving console modal/list/action
  hints are generated for the reserved footer instead of floating inside dialog
  bodies.
- Direct mutations of scroll fields outside shared scroll methods: 2 → 0.
  Fresh sweep exits with no hits after `clamp_list_names_scroll()` moved to the
  shared `clamp_scroll_offset()` helper.
- Fresh 2026-06-08 sweep status: `rg -n "scroll_[xy]\\s*=|\\.scroll_[xy]\\s*=|scrollback_offset\\s*=|\\.scrollback_offset\\s*=" crates/jackin/src crates/jackin-console/src crates/jackin-launch/src crates/jackin-capsule/src -g '*.rs' | rg -v "tests|test_|jackin-tui|session.rs|state/manager.rs|layout/list.rs|message.rs|update.rs|let mut scroll_[xy]|scroll_x = 0u16|scroll_y = u16::try_from|scrollback_offset = session.scrollback_offset|scrollback_offset ==|scrollback_offset,|scrollback_offset\\)"`
  exits with no hits. This confirms the direct scroll-field mutation metric is
  still green while the separate interaction-lane list work is in flight.
- Wheel handlers bypassing shared delta/clamp helpers: 0.
- `compose_full_redraw`/`request_full_redraw` callers that clear the terminal
  for status-only refreshes: 0 by source sweep; remaining diff-tier routes
  still need the final convergence sweep before the broader metric can be
  closed. Saturated scrollback wheel events produce no frame.
- Fresh 2026-06-08 sweep status: the production-only diff-tier full-redraw
  search exits with no hits.
- Capsule raw bottom chrome is now a documented structural exception rather
  than an unexplained parallel renderer: `compose_ratatui_frame()` keeps the
  attach-tail hint/branch/debug-chip rows in a raw ANSI adapter, caches the
  rendered bytes, suppresses unchanged re-emits on diff frames, and re-emits
  only when the chrome content changes or after a screen clear. Evidence:
  `cargo test -p jackin-capsule unchanged_diff_frame_suppresses_cached_raw_bottom_chrome --locked`
  exits 0 (1 passed).
- Shared selected-line renderers: filtered picker rows route through
  `render_selected_lines_in_area` -> `ScrollableList`; selected backgrounds fill
  the content width and leave the scrollbar gutter owned by the scrollbar.
- Bordered selectable-list renderers: File Browser uses
  `ScrollableList::render_with_block`; selection cursor, row fill,
  selection-follow offset, and border scrollbar placement are shared instead of
  hand-styled in the File Browser renderer. File Browser wheel routing captures
  scroll events before background panels on editor, create-prelude, settings
  mounts, and settings auth source-folder owners; saturated edge events are
  consumed by the modal instead of leaking through.
- Fresh 2026-06-08 final convergence audit: `cargo test -p jackin-tui
  scrollable_panel --locked` exits 0 (33 passed), `cargo test -p jackin-tui
  select_list --locked` exits 0 (5 passed), `cargo test -p jackin-console
  file_browser --locked` exits 0 (55 passed), and `cargo test -p jackin-console
  workspaces --locked` exits 0 (20 passed). The production sweep for
  `List::new`, `widgets::List`, `render_selected_lines_in_area`,
  `ScrollableList::new`, `render_select_list`, and `SelectList::new` shows
  remaining list construction either inside `jackin-tui` shared primitives or
  row-content assembly that feeds those primitives (`render_picker_list`,
  File Browser, workspace list).
- Debug info renderers: exactly 1 shared shell; per-surface code is fact
  assembly + state storage only.
- Fresh 2026-06-08 final convergence audit: `cargo test -p jackin-tui
  container_info --locked` exits 0 (12 passed), `cargo test -p jackin-launch
  container_info --locked` exits 0 (4 passed), and `cargo test -p
  jackin-capsule container_info --locked` exits 0 (20 passed). The production
  sweep for `render_container_info`, `ContainerInfoState`, `DebugInfo`,
  `container_info_dialog`, and `debug_run_info_state` shows one shared shell in
  `jackin-tui`; console, launch, and capsule only assemble facts, keep state,
  or bridge clicks/overlays into the shared renderer.
- Pane selection stored in screen coordinates: 0 (content-coordinate model
  only).
- The convergence box still stays open only because the final live `--debug`
  smoke run id has not yet proven these converged paths in an operator terminal.

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
  reason=scrollback-movement` — in a post-fix smoke log, the count of
  `reason=scrollback-movement` full redraws must be at most the count of
  offset-*changing* wheel events, not ≈1:1 with all wheel events as in
  `jk-run-533476` (3,442 full redraws / ~3,441 events).
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
- Spacing follows the same TUI chrome convention used elsewhere
  (`render_status_footer`,
  `crates/jackin-tui/src/components/status_footer.rs:163`, which places a 1-row
  spacer above the status footer).

Actual behavior:

- Hint row is immediately adjacent to the status/footer row.

Relevant files:

- `crates/jackin-launch/src/tui/components/build_log_dialog.rs`
- `crates/jackin-launch/src/tui/components/footer.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/status_footer.rs`
- `crates/jackin-tui/src/components/hint_bar.rs`

Starting anchors:

- `build_log_box_area` — `build_log_dialog.rs:28`
- `render_build_log_dialog` — hint/footer row math at `build_log_dialog.rs:197-206`
- `render_footer` — `footer.rs:27`
- `render_status_footer` — `status_footer.rs:163`
- `render_hint_bar` — `hint_bar.rs:78`

Evidence (verified):

- Original evidence run `jk-run-533476` showed the Docker build-log hint row
  adjacent to the status/footer row.
- The code path now uses the shared `bottom_chrome_areas()` helper in
  `crates/jackin-tui/src/components/bottom_chrome.rs`.
- `build_log_box_area()` delegates to `bottom_chrome_areas(area).body` in
  `crates/jackin-launch/src/tui/components/build_log_dialog.rs`, so scroll
  metrics, scrollbar hit-testing, wrapping, and rendering all reserve the same
  bottom stack.
- `render_build_log_dialog()` consumes `chrome.hint` and `chrome.footer` from
  the same helper; there are no build-log-local bottom-row constants left.
- `cargo test -p jackin-tui bottom_chrome --locked` exits 0 (2 passed).
- `cargo test -p jackin-launch build_log --locked` exits 0 (11 passed) and
  includes assertions for hint, blank spacer, debug run id, and instance footer
  placement.
- `rg -n "BUILD_LOG_BOTTOM_ROWS|BUILD_LOG_HINT_ROW_FROM_BOTTOM|BUILD_LOG_FOOTER_ROW_FROM_BOTTOM|area\\.height\\.saturating_sub\\(2\\)|area\\.height\\.saturating_sub\\(3\\)" crates/jackin-launch/src/tui/components/build_log_dialog.rs crates/jackin-tui/src/components/bottom_chrome.rs`
  exits 1 with no matches, proving the local constants and stale height math are
  gone from the build-log overlay path.

Suspected root cause:

- Fixed at the code/test level: the build-log overlay was manually composing
  bottom rows; it now uses the shared bottom-chrome stack. The item remains
  open only because real terminal smoke has not yet captured the final visual
  proof.

Blocks checklist:

- Blocks Defect 54 visual smoke polish.
- Related to the earlier footer spacing design rule.

Acceptance:

- Build log overlay bottom chrome matches the standard status/hint spacing.
- Add/adjust tests or snapshots that prove the spacer row exists. The existing
  test `build_log_overlay_keeps_status_footer_in_debug_mode`
  (`build_log_dialog.rs:405`) already asserts footer presence — extend it (or
  add a sibling) to assert the spacer row, since it currently passes against
  the broken layout.
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

1. Start a real capsule session under `--debug` (rebuild + export
   `JACKIN_CAPSULE_BIN` first — see "Capsule binary resolution trap").
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

- `apply_pane_scrollbar` — gating condition at `view.rs:322-324`
- `tail_vertical_thumb` — `crates/jackin-tui/src/scroll.rs:439`
- `scrollback_filled` — `crates/jackin-capsule/src/session.rs:705`
- `Session::scroll_by` — `crates/jackin-capsule/src/session.rs:673` (capsule-local;
  shared equivalent is `ScrollState::scroll_by`, `crates/jackin-tui/src/scroll.rs:79`)

Evidence (verified):

- Original evidence run `jk-run-533476` showed missing/unstable pane scrollbars
  before the operator scrolled.
- `apply_pane_scrollbar()` uses the shared tail-scroll adapter:
  `jackin_tui::scroll::tail_vertical_thumb(interior_rows, filled, offset)` in
  `crates/jackin-capsule/src/tui/view.rs`.
- Current rendering gates on `filled > 0` only, so retained scrollback draws a
  thumb even at the live tail (`offset == 0`).
- `cargo test -p jackin-capsule retained_scrollback_draws_scrollbar_at_live_tail --locked`
  exits 0 (1 passed) and proves a pane with retained scrollback paints a
  scrollbar before the operator manually scrolls back.
- `rg -n "filled > 0|offset > 0|scrollback_offset != 0|apply_pane_scrollbar|tail_vertical_thumb" crates/jackin-capsule/src/tui/view.rs crates/jackin-capsule/src/daemon/compositor.rs`
  shows the pane scrollbar render path uses `filled > 0`; remaining
  `scrollback_offset != 0` matches are compositor state checks, not scrollbar
  visibility gates.

Suspected root cause:

- Fixed at the code/test level: capsule pane scrollbars now use the shared
  `tail_vertical_thumb` adapter and visibility is based on retained scrollback
  (`filled > 0`), not current scrollback offset. The item remains open because
  real `--debug` smoke has not yet captured visual stability in a live session.

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

1. Start a multi-pane capsule session under `--debug` (rebuild + export
   `JACKIN_CAPSULE_BIN` first).
2. Scroll a pane up/down with the mouse wheel.
3. Continue scrolling after the pane has reached max scrollback.
4. Watch for full-screen flicker.

Expected behavior:

- Wheel events that do not change scroll offset should not redraw.
- Wheel events that do change scroll offset should repaint only the needed region.
- Bottom chrome should not be redrawn through competing paths.

Actual behavior:

- Every wheel event returns a full redraw for `scrollback-movement` (≈1:1 in the
  evidence run: 3,442 full redraws for ~3,441 wheel events).
- Saturated no-op wheel events still full-redraw (multiplexer.log lines
  2476–2480: `before=9 filled=9` → `after=9` → `damage=full`).

Relevant files:

- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/daemon/compositor.rs`
- `crates/jackin-capsule/src/tui/update.rs`
- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/render.rs`

Starting anchors:

- Wheel dispatch + redraw request — `input_dispatch.rs:427-443`:
  `session.scroll_by(delta)` at line 437; line 443 unconditionally returns
  `Some(self.compose_full_redraw(wheel_scrollback_redraw_reason()))` — no
  before/after offset comparison.
- `wheel_scrollback_redraw_reason` — `crates/jackin-capsule/src/tui/update.rs:195`
  (the `ScrollbackMovement` reason variant is in `update.rs:44`).
- `compose_full_redraw` — `compositor.rs:56`.
- `compose_direct_dirty_pane_frame` — `compositor.rs:431` (the existing partial
  path; logs `render: kind=partial reason=pty-output ... via=direct-grid-patch`
  at `compositor.rs:469`). This is the path scrollback movement could reuse.

Evidence (verified):

- Original capsule log lines around the wheel repro show (lines 2476–2480):
  - `wheel dispatch: jackin-scrollback ... before=9 filled=9`
  - `wheel dispatch: jackin-scrollback ... after=9`
  - `render: ratatui-frame damage=full ...` followed by both bottom-chrome sites
- Counts over the whole run: 3,442 `kind=full reason=scrollback-movement`
  vs 7,343 `kind=partial` (all partials are `reason=pty-output`; scrollback
  movement never takes the partial path today).
- Resize/render logs show `bottom-chrome: site=ratatui` and
  `bottom-chrome: site=raw-full` strictly paired, 6,850 each.
- Current wheel dispatch records the result of `session.scroll_by(delta)` and
  returns `None` when it is saturated/no-op; offset-changing scrollback returns
  `compose_partial_frame(...)`.
- `cargo test -p jackin-capsule apply_action_wheel_noops_at_scrollback_boundary --locked`
  exits 0 (1 passed) and proves saturated wheel scroll does not request a
  redraw.
- `cargo test -p jackin-capsule apply_action_wheel_scrolls_scrollback --locked`
  exits 0 (1 passed) and proves offset-changing wheel scroll moves the
  scrollback offset.
- `rg -n "compose_full_redraw\\([^\\n]*(wheel_scrollback|ScrollbackMovement)|wheel_scrollback_redraw_reason\\(" crates/jackin-capsule/src/daemon crates/jackin-capsule/src/tui crates/jackin-capsule/tests`
  finds only the vocabulary helper and its vocabulary test; no daemon dispatch
  path still composes a full redraw for wheel scrollback.

Suspected root cause:

- Fixed at the code/test level: wheel dispatch now suppresses saturated no-op
  redraws and routes offset-changing scrollback through the partial frame path.
- Bottom chrome ownership is split between Ratatui and raw append paths (sites
  at `view.rs:267` and `view.rs:41`; a third `site=dialog` at `view.rs:104`).

Blocks checklist:

- Blocks Defect 54 resize/scrollback/zero-ghosting.
- Blocks live performance run-id confidence for bytes-on-wire and present-frame
  behavior.

Acceptance:

- No-op scroll events do not request redraw.
- Offset-changing scroll events avoid whole-screen flicker.
- Logs prove no saturated scrollback event creates a full redraw.
- Add focused tests for no-op wheel dispatch and scrollback redraw reason.
  Existing redraw-reason tests to extend:
  `wheel_redraw_reason_uses_visible_update_vocabulary`
  (`crates/jackin-capsule/src/tui/update/tests.rs:156`) and
  `pane_data_redraw_reason_prioritizes_scrollback_snap` (`tests.rs:100`).
- Validate in a real `--debug` capsule run and record the run id.

Close when:

- A unit test proves saturated wheel scroll does not request a redraw.
- A focused integration/compositor test proves an offset-changing scroll does not
  redraw unrelated chrome unless required.
- A live `--debug` run id includes log evidence for saturated wheel events with no
  full redraw (grep the new multiplexer.log: saturated `before=N ... after=N`
  pairs must not be followed by `kind=full reason=scrollback-movement`).

### 4. Debug Info Dialog Hides Capsule Status Bar

Status: open.

Observed behavior:

- Opening Debug info inside the capsule replaces the whole screen with a blank
  modal backdrop.
- The capsule status bar is not displayed.

Repro steps:

1. Start a capsule session under `--debug` (rebuild + export
   `JACKIN_CAPSULE_BIN` first).
2. Open Debug info from the capsule status/chrome.
3. Observe whether the status bar remains visible.

Expected behavior:

- If the screen had a status bar before opening Debug info, that status bar
  remains visible.
- Debug info is an overlay over the content area, not a full-screen replacement
  that covers status/footer chrome.

Actual behavior:

- Fixed at the focused-test level: the capsule frame renders `StatusBarWidget`
  first, paints the dialog backdrop only below the two-row status bar, renders
  Debug info through the shared `render_container_info()` path, and then returns
  without drawing panes behind the modal.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-capsule/src/tui/components/chrome.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`
- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`

Starting anchors:

- `render_capsule_ratatui_frame` — dialog-open early return at `view.rs:234-239`
- `StatusBarWidget` — `crates/jackin-capsule/src/tui/components/chrome.rs:32`
- `DialogBackdrop` — re-export of shared `ModalBackdrop` at `chrome.rs:195`
- `DialogRatatuiSnapshot::DebugInfo` — mapping at `dialog_widgets.rs:251-254`
  (calls `Dialog::container_info_state()`,
  `crates/jackin-capsule/src/tui/components/dialog.rs:399`)
- Original stale helper: `render_container_info_on_blank` in
  `crates/jackin-tui/src/components/container_info.rs` painted a backdrop over
  `full_area`; current code has retired that helper and exports only
  `render_container_info()`.

Evidence (verified):

- Original evidence: `render_capsule_ratatui_frame()` treated
  `view.dialog_open` as screen-owning (`view.rs:234-239`):
  `frame.render_widget(DialogBackdrop, frame.area())`, render dialog, `return`
  — `StatusBarWidget` was never reached.
- Original evidence: `DialogRatatuiSnapshot::DebugInfo` called
  `render_container_info_on_blank()`, and that helper painted `ModalBackdrop`
  over the full terminal area.
- Current evidence: `DialogRatatuiSnapshot::DebugInfo` calls
  `render_container_info()` in the dialog-owned area, and
  `rg -n "render_container_info_on_blank|blank_render_clears_full_background" crates docs`
  has no production/test/doc hits.
- Current `render_capsule_ratatui_frame()` renders `StatusBarWidget` before the
  dialog branch, computes `backdrop_area` with
  `y = STATUS_BAR_ROWS`, and paints `DialogBackdrop` only in that area.
- `cargo test -p jackin-capsule debug_dialog_keeps_status_bar_visible --locked`
  exits 0 (1 passed) and proves Debug info plus the brand/tab row and active-tab
  underline row are visible in the same frame.
- `cargo test -p jackin-capsule view --locked` exits 0 (9 passed), including
  the Debug info/status-bar test and the dialog bottom-chrome tests.
- `rg -n "render_container_info_on_blank|blank_render_clears_full_background|frame\\.render_widget\\(DialogBackdrop, frame\\.area\\(\\)\\)" crates/jackin-capsule/src crates/jackin-tui/src/components crates/jackin-launch/src/tui crates/jackin/src/console/tui -g '*.rs' -g '!**/tests.rs'`
  exits 1 with no production hits; the only full-frame backdrop match is a
  widget unit test.
- Note: the backdrop *widget* is already shared (`chrome.rs:195` aliases
  `ModalBackdrop`); the defect is the full-frame area + early return, not a
  duplicate widget.

Suspected root cause:

- Fixed at the code/test level: capsule Debug info no longer uses a screen-owning
  full-frame backdrop over the status bar. The item stays open until a real
  `--debug` smoke run confirms the visible status bar in the live capsule.

Blocks checklist:

- Blocks Defect 54 visual smoke.
- Violates `docs/content/docs/reference/tui/dialogs.mdx` status/footer rules
  (see "Binding Design Rules" above).

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

- `DebugInfo::into_state` — `crates/jackin-tui/src/components/container_info.rs:122-145`
- `debug_run_info_state` — `crates/jackin-console/src/tui/components/container_info.rs:11`
- `launch_container_info_state` — `crates/jackin-launch/src/tui/components/container_info_dialog.rs:14`
- `Dialog::container_info_state` — `crates/jackin-capsule/src/tui/components/dialog.rs:399`
- `render_container_info` — `container_info.rs:384`
- Original stale helper: `render_container_info_on_blank` in
  `container_info.rs` (retired; shared callers now use
  `render_container_info()`).

Evidence (verified):

- `DebugInfo` already defines canonical row order and labels
  (`container_info.rs:97-145`).
- Console renders `render_container_info()` into a modal area and relies on
  reserved footer hints.
- Launch renders `render_container_info()` and `render_debug_info_hint()` from
  `render_launch_container_info()`.
- Capsule `DialogRatatuiSnapshot::DebugInfo` renders through the same
  `render_container_info()` function.
- Original evidence: launch and capsule used `render_container_info_on_blank()`
  and painted a full blank backdrop. Current code has retired that helper; any
  new Debug info regression must be fixed in the shared `render_container_info()`
  path plus each surface's content-area modal placement.
- `cargo test -p jackin-tui container_info --locked` exits 0 (9 passed).
- `cargo test -p jackin-console container_info --locked` exits 0 (1 passed).
- `cargo test -p jackin-launch container_info --locked` exits 0 (4 passed).
- `cargo test -p jackin-capsule container_info --locked` exits 0 (20 passed).

Suspected root cause:

- Fixed at the shared component/test level for the Debug info row model,
  renderer, copy affordances, hit-tests, scrollbars, and hyperlink overlays.
  Per-surface code still owns modal placement/event-loop adapters, so live smoke
  remains the acceptance proof for visual parity under the real surfaces.

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

- `DebugInfo { run_id, diagnostics_log_path }` — field docs at
  `container_info.rs:110-113` (`run_id` is documented bare;
  `diagnostics_log_path` is the absolute JSONL path)
- `debug_run_info_state` — `crates/jackin-console/src/tui/components/container_info.rs:11`
- `run.run_id()`
- `run.path()`
- `diagnostics.run_id`
- `diagnostics.run_log_display`

Evidence (verified):

- `DebugInfo` comments explicitly state `run_id` is bare and
  `diagnostics_log_path` is absolute path (`container_info.rs:110-113`).
- `DebugInfo::into_state()` renders `Run ID` from `run_id` and
  `Diagnostics log` from `diagnostics_log_path`, marking both copyable and
  hyperlinking only the diagnostics path.
- `cargo test -p jackin-tui container_info --locked` exits 0 (9 passed),
  including `debug_info_keeps_run_id_bare_and_diagnostics_path_separate`.
- `cargo test -p jackin-console container_info --locked` exits 0 (1 passed),
  proving the console builder keeps bare run id and diagnostics path separate.
- `cargo test -p jackin-launch container_info --locked` exits 0 (4 passed),
  including `launch_container_info_keeps_run_id_bare_and_log_path_separate`.
- `cargo test -p jackin-capsule container_info --locked` exits 0 (20 passed),
  including `container_info_state_keeps_run_id_bare_and_log_path_separate`.
- `rg -n 'Run ID.*jsonl|Run ID.*diagnostics|render_container_info_on_blank|blank_render_clears_full_background' crates`
  finds only negative-test assertion messages for the Run ID/path bug, and no
  production/test helper named `render_container_info_on_blank`.

Suspected root cause:

- Fixed at the code/test level: current console, launch, and capsule builders
  route bare run ids and diagnostics paths through separate shared fields. The
  item remains open until live Debug info smoke confirms the on-screen values in
  a fresh real run.

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

- Current focused tests show capsule Debug info preserves both status-bar rows,
  launch Debug info preserves the status footer, and console Debug info renders
  through the reserved-footer modal model. The item remains open because the
  full status-preserving dialog inventory still needs live smoke and convergence
  sweep evidence.

Relevant files:

- `crates/jackin-capsule/src/tui/view.rs`
- `crates/jackin-launch/src/tui/view.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin/src/console/tui/components/modal.rs`
- `crates/jackin/src/console/tui/view/frame.rs`
- `crates/jackin/src/console/tui/layout/prepare.rs`

Starting anchors:

- `dialog_backdrop` — `crates/jackin-launch/src/tui/components/dialog.rs:13`
- `render_launch_frame`
- `render_capsule_ratatui_frame` — `crates/jackin-capsule/src/tui/view.rs:234-239`
- `prepare_visible_modal` — `crates/jackin/src/console/tui/layout/prepare.rs:43`
  (the console-side model to generalize: subtracts footer height before centering)
- `modal_outer_rect` — `crates/jackin/src/console/tui/components/modal_layout.rs:16`
- `footer_hint_spans` — capsule-local at
  `crates/jackin-capsule/src/tui/components/dialog.rs:1317`

Evidence:

- `docs/content/docs/reference/tui/dialogs.mdx` says status/hint rows are
  inviolable and modal backdrop must not cover footer (verbatim quotes under
  "Binding Design Rules").
- `docs/content/docs/reference/tui/navigation.mdx` says hints are footer-only,
  never internal dialog lines.
- Capsule: `render_capsule_ratatui_frame()` renders `StatusBarWidget` before
  the dialog branch, and its `DialogBackdrop` area starts at
  `STATUS_BAR_ROWS`, preserving the two-row top status bar.
- Capsule: `render_capsule_dialog_bottom_chrome()` owns dialog hints and can
  preserve the branch/context bottom bar; `dialog_bottom_chrome_nonblank_background_keeps_context_bar`
  proves the nonblank path keeps branch + instance context.
- Launch: `render_launch_container_info()` renders the shared Debug info dialog
  and shared `render_debug_info_hint()` inside the supplied area, while
  `launch_debug_info_keeps_status_footer_visible` proves the status footer stays
  visible.
- Console: `render_modal()` renders `Modal::ContainerInfo` through
  `jackin_tui::components::render_container_info()` and the console frame owns
  footer hints, matching the reserved-footer model.
- `cargo test -p jackin-capsule view --locked` exits 0 (9 passed).
- `cargo test -p jackin-launch launch_debug_info_keeps_status_footer_visible --locked`
  exits 0 (1 passed).
- `rg -n "render_container_info_on_blank|blank_render_clears_full_background|frame\\.render_widget\\(DialogBackdrop, frame\\.area\\(\\)\\)" crates/jackin-capsule/src crates/jackin-tui/src/components crates/jackin-launch/src/tui crates/jackin/src/console/tui -g '*.rs' -g '!**/tests.rs'`
  exits 1 with no production hits.

Suspected root cause:

- Fixed at the focused-test level for Debug info and build-log/status-footer
  cases, with remaining risk in the broader dialog inventory and live surfaces.
  Keep this open until the convergence sweep and final smoke prove every
  status-preserving dialog path.

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

Orientation note (verified): the string `context7` does not appear anywhere in
the source tree — the prompt the operator saw is an *instance* of the generic
launch text prompt (`draw_text_prompt`,
`crates/jackin-launch/src/tui/components/prompts.rs:46`, driven from
`crates/jackin-launch/src/tui/run.rs:457`) whose label/content comes from role
or MCP configuration data. Fixing the shared prompt shell fixes the Context7
instance; do not hunt for Context7-specific rendering code.

Repro steps:

1. Trigger MCP/Context7 API key setup prompt (use a role whose manifest
   configures an MCP server that needs an API key).
2. Compare it with Debug info, capsule rename prompt, capsule picker, and console
   modal shapes.

Expected behavior:

- All dialogs use the shared dialog system.
- Surface-specific code supplies content/state; shared code supplies shell,
  spacing, backdrop, footer/hints, scroll, and input affordances.

Actual behavior:

- Launch prompt uses shared `render_text_input()` and shared
  `text_input_prompt_rect()` for the one-label prompt box, but still owns
  launch backdrop/footer geometry.
- Capsule previously had a local `render_text_input_dialog()` in
  `dialog_widgets.rs`; implementation evidence now shows this path routed
  through the shared `jackin-tui` labeled text-input helper.
- Capsule filter/info dialogs build their own panels and hint behavior.

Relevant files:

- `crates/jackin-launch/src/tui/components/prompts.rs`
- `crates/jackin-launch/src/tui/components/dialog.rs`
- `crates/jackin-tui/src/components/text_input.rs`
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs`
- `crates/jackin-tui/src/components/dialog_layout.rs`
- `crates/jackin-tui/src/components/panel.rs`

Starting anchors:

- `draw_text_prompt` — `prompts.rs:46`
- `text_prompt_rect` — `prompts.rs:93`
- `dialog_backdrop` — `crates/jackin-launch/src/tui/components/dialog.rs:13`
- `render_text_input` — `crates/jackin-tui/src/components/text_input.rs:385`
- `text_input_prompt_rect` — `crates/jackin-tui/src/components/text_input.rs`
- `render_labeled_text_input_dialog` —
  `crates/jackin-tui/src/components/text_input.rs`
- `DialogRatatuiSnapshot::TextInputDialog`

Evidence (verified):

- `draw_text_prompt()` calls launch-local `dialog_backdrop()` (`dialog.rs:13`)
  but uses shared `text_input_prompt_rect()` instead of a local text prompt
  rectangle.
- `cargo test -p jackin-tui text_input_prompt_rect --locked` proves the shared
  launch prompt rectangle keeps the current 60%-wide, 5-row prompt shape.
- Capsule `DialogRatatuiSnapshot::TextInputDialog` now routes through
  `jackin_tui::components::render_labeled_text_input_dialog()` instead of a
  private renderer.
- `cargo test -p jackin-tui labeled_text_input_dialog --locked` proves the
  shared shell renders the title, label, value, and cursor styling.
- `cargo test -p jackin-capsule rename_tab --locked` proves the capsule rename
  path still drives the shared renderer-backed dialog state.
- The unused raw-ANSI `render_text_input_dialog()` duplicate and
  `TextInputDialogRect` re-export were deleted from `jackin-tui`; the remaining
  source sweep finds only the shared `render_text_input` and
  `render_labeled_text_input_dialog` functions.
- `cargo test -p jackin-tui text_input --locked` passes 2 tests after that
  removal.
- `cargo test -p jackin-launch text_prompt --locked` passes 2 tests after that
  removal.
- The required dialog-variant audit is recorded above in the implementation
  evidence. It covers root console `Modal`, settings/global-mount modal
  families, launch build/failure/prompt/Debug-info overlays, and capsule
  `DialogRatatuiSnapshot` variants. Remaining surface-local renderers are
  classified as content/input adapters over shared primitives, not duplicate
  Debug-info or text-input shells.

Suspected root cause:

- Shared low-level widgets existed, but dialog shell/layout was fragmented. The
  capsule text-input duplicate and the unused raw-ANSI duplicate have been
  removed; launch prompt geometry/backdrop still needs final smoke comparison
  before closing F8.

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
- The evidence run recorded 2,700 `cockpit-dialog-mouse` events with
  `container_info_open=true` (JSONL lines 72–74 onward) — the dialog absorbs a
  mouse-move flood, and on the capsule each hover change composes a full dialog
  overlay redraw.

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

- Current implementation has shared copy, hover-row styling, copy affordance,
  both-axis scroll placement, and hyperlink overlay primitives in
  `jackin-tui`; surface code still owns state storage and event routing.
- Capsule hover redraw now uses an overlay frame without screen erase in focused
  tests; live smoke still needs to prove the interaction is stable under real
  mouse-move volume.

Relevant files:

- `crates/jackin-tui/src/components/container_info.rs`
- `crates/jackin-launch/src/tui/subscriptions.rs`
- `crates/jackin/src/console/tui/input/mouse.rs`
- `crates/jackin/src/console/effects.rs`
- `crates/jackin-capsule/src/daemon/mouse_input.rs`
- `crates/jackin-capsule/src/daemon/input_dispatch.rs`
- `crates/jackin-capsule/src/tui/components/dialog.rs`

Starting anchors:

- `ContainerInfoState` — `container_info.rs:154-164`
- `ContainerInfoRow::copyable` — builder at `container_info.rs:54`,
  `is_copyable` at line 76
- `mark_copied` — `container_info.rs:289`
- `set_hovered_row` — `container_info.rs:301`
- `copy_payload_at` — `container_info.rs:421`
- `value_placements` — `container_info.rs:481`
- `hyperlink_overlay` — `container_info.rs:439`
- `debug_info_hint_spans` — `container_info.rs:335`

Evidence (verified):

- `ContainerInfoState` stores `DialogBodyScroll`, copied row, and hovered row
  (`container_info.rs:154-164`).
- `copy_payload_at()` and `value_placements()` account for both scroll axes.
- `hyperlink_overlay()` accounts for the visible horizontally scrolled slice.
- `cargo test -p jackin-tui container_info --locked` exits 0 (12 passed),
  including:
  - `debug_info_puts_run_id_first_when_available`
  - `keyboard_copy_payload_uses_first_copyable_row`
  - `enter_does_not_dismiss_container_info_state`
  - `copyable_rows_render_explicit_copy_affordance`
  - `long_value_shows_horizontal_scrollbar_and_scroll_reveals_tail`
  - `copy_payload_at_follows_horizontal_and_vertical_scroll`
  - `hyperlink_overlay_follows_horizontal_and_vertical_scroll`
- `cargo test -p jackin-capsule container_info --locked` exits 0 (20 passed),
  including copy feedback, horizontal wheel scroll, unsupported-axis no-op, and
  row semantics.
- `cargo test -p jackin-launch container_info --locked` exits 0 (4 passed),
  including copy/open/close overlay behavior and status-footer preservation.
- `cargo test -p jackin-console container_info --locked` exits 0 (1 passed),
  proving the console state uses the shared copyable/hyperlinked row model.
- `cargo test -p jackin-console footer_hints --locked` exits 0 (19 passed),
  proving the Debug info footer advertises the shared keyboard-copy action
  separately from dismiss.
- `cargo test -p jackin container_info_enter_copies_default_value_without_dismissing --locked`
  exits 0, proving the console key handler emits
  `ManagerEffect::CopyContainerInfoValue` from the shared default target and
  keeps Debug info open.
- `cargo clippy -p jackin-tui --all-targets --all-features --locked -- -D warnings`
  exits 0 after the shared both-axis tests.
- Console, launch, and capsule each still own their event-loop routing/state
  adapters, but the default keyboard copy, copy hit-test, hyperlink, and
  rendering logic are shared.

Suspected root cause:

- Fixed at the shared component/test level for Run-ID-first ordering, copy
  affordances, default keyboard copy, hover styling, copy hit-tests, and
  hyperlink overlays under both axes. Remaining live risk is per-surface event
  routing under real mouse volume, so the item stays open until smoke evidence
  confirms it.

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

1. Start a capsule session with enough pane content to overflow vertically
   (rebuild + export `JACKIN_CAPSULE_BIN` first).
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
  style confirmation in a transient overlay/toast near the top-right of the
  visible surface.
- Copy-success feedback must never use the hint bar, footer row, status bar, or
  any other bottom-chrome row. The hint bar is only for actions currently
  available in the focused surface; a successful copy is state feedback, so it
  belongs in the transient toast layer.
- The copied toast is anchored to the screen/content overlay's top-right corner,
  not to the selected text and not inside the scrollable pane. It may overlap
  only unused/background cells; it must not cover the retained selection when
  there is room to avoid it.
- The toast must be non-blocking and deterministic: it does not steal focus, it
  does not clear or cover the persisted selection, it does not rewrite or append
  hint-bar text, and it disappears automatically after the configured short
  lifetime.
- Clicking unrelated content/chrome or starting to type clears the persisted
  selection.
- Starting a new selection replaces the previous selection.
- Dragging beyond the top/bottom edge auto-scrolls the scrollable pane and
  extends the selection in the drag direction.
- Auto-scroll stops at content bounds and does not flicker or full-redraw the
  whole screen unnecessarily.

Actual behavior:

- Code/test progress has converged on the expected behavior: capsule selection
  is stored in content coordinates, remains highlighted after mouse-up copy,
  shows a shared `Selection copied` toast outside the bottom chrome, clears on
  later click/type, treats plain pane clicks as focus/click gestures rather than
  selection gestures, and edge-drags scroll the pane through the same scrollback
  bounds used by wheel scrolling.
- This finding remains open until a real capsule smoke run id confirms the
  behavior in a live scrollable pane.

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
- `Session::scroll_by` — `crates/jackin-capsule/src/session.rs:673`
- `scrollback_filled` — `crates/jackin-capsule/src/session.rs:705`

Evidence:

- Operator observed correct clipboard copying but no persistent selection or
  visible copied feedback.
- Operator observed that drag selection does not auto-scroll when selecting past
  the visible viewport.
- `SelectionState` in `crates/jackin-capsule/src/tui/selection.rs` stores
  `anchor_row`/`end_row` as absolute content rows: retained scrollback
  oldest-first, followed by the live screen rows. `visible_selection()` projects
  that stored range back into the current viewport, so the highlight survives
  scrollback movement.
- `finalize_selection()` in
  `crates/jackin-capsule/src/daemon/mouse_input.rs` leaves dragged selections in
  `self.selection`, emits OSC 52 for non-empty selected text, sets
  `selection_copied`, and schedules `selection_copy_feedback_deadline`.
- `selection_motion()` scrolls above/below-pane drags with
  `Session::scroll_by()` before updating the content-coordinate end cell,
  keeping selection auto-scroll bounded by the same scrollback model as wheel
  scrolling.
- `render_capsule_ratatui_frame()` renders the shared
  `jackin_tui::components::Toast::new("Selection copied")` in the content area
  below the top status rows and above the bottom chrome when
  `selection_copied` is active.
- `apply_action()` in `crates/jackin-capsule/src/daemon/input_dispatch.rs`
  clears persisted selection/copy feedback on later click and typing before
  forwarding typed pane data. It now stores a press-time `pending_selection`
  anchor and promotes it only after button motion leaves the anchor cell, so a
  normal click does not flash selection chrome or arm clipboard copy.
- `cargo test -p jackin-capsule selection --locked` exits 0 (25 passed),
  covering:
  - `apply_action_pane_primary_press_only_arms_selection_for_shell`;
  - `pane_button_motion_promotes_pending_selection`;
  - `mouse_release_without_drag_clears_pending_selection`;
  - `finalize_selection_keeps_highlight_and_shows_copied_toast`;
  - `selection_copy_feedback_expires_without_clearing_highlight`;
  - `click_after_copied_selection_clears_highlight`;
  - `typed_input_after_copied_selection_clears_and_forwards`;
  - `selection_motion_above_pane_scrolls_into_history`;
  - `selection_motion_below_pane_scrolls_toward_live_tail`;
  - `selection_copy_toast_keeps_status_and_bottom_chrome_rows_free`;
  - `scrolled_inline_history_preserves_color_and_selection_highlight`;
  - the content-coordinate projection tests in `tui::selection::tests`.

Suspected root cause:

- Original root cause fixed at the code/test level: selection is no longer only
  an active drag overlay, copy feedback is connected to the shared toast layer,
  and drag selection owns a bounded edge-scroll path. Remaining risk is live
  terminal behavior under a real capsule session.

Blocks checklist:

- Blocks Defect 54 live smoke polish.
- Blocks expected behavior for scrollable pane selection.

Acceptance:

- Selection supports copy only; it never edits, deletes, cuts, replaces, or
  pastes pane content.
- Plain pane clicks do not create an active selection, do not trigger a copied
  toast, and do not write to the clipboard. Selection starts only after drag
  motion leaves the press cell.
- Selection range is stored in content coordinates with anchor/focus semantics.
- Mouse-up copies selected text and leaves selection visibly highlighted.
- A visible copied confirmation appears in a transient overlay/toast outside the
  hint/footer row.
- The copied confirmation uses the shared toast/popup overlay in the screen's
  top-right corner. It must never occupy the hint bar, footer row, or status
  bar, because those rows are only for currently available actions and persistent
  screen state.
- While the copied toast is visible, the hint bar content remains exactly the
  same as it was before mouse-up; no `copied`, `selected`, or clipboard status
  text is appended there.
- The toast appears when mouse-up copies a selection, remains long enough to be
  read, and then expires without requiring input.
- Clicking unrelated content/chrome clears the selection.
- Typing clears the selection before forwarding the key to the pane.
- Starting a new selection replaces the old selection.
- Wheel scrolling after selection keeps the same content range selected when it
  remains visible.
- Dragging beyond top/bottom viewport edges auto-scrolls and extends selection.
- Auto-scroll is bounded, stable, and does not send drag-scroll input to the PTY.

Close when:

- Tests cover mouse-up copy with persisted selection. **Code/test evidence
  recorded 2026-06-08:** `finalize_selection_keeps_highlight_and_shows_copied_toast`.
- Tests cover clear-on-click, clear-on-type, and replace-on-new-selection.
  **Code/test evidence recorded 2026-06-08:**
  `click_after_copied_selection_clears_highlight`,
  `typed_input_after_copied_selection_clears_and_forwards`, and
  `apply_action_start_selection_sets_selection_state`.
- Tests cover plain click vs drag-select behavior. **Code/test evidence
  recorded 2026-06-08:** `cargo test -p jackin-capsule selection --locked`
  exits 0 (25 passed), including
  `apply_action_pane_primary_press_only_arms_selection_for_shell`,
  `pane_button_motion_promotes_pending_selection`, and
  `mouse_release_without_drag_clears_pending_selection`.
- Tests cover selection rendering after scroll offset changes. **Code/test
  evidence recorded 2026-06-08:**
  `scrolled_inline_history_preserves_color_and_selection_highlight` plus
  `tui::selection::tests::visible_selection_projects_content_rows_into_viewport`.
- Tests assert the copy-success toast renders in the overlay layer and that the
  hint/footer/status rows remain unchanged while the toast is visible.
  **Code/test evidence recorded 2026-06-08:**
  `selection_copy_toast_keeps_status_and_bottom_chrome_rows_free`.
- Tests cover upward and downward auto-scroll while drag-selecting beyond the
  pane viewport. **Code/test evidence recorded 2026-06-08:**
  `selection_motion_above_pane_scrolls_into_history` and
  `selection_motion_below_pane_scrolls_toward_live_tail`.
- Live smoke confirms persistent highlight, copied feedback, deselect behavior,
  and edge auto-scroll in a real scrollable capsule pane.

## Existing Tests To Extend Or Expect To Break

Inventory of the tests already guarding this territory. Extend these rather
than writing parallel siblings, and expect the flagged ones to need updating
when the contract changes:

| Test | Location | Role in this goal |
| --- | --- | --- |
| `renders_rows_with_title_and_link_style` | `crates/jackin-tui/src/components/container_info/tests.rs:12` | Baseline shared-render assertion; extend for shell contract. |
| `copy_payload_at_hits_copyable_value_column` + `copy_payload_at_follows_horizontal_and_vertical_scroll` | `container_info/tests.rs` | Covers copy payload hit-testing at rest and after both horizontal + vertical scroll. |
| `hyperlink_overlay_emits_osc8_for_link_rows` + `hyperlink_overlay_follows_horizontal_and_vertical_scroll` | `container_info/tests.rs` | Covers OSC 8 hyperlink emission and visible-slice placement after both scroll axes. |
| `long_value_shows_horizontal_scrollbar_and_scroll_reveals_tail` | `container_info/tests.rs` | Covers h-overflow and tail reveal after horizontal scroll. |
| `short_content_shows_no_horizontal_scrollbar` | `container_info/tests.rs:119` | Negative case; keep. |
| `render_container_info_on_blank` stale helper | retired | The old full-screen Debug info helper was removed because it could cover status/footer chrome. Shared Debug info callers render `render_container_info()` inside a content-owned overlay area instead. |
| `wheel_redraw_reason_uses_visible_update_vocabulary` | `crates/jackin-capsule/src/tui/update/tests.rs:156` | Extend for no-op suppression (Finding 3). |
| `pane_data_redraw_reason_prioritizes_scrollback_snap` | `crates/jackin-capsule/src/tui/update/tests.rs:100` | Related redraw-reason coverage. |
| `scrollbar_hit_maps_track_to_top_offset` | `crates/jackin-launch/src/tui/components/build_log_dialog.rs:357` | Build-log scroll hit-testing; keep green through Finding 1. |
| `build_log_overlay_keeps_status_footer_in_debug_mode` | `build_log_dialog.rs:405` | **Currently passes against the broken layout** — extend to assert the spacer row (Finding 1). |

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
- Keep `render_container_info_on_blank()` retired; Debug info must render via
  `render_container_info()` inside a content-owned overlay area so it cannot
  cover reserved status/footer chrome.
- Make launch and capsule compute modal rects against the content area, not the
  full terminal. The console-side model to generalize is
  `prepare_visible_modal` (`crates/jackin/src/console/tui/layout/prepare.rs:43`).
- Keep only surface-specific data assembly and state storage outside
  `jackin-tui`.

### Phase 3 - Footer And Hint Unification

- Move Debug info hints into the reserved footer/status area on all surfaces.
- Ensure hints use `HintSpan` (`crates/jackin-tui/src/geometry.rs:32`) and the
  shared hint renderer (`render_hint_bar`, `hint_bar.rs:78`).
- Add copy hints that clearly identify copyable values.
- Render a shared explicit copy affordance for each copyable Debug info row.
- Ensure scroll hints are generated by `scroll_hint_spans()`
  (`dialog_layout.rs:284`) from actual overflow axes.
- Fix build-log overlay spacing through the same footer/hint layout primitive
  (`render_status_footer`, `status_footer.rs:163`).
- Fold the duplicate footer renderers
  (`crates/jackin-console/src/tui/view.rs:71`,
  `crates/jackin-launch/src/tui/components/footer.rs:27`) and the capsule-local
  `footer_hint_spans` (`dialog.rs:1317`) into or onto the shared stack where
  practical; document any that must stay local.

### Phase 4 - Dialog System Reuse Beyond Debug Info

- Audit launch prompts, Context7 API key prompt (generic `draw_text_prompt`
  instance — see Finding 8 orientation note), capsule text input, capsule
  pickers, console modals, and info dialogs.
- Replace duplicate local shell/backdrop/text-input renderers with shared
  `jackin-tui` helpers where practical.
- If a renderer must remain surface-local, document the structural reason and
  keep row spacing, footer/hints, and status preservation identical.

### Phase 5 - Capsule Scrollbar And Flicker Fix

- [x] Change pane scrollbar visibility from `offset > 0` to real scrollability
  (`apply_pane_scrollbar`, `view.rs:322-324`). Evidence:
  `crates/jackin-capsule/src/tui/view.rs` gates the thumb on `filled > 0`, and
  `cargo test -p jackin-capsule retained_scrollback_draws_scrollbar_at_live_tail --locked`
  exits 0 (1 passed).
- [x] Route pane scrollbar math through shared scroll helpers
  (`crates/jackin-tui/src/scroll.rs`) or a documented shared adapter. Evidence:
  `apply_pane_scrollbar()` calls
  `jackin_tui::scroll::tail_vertical_thumb(interior_rows, filled, offset)`.
- [x] In wheel dispatch, compare scrollback offset before/after and skip redraw
  when unchanged. Evidence:
  `cargo test -p jackin-capsule apply_action_wheel --locked` exits 0 (2
  passed), including `apply_action_wheel_noops_at_scrollback_boundary`.
- [x] Prefer partial/redraw-minimal frame composition for scrollback movement
  where technically possible. Evidence: `apply_action_wheel_scrolls_scrollback`
  proves offset-changing wheel scroll moves scrollback through the non-full
  frame path, and the sweep
  `rg -n "compose_full_redraw\\([^\\n]*(wheel_scrollback|ScrollbackMovement)|wheel_scrollback_redraw_reason\\(" crates/jackin-capsule/src/daemon crates/jackin-capsule/src/tui crates/jackin-capsule/tests`
  finds only the vocabulary helper and vocabulary test.
- [x] Eliminate or explain the dual bottom-chrome draw path (`site=ratatui` at
  `view.rs:267` plus `site=raw-full` at `view.rs:41`; `site=dialog` at
  `view.rs:104`) so chrome does not flicker. Evidence: the raw bottom chrome is
  now documented as a structural attach-tail adapter in
  `crates/jackin-capsule/src/daemon/compositor.rs`, and
  `cargo test -p jackin-capsule unchanged_diff_frame_suppresses_cached_raw_bottom_chrome --locked`
  exits 0 (1 passed), proving unchanged diff/status frames do not re-append the
  cached raw hint row while real scrollback-state changes re-emit the alternate
  hint.
- [x] Live capsule smoke proves the pane scrollbar is visible at tail and while
  scrolled back, and that saturated wheel scrolling does not flicker or full
  redraw in the real multiplexer logs. Evidence: operator-verified run
  `jk-run-aa0e87`, with current-head capsule log at
  `/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/jk-zr6f77yy-thearchitect/state/multiplexer.log`.
  Saturated wheel no-op evidence appears at lines 13496-13646 (`moved=false`),
  and the bad full-redraw grep for scrollback/status/selection/dialog clear-tier
  reasons returned no hits.

### Phase 6 - Scrollable Pane Selection

- [x] Introduce or extend a pane selection model that stores anchor/focus in
  content coordinates. Evidence:
  `crates/jackin-capsule/src/tui/selection.rs` and
  `cargo test -p jackin-capsule selection --locked` (25 passed).
- [x] Keep selection visible after mouse-up and copy. Evidence:
  `finalize_selection_keeps_highlight_and_shows_copied_toast`.
- [x] Add a copied feedback path through the shared transient toast overlay, not
  the hint/footer row. Evidence:
  `selection_copy_toast_keeps_status_and_bottom_chrome_rows_free`.
- [x] Clear selection on explicit deselect, typing, pane close/clear, or new
  selection. Evidence: `click_after_copied_selection_clears_highlight`,
  `typed_input_after_copied_selection_clears_and_forwards`, and the session
  lifecycle clear paths.
- [x] Add edge auto-scroll during active drag selection. Evidence:
  `selection_motion_above_pane_scrolls_into_history` and
  `selection_motion_below_pane_scrolls_toward_live_tail`.
- [x] Ensure selection auto-scroll and scrollback wheel use the same scroll
  bounds (`Session::scroll_by` / `scrollback_filled`,
  `crates/jackin-capsule/src/session.rs:673/705`). Evidence:
  `selection_motion()` calls `Session::scroll_by()` before recalculating the
  content-coordinate endpoint; the focused selection test suite above exits 0.
- [x] Live capsule smoke proves persistent highlight, copied toast, clear
  behavior, and edge auto-scroll in a real scrollable pane with a captured run
  id. Evidence: operator-verified run `jk-run-aa0e87`; the same capsule log is
  tied to current head at line 7 and to the run JSONL by `container_started` line
  3689.

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

Focused tests per finding first (fast inner loop):

```sh
cargo nextest run -p jackin-tui -E 'test(/container_info/)'
cargo nextest run -p jackin-capsule -E 'test(/update::tests/)'
cargo nextest run -p jackin-launch -E 'test(/build_log/)'
```

Then broad gates:

```sh
cargo fmt --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked -E 'not binary(dind_e2e)'
```

(The `-E 'not binary(dind_e2e)'` filter is load-bearing locally: `dind_e2e` is
gated behind the `e2e` feature, which `--all-features` turns on, and the suite
needs Docker plus an exported `JACKIN_CAPSULE_BIN`. CI runs it with the capsule
binary prebuilt; locally run it deliberately, not by accident.)

Run docs gates if TUI docs are edited:

```sh
cd docs
bun run build
bun run check:repo-links
bunx tsc --noEmit
bun test
```

Run a real operator smoke after rebuilding (capsule binary freshly exported —
see "Capsule binary resolution trap"):

- Capsule smoke build and real multi-pane `--debug` session.
- Debug info on console, launch, and capsule.
- Horizontal scroll in Debug info with a long diagnostics path.
- Hover and copy Container ID, Run ID, and Diagnostics log.
- Build log overlay footer spacing.
- Pane scrollbar visible at live tail and while scrolled back.
- Wheel scroll no-op does not flicker or full redraw.
- Pane text selection remains visible after copy.
- Pane copied feedback appears as a transient overlay/toast outside the
  hint/footer row.
- Clicking elsewhere and typing clear the persisted selection.
- Drag-selecting past the top/bottom edge auto-scrolls and extends selection.

Capture run ids and relevant log lines before marking any Defect 54 checklist
item complete. Use the template below.

### Live `jackin-term` metric extraction from `jk-run-aa0e87`

This is useful live-capsule evidence, but it does **not** close the
`jackin-term` performance rows. The run was the Defect 64 visual re-smoke, not
the dedicated performance acceptance run; it gives present-frame and
bytes-on-wire samples from a real capsule log, but it does not prove
focused-path DHAT allocation in the live capsule session and does not exercise
the 16-32 pane RSS/CPU scale check.

- Run id: `jk-run-aa0e87`.
- Capsule log:
  `/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/jk-zr6f77yy-thearchitect/state/multiplexer.log`.
- Direct-grid-patch extraction output:

```text
direct_grid_patch_frames=399
p50_duration_us=44
p95_duration_us=141
p99_duration_us=267
max_duration_us=285
total_bytes_out=2535436
total_changed_rows=5155
total_changed_cells=878204
bytes_per_changed_cell=2.887
```

- All partial PTY-frame extraction output:

```text
all_partial_pty_frames=460
p50_duration_us=54
p95_duration_us=917
p99_duration_us=3998
max_duration_us=4717
total_partial_bytes=2626199
```

- PTY parse extraction output:

```text
feed_pty_events=1073
p50_parse_us=16
p95_parse_us=50
p99_parse_us=96
max_parse_us=1621
total_pty_input_bytes=1777040
```

Keep the `jackin-term` live performance ledger row open until a dedicated run
adds the missing live focused-path allocation proof and 16-32 pane RSS/CPU
numbers.

#### Remaining `jackin-term` performance close-out recipe

Use this only for the remaining performance row; do not use it to re-close
Defect 64. The current code has DHAT allocation regression tests and
real-capsule render/byte counters. It now also has an opt-in live DHAT capsule
counter: build the capsule with `--features dhat-heap` and launch with
`JACKIN_DHAT_ALLOC_LOG=1`; direct focused dirty-patch frames will emit
`render_alloc` lines in `multiplexer.log`. Do not mark the row `[x]` until a
real run id captures zero `alloc_blocks` / `alloc_bytes` for the focused
direct-grid-patch frames plus the RSS/CPU and byte-minimum evidence below.

1. Build and launch a fresh debug smoke session with the local capsule:

```bash
set -euo pipefail

export JACKIN_SMOKE_ROLE="${JACKIN_SMOKE_ROLE:-the-architect}"
export JACKIN_SMOKE_WORKDIR="${JACKIN_SMOKE_WORKDIR:-$PWD}"
eval "$(cargo run --bin build-jackin-capsule -- --features dhat-heap --export)"
export JACKIN_DHAT_ALLOC_LOG=1

cargo run --bin jackin -- --debug load "$JACKIN_SMOKE_ROLE" "$JACKIN_SMOKE_WORKDIR" --agent claude

RUN_JSONL="$(ls -t "${JACKIN_HOME_DIR:-$HOME/.jackin}/data/diagnostics/runs/"*.jsonl | head -n 1)"
RUN_ID="$(basename "$RUN_JSONL" .jsonl)"
CAPSULE_LOG="$(rg -o '"capsule_log":"[^"]+"' "$RUN_JSONL" | tail -n 1 | cut -d'"' -f4)"
CONTAINER_NAME="$(rg -o '"container_name":"[^"]+"' "$RUN_JSONL" | tail -n 1 | cut -d'"' -f4)"
printf 'run_id=%s\nrun_jsonl=%s\ncapsule_log=%s\ncontainer_name=%s\n' \
  "$RUN_ID" "$RUN_JSONL" "$CAPSULE_LOG" "$CONTAINER_NAME"
```

2. Confirm the DHAT feature build is active and extract focused-path allocation
   deltas from the capsule log. The first command must show the startup marker;
   the second command must report `max_alloc_blocks=0` and `max_alloc_bytes=0`
   before the allocation part of the performance row can close.

```bash
rg -n 'dhat allocation telemetry enabled|render_alloc: kind=partial reason=pty-output via=direct-grid-patch' "$CAPSULE_LOG"

ruby -ne 'if $_ =~ /render_alloc: kind=partial reason=pty-output via=direct-grid-patch alloc_blocks=(\d+) alloc_bytes=(\d+)/ then $n=($n||0)+1; $blocks=[($blocks||0),$1.to_i].max; $bytes=[($bytes||0),$2.to_i].max end; END { if $n && $n > 0 then printf("render_alloc_frames=%d\nmax_alloc_blocks=%d\nmax_alloc_bytes=%d\n", $n, $blocks, $bytes) else puts "no render_alloc frames" end }' "$CAPSULE_LOG"
```

3. During the live session, create 16-32 panes/tabs with active output, then
   sample RSS/CPU from Docker. Paste the exact run id and command output into
   the checklist before marking the row done.

```bash
docker stats --no-stream "$CONTAINER_NAME"
docker inspect -f '{{.State.Pid}} {{.State.Status}} {{.State.ExitCode}}' "$CONTAINER_NAME"
```

4. Extract the live render/byte counters and compare bytes-on-wire against the
   theoretical minimum for the captured cell deltas. The helper below reports
   the measured side only; the acceptance note still has to state the
   theoretical-minimum calculation and whether the result is within the
   roadmap's ~15% target.

```bash
ruby -ne 'if $_ =~ /render: kind=partial reason=pty-output.*via=direct-grid-patch bytes=(\d+) duration_us=(\d+) changed_rows=(\d+) changed_cells=(\d+)/ then ($d ||= []) << $2.to_i; $bytes = ($bytes || 0) + $1.to_i; $rows = ($rows || 0) + $3.to_i; $cells = ($cells || 0) + $4.to_i; $n = ($n || 0) + 1 end; END { if $n && $n > 0 then s=$d.sort; idx=[($n*0.99).floor,$n-1].min; printf("direct_grid_patch_frames=%d\np99_duration_us=%d\nmax_duration_us=%d\ntotal_bytes_out=%d\ntotal_changed_rows=%d\ntotal_changed_cells=%d\nbytes_per_changed_cell=%.3f\n", $n, s[idx], s[-1], $bytes, $rows, $cells, ($cells && $cells > 0 ? $bytes.to_f/$cells : 0)) else puts "no direct-grid-patch frames" end }' "$CAPSULE_LOG"
```

5. Keep the existing DHAT regression commands as supporting evidence only:

```bash
cargo test -p jackin-term --test allocation --features dhat-heap --locked -- --nocapture
cargo test -p jackin-capsule --test render_allocation --features dhat-heap --locked -- --nocapture
```

### Defect 64 live-smoke close-out commands

The code/test convergence work is complete; the remaining F1-F10 boxes close
only with a real operator `--debug` run id. Use this block after rebuilding the
capsule from the PR worktree:

```bash
set -euo pipefail

export JACKIN_SMOKE_ROLE="${JACKIN_SMOKE_ROLE:-the-architect}"
export JACKIN_SMOKE_WORKDIR="${JACKIN_SMOKE_WORKDIR:-$PWD}"
eval "$(cargo run --bin build-jackin-capsule -- --export)"

latest_jackin_run() {
  ls -t "${JACKIN_HOME_DIR:-$HOME/.jackin}/data/diagnostics/runs/"*.jsonl | head -n 1
}

capture_latest_jackin_run() {
  RUN_JSONL="$(latest_jackin_run)"
  RUN_ID="$(basename "$RUN_JSONL" .jsonl)"
  CAPSULE_LOG="$(rg -o '"capsule_log":"[^"]+"' "$RUN_JSONL" | tail -n 1 | cut -d'"' -f4 || true)"
  printf 'run_id=%s\nrun_jsonl=%s\ncapsule_log=%s\n' "$RUN_ID" "$RUN_JSONL" "$CAPSULE_LOG"
}

cargo run --bin jackin -- --debug load "$JACKIN_SMOKE_ROLE" "$JACKIN_SMOKE_WORKDIR" --agent claude
capture_latest_jackin_run
```

During that live session, exercise all ten findings before exiting:

- F1: open the Docker build-log overlay and confirm the hint row, spacer row,
  and status/footer row keep the standard separation.
- F2/F3: scroll a retained-scrollback pane at live tail and while scrolled back;
  confirm the scrollbar is visible when retained history exists, saturated wheel
  events do not flicker, and offset-changing scrollback does not ghost.
- F4/F5/F6/F9: open Debug info on capsule, launch, and console surfaces; confirm
  one shared shell, `Run ID` first and bare, `Diagnostics log` as a full JSONL
  path, horizontal scroll for long values, hover feedback, explicit copy
  affordances, and copy for Run ID, Container ID, and Diagnostics log.
- F7/F8: open status-preserving dialogs, the MCP API-key text prompt, and the
  File Browser; confirm reserved status/footer rows remain visible and list
  selection/scrollbar behavior uses the shared selected-list chrome.
- F10: drag-select pane text, confirm copied toast appears outside the hint bar,
  the selection remains visible after mouse-up, plain clicks do not select,
  typing/clicking elsewhere clears it, and top/bottom edge drags auto-scroll.

After `capture_latest_jackin_run`, paste these command results into the smoke
run block below before flipping any checkbox:

```bash
rg -n 'container_started|capsule_log|run_log|diagnostics' "$RUN_JSONL"
rg -n 'bottom-chrome|render: kind=(full|diff|partial)|wheel dispatch|scrollback|changed_rows|changed_cells|bytes_out|t_parse_us' "$CAPSULE_LOG"
rg -n 'render: kind=full reason=(scrollback-movement|status-change|selection-repaint|dialog-change)|\\x1b\\[2J' "$CAPSULE_LOG" || true
rg -n 'cockpit-dialog-mouse|container_info_open|build_log_open|copy|copied|hover|dialog' "$RUN_JSONL" "$CAPSULE_LOG"
```

Expected evidence:

- `capsule_log` is non-empty and points to a readable multiplexer/capsule log.
- No saturated wheel event is followed by a clear-tier full redraw.
- `scrollback-movement`, status refresh, selection repaint, and dialog movement
  stay out of the terminal-clearing full-redraw tier except for intentional
  geometry/first-attach clears.
- Debug-info mouse/copy/hover events are visible in JSONL/capsule logs, and the
  copied rows match the visible row order.
- The operator observation line for each F1-F10 item says pass/fail explicitly.

## Evidence Capture Template

One block per smoke run, appended to this file (or the Defect 54 ledger) before
any checklist box flips to `[x]`:

```text
### Smoke run: <date>
- Command: <exact cargo run invocation incl. --debug and env exports>
- Capsule binary: <JACKIN_CAPSULE_BIN path or "release/cache (unchanged)">
- Run id: <jk-run-XXXXXX>
- JSONL: <path>
- Capsule log: <path from container_started detail.capsule_log>
- Findings exercised: <F1..F10 list>
- Key greps + results:
  - <grep command> -> <count / line numbers / excerpt>
- Verdict per finding: <Fn: pass/fail + one-line observation>
```

### Smoke run: 2026-06-08

- Command: `cargo run --bin jackin -- --debug load the-architect "$PWD" --agent claude` (operator PR-scoped environment from the PR template).
- Capsule binary: `/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/cache/jackin-capsule/0.6.0-dev_2187510/linux-arm64/jackin-capsule`.
- Run id: `jk-run-aa0e87`.
- JSONL: `/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/diagnostics/runs/jk-run-aa0e87.jsonl`.
- Capsule log: `/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/jk-zr6f77yy-thearchitect/state/multiplexer.log`.
- Findings exercised: F1-F10, plus the broader Defect 54 capsule smoke path
  for multi-session tabs, split-pane geometry, scrollback movement, tab/pane
  open+close, and the Defect 58 manual resize/ghosting repro.
- Key greps + results:
  - `rg -n 'capsule_binary|container_started' "$RUN_JSONL"` -> JSONL lines 68 and 3689 prove the capsule binary path and the `capsule_log` path.
  - `rg -n 'git-branch-context: lookup loaded|bottom-chrome: site=(raw-full|dialog)|render: kind=(full|diff|partial)|wheel dispatch|t_parse_us|changed_rows|changed_cells' "$CAPSULE_LOG"` -> capsule log lines 7, 12, 23, 26, 28-30, 45, 81, 139, 168, 261, 270, 273, 351, and 13496-13646 prove current branch/head context, shared bottom chrome on raw/dialog sites, partial/diff frame use, parse timing, damage sizes, and saturated wheel no-op handling.
  - `rg -n 'render: kind=full reason=(scrollback-movement|status-change|selection-repaint|dialog-change)|\\x1b\\[2J' "$CAPSULE_LOG" || true` -> no hits; the run does not show the forbidden full-redraw/clear-tier regressions for scrollback movement, status refresh, selection repaint, or dialog movement.
  - `rg -n 'cockpit-dialog-mouse|container_info_open|build_log_open' "$RUN_JSONL"` -> container-info mouse evidence at JSONL line 78 and build-log scroll/dialog evidence at lines 3302-3374.
  - `rg -n 'action: spawn_session|action: remove_exited_session|resize-event|resize:|reason=tab-switch|reason=scrollback-movement|screen=38x86|stopped exit:0|finalize_foreground_session' "$CAPSULE_LOG" "$RUN_JSONL"` -> capsule log lines 18-20 prove attach resize and first Claude session; lines 345/351 prove Codex spawn and tab switch; lines 4615/4621 prove Amp spawn and tab switch; line 8635 proves Opencode/Z.AI spawn; lines 9105 and 10664 prove two Shell sessions; lines 11065-11323 and 17529-17620 prove split-pane geometry (`screen=38x86` inside a `175x45` attach); lines 17121 and 17857 prove scrollback movement used diff frames; lines 17063, 17286, 17495, 17626, 17828, 17964, and 18058 prove session close/removal; JSONL lines 3696-3700 prove the role container stopped with exit `0` and the foreground session finalized cleanly.
- Verdict per finding: F1-F10 pass by operator visual verification; the operator
  reported "everything looks good" for the run and no new visual issue was filed
  against this run. The same operator verification closes the broader Defect 54
  capsule multi-pane smoke and Defect 58 manual resize/ghosting repro for this
  PR head. The run does not prove provider-picker contents, B.5 auth
  source-folder overrides, symbolicated panic frames, detach/re-attach socket
  reuse, or host-console resize, so those remain open.

## Traceability Matrix

| Finding | Phases | Defect / checklist tie-in | Key tests |
| --- | --- | --- | --- |
| F1 build-log spacing | 3 | Defect 54 visual smoke polish | `build_log_overlay_keeps_status_footer_in_debug_mode` (+ spacer assertion) |
| F2 pane scrollbar gating | 5 | Defect 54 resize/scrollback/zero-ghosting | new scrollable-at-tail + scrolled-back tests |
| F3 wheel full-redraw flicker | 5 | Defect 54 zero-ghosting; live perf run-id confidence; adjacent to Defect 58's re-run of the Defect 44 manual repro | `wheel_redraw_reason_uses_visible_update_vocabulary` (+ no-op suppression) |
| F4 capsule status bar hidden | 1, 2 | Defect 54 visual smoke; dialogs.mdx rules | new capsule frame test (dialog + status bar same frame) |
| F5 Debug info variants | 1, 2, 3 | Defect 54 Debug info validation | per-surface row-order/shell tests |
| F6 Run ID semantics | 1 | Debug info trust | builder tests across console/launch/capsule |
| F7 status/footer preservation | 2, 3 | Defect 54 visual smoke | backdrop-excludes-reserved-rows tests |
| F8 dialog system reuse | 4 | TUI architecture epic reusability | text-input shell consistency tests |
| F9 interaction contract | 1, 3 | Debug info smoke acceptance | extended `container_info/tests.rs` suite |
| F10 pane text selection | 6 | Defect 54 live smoke polish | selection persist/clear/auto-scroll tests |

## Glossary

- **Render kinds** — `render: kind=full` (whole-screen recomposition via
  `compose_full_redraw`, `compositor.rs:56`) vs `kind=partial` (dirty-pane
  patches; `via=direct-grid-patch` marks the minimal-grid path from
  `compose_direct_dirty_pane_frame`, `compositor.rs:431/469`).
- **`FullRedrawReason`** — vocabulary for why a full redraw was requested;
  `scrollback-movement` comes from `wheel_scrollback_redraw_reason()`
  (`update.rs:195`, variant at `update.rs:44`).
- **Bottom-chrome sites** — debug tags for which code path drew the bottom
  chrome: `site=ratatui` (`view.rs:267`), `site=raw-full` (`view.rs:41`),
  `site=dialog` (`view.rs:104`). Two sites firing in the same frame family =
  duplicated chrome work.
- **Frame family** — the burst of log lines belonging to one composed frame
  (render line + its chrome lines, same timestamp neighborhood).
- **Scrollback `offset` / `filled`** — `offset` = how far the pane is scrolled
  away from the live tail (0 = at tail); `filled` = how many scrollback rows
  exist (`session.rs:705`). `before=9 filled=9` then `after=9` = saturated
  no-op wheel event.
- **Content coordinates (selection)** — positions anchored to the pane's
  scrollback content, not the visible screen; a selection stored this way
  survives scrolling and redraws (Finding 10 contract).
- **`DialogRatatuiSnapshot`** — capsule-side enum mapping open-dialog state to
  a Ratatui render call (`dialog_widgets.rs:251-254` for DebugInfo).
- **`cockpit-dialog-mouse`** — JSONL debug event recorded for every mouse event
  while a launch-cockpit dialog is open; fields include
  `container_info_open=`/`build_log_open=`.

## External References Used For Selection Rules

- W3C Selection API: <https://www.w3.org/TR/selection-api/>
- W3C Selection API user interactions: <https://www.w3.org/TR/selection-api/#user-interactions>
- MDN Selection API: <https://developer.mozilla.org/en-US/docs/Web/API/Selection_API>
- MDN Selection object: <https://developer.mozilla.org/en-US/docs/Web/API/Selection>
- W3C Pointer Events `mousedown`: <https://www.w3.org/TR/pointerevents/#the-mousedown-event>
- W3C WAI G149 highlighting technique: <https://w3c.github.io/wcag/techniques/general/G149>

## Do Not Mark Done Yet

These stay open until real evidence exists:

- Defect 54 live smoke ledger — `jk-run-aa0e87` closes the capsule multi-pane
  resize/scrollback/zero-ghosting smoke and the Defect 58 manual repro. The
  session-bound command-ledger items still needing captured run ids are provider
  picker, auth source-folder override, symbolicated debug-capsule build,
  clean-exit/re-attach cycle, and host-console resize sweep. The Docker-capable
  `dind_e2e` item is already recorded as `[x]` in the roadmap checklist with
  GitHub Actions evidence, so do not re-open it here without a new failure.
- `jackin-term` live performance acceptance — `jk-run-aa0e87` now provides
  real-capsule present-frame and bytes-on-wire samples, but the acceptance row
  stays open until a dedicated run captures focused-path allocation proof,
  16-32 pane RSS/CPU, and the byte-minimum comparison required by the roadmap.
- Defect 58 — closed by `jk-run-aa0e87`; do not re-open unless a new
  resize/ghosting failure is reported.
- Defect 59 B.5 — source-folder end-to-end smoke (B.3 UI shipped `[x]`; the
  smoke + `auth-sync-source-folder.mdx` update remain).
- Defect 60 — final roadmap sweep (all other items `[x]`; the sweep waits on
  Defects 48–59 closing).
- Defect 63 — deferred license ruling: operator decision pending for every
  temporary non-Apache/MIT exception in `deny.toml`. The authoritative list is
  the "Deferred license decisions" table under Defect 63 in
  `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`;
  it includes the full set of exceptions such as `adler2@2.0.1` (`0BSD`),
  `aho-corasick@1.1.4` (`Unlicense`), `aws-lc-rs@1.17.0` (`ISC`), the ICU
  `Unicode-3.0` stack, WASI `Apache-2.0 WITH LLVM-exception` crates, and the
  remaining BSD/Zlib/ISC/Unlicense/MPL/BSL/CDLA/LGPL/CC0/MIT-0 cases. Do not
  mark Defect 63 done until the operator rules on the full table and `deny.toml`
  reflects that final decision.

### Remaining Evidence Ledger

Use this table as the close-out handoff. A row stays open if the proof column is
missing, even when the implementation and focused tests are already green.

| Open item | Required proof before `[x]` | Paste evidence into | Notes |
| --- | --- | --- | --- |
| Defect 54 provider picker | Real `--debug` console/load run id proving non-Claude provider picker contents and spawned-session provider env/config, including Codex/OpenCode provider configs. | Defect 54 provider-picker row and the provider/AgentRuntime roadmap close-out notes. | Another agent may own picker code; only record evidence here unless explicitly assigned to change picker files. |
| Defect 59 B.5 source-folder smoke | Two real run ids: one launch from the workspace-scoped `sync_source_dir` override and one launch from the default workspace, proving credentials sync from the expected source in each. | Defect 54 auth source-folder row, Defect 59 B.5 row, and `auth-sync-source-folder.mdx` Status only after both run ids exist. | Unit tests and UI screenshots are not enough; B.5 is explicitly end-to-end. |
| Defect 42 symbolicated capsule panic | Debug capsule run id with `JACKIN_CAPSULE_FORCE_PANIC=1` and `RUST_BACKTRACE=full`, JSONL `capsule_log` pointer, the stable forced-panic message, and `multiplexer.log` frames resolved to `crates/jackin-capsule/...` paths. | Defect 54 symbolicated debug-capsule row. | The controlled trigger now exists; the row still stays `[ ]` until the real run id and log-frame evidence are pasted back. |
| Defect 30 clean exit / re-attach | Run id(s) showing launch, detach/reattach with `hardline`, clean role-container exit `0`, socket reclaimed, and second attach or new launch succeeds. | Defect 54 clean-exit / re-attach row. | Do not infer this from a normal one-shot exit unless reattach/socket reuse was actually exercised. |
| Defect 35 host-console resize | Host console `--debug` run id with very-small shrink and re-expand observation: no panic, overlap, or stale debug-chip/footer state. | Defect 54 host-console resize row. | This is a host-console check, not a capsule pane check. |
| `jackin-term` live performance | Dedicated real-capsule proof for focused-path allocation, 16-32 pane RSS/CPU, and the byte-minimum comparison. | Defect 45/52 performance rows and final roadmap sweep. | `jk-run-aa0e87` now supplies real-capsule present-frame/bytes samples; headless run `jk-run-f9a03c` remains useful but does not satisfy the remaining live acceptance criteria. |
| Defect 63 license rulings | Operator decisions for every temporary non-Apache/MIT exception in `deny.toml`, followed by matching `deny.toml` policy updates and `cargo deny check licenses bans sources`. | Defect 63 license row and final report. | The full table in the roadmap checklist is authoritative; do not decide licenses on behalf of the operator. |
| DCO/back-history | Laris/operator back-history lane repairs historical commits, DCO check turns green, and no unrelated history rewrite happens from this lane. | Defect 48 DCO notes and PR status. | New commits here still use `git commit -s`; do not force-push unless coordination explicitly records it. |
