# TUI Interaction Consistency — Goal Execution Plan

This file is a goal-execution plan for standardizing TUI interaction behavior:
selected-row cursors, full-width selected highlighting, scrollbars, keyboard and
touchpad scrolling, File Browser behavior, and footer hints. It is designed to be
driven end-to-end by an agent (e.g. `/goal fix all issues in this markdown file`).

The implementation goal is not a redesign. The goal is to make existing TUI
surfaces obey the published interaction contract, using shared components and
helpers wherever possible.

## How to use this file

- Work items are `W1`–`W10`, grouped into ordered phases. Execute phases in
  order; items inside a phase are independent unless a dependency is named.
- Track progress by checking the `- [ ]` boxes in place as items complete.
- **Mandatory re-verification:** this branch (`feature/tui-architecture`,
  PR #495) is under active development and several findings below were already
  partially fixed while this file was being researched. Before starting any
  work item, re-verify its *Current state* against the working tree (commands
  are given per item). If the gap is already closed, verify the acceptance
  criteria, check the box, and move on. File:line references were verified on
  2026-06-08 and may have drifted.
- This file complements two sibling artifacts; do not duplicate their tracking:
  - `PR-495-TUI-SMOKE-FINDINGS.md` (repo root) — owns findings F1–F10 (Debug
    info dialog contract, capsule scrollback, redraw tiers, pane selection).
    Out of scope here.
  - `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`
    — the roadmap defect checklist. Items below that ship a defect from that
    checklist must update its status markers in the same commit (Roadmap
    freshness rule in `AGENTS.md`).

## Definition of done

All of the following, in addition to every work-item checkbox being checked:

```bash
# Rust gate (run per-crate while iterating; full suite at the end; never `cargo test`)
cargo nextest run
cargo nextest run --all-features   # Docker-backed smoke tests; CI also runs this

# Docs gate (from docs/)
bun run build
bun run check:repo-links
bunx tsc --noEmit
bun test

# Operator smoke (record the printed run id in the Evidence section at the bottom)
cargo run --bin jackin -- console --debug
```

- Roadmap checklist statuses updated; sidebar/overview audits per `docs/AGENTS.md` re-run.
- `crates/jackin-tui/COMPONENTS.md` and lookbook stories updated if shared
  component contracts changed (W9).
- PR #495 description's Verify-locally block extended per `PULL_REQUESTS.md`
  if new verification steps apply.

## Process constraints

- All work lands on `feature/tui-architecture` (PR #495). Never create a new
  branch. Push after every commit (or after the last commit of a same-turn
  chain).
- Conventional Commits with DCO sign-off and the agent `Co-authored-by`
  trailer (see `COMMITS.md`, `AGENTS.md`).
- `cargo nextest run`, never `cargo test` (`TESTING.md`).
- No `CHANGELOG.md` entries (pre-release rule).
- New cross-cutting TUI rules must land in `docs/content/docs/reference/tui/`
  in the same PR as the behavior (hard rule in `AGENTS.md`). W5 and W9 do this.
- If observed behavior cannot be explained from existing logs, add durable
  `cdebug!`-tier telemetry first and re-run with `--debug`; do not guess.

## Binding interaction contract

The published TUI docs are the spec. The rules this plan enforces already exist
there — cite them in PRs instead of restating:

| # | Rule | Source |
|---|------|--------|
| R1 | `▸` cursor appears on the selected row **only when the enclosing panel owns focus** | `reference/tui/navigation.mdx` ("Selected-row cursor"), `architecture.mdx` |
| R2 | Selected-row highlight fills the full available row width, not just the text | `reference/tui/navigation.mdx`, `chrome.mdx` |
| R3 | Highlight must stop before the scrollbar gutter; scrollbar cells win | enforced by `render_selected_lines_in_area_highlight_stops_before_scrollbar_gutter` test; document in W9 |
| R4 | Scrollbars render only when `is_scrollable` gates true; same gate drives scroll hints | `reference/tui/navigation.mdx`, `dialogs.mdx` |
| R5 | Vertical scrollbar glyph `┃` (Line style) / `█` (Block opt-in), horizontal `━`, track `·` | `reference/tui/visual-design.mdx` |
| R6 | Up/Down move selection inside the focused container; Tab/BackTab cross containers | `reference/tui/navigation.mdx` |
| R7 | Left/Right on tree headers expand/collapse; otherwise horizontal scroll on overflowing details | `reference/tui/navigation.mdx` — extended by the decided rule below (W5) |
| R8 | Every active keyboard shortcut appears in footer hints; hints live in the footer, never inside dialogs | `reference/tui/navigation.mdx`, `dialogs.mdx` |
| R9 | Bottom chrome order: body → hint → spacer → footer/status; modals never draw over reserved rows | `reference/tui/chrome.mdx`, `bottom_chrome_areas(...)` |
| R10 | Exactly one bright (PHOSPHOR_GREEN) focused container per surface layer | `reference/tui/chrome.mdx`, `visual-design.mdx` |
| R11 | Never copy-paste a TUI component; shared implementation for any pattern in >1 place | `reference/tui/components.mdx` (hard reuse rule) |
| R12 | Picker rows render via the shared picker renderer, which reserves the selection gutter | `reference/tui/components.mdx` |
| R13 | Selection/offset state must be clamped after content changes (filter, refresh, resize, expand/collapse) | partially documented; codify in W9 |
| R14 | Topmost modal captures wheel/touchpad scroll; click/drag on scrollbars uses render geometry | partially implemented in `input/mouse.rs`; codify in W9 |

## Decided rules (no open questions)

These were open questions in the original findings; they are now decided.
Implement them as stated; W9 writes them into the published docs.

**Cursor/focus semantics (extends R1, R10):**
- The active selectable surface shows the `▸` cursor.
- Background surfaces under a child prompt/modal keep their selection state but
  must not show the `▸` cursor or a bright focused border.
- Hover never moves selection or steals keyboard focus; clicks select only
  where the existing interaction model already says clicks select.

**Left/Right key ownership (extends R7):**
1. If the selected row can expand/collapse and the pressed direction performs
   that action → expand/collapse.
2. Otherwise, if the focused list has horizontal overflow → horizontal scroll.
3. Otherwise → no-op.
- `h`/`l` remain alternate horizontal-scroll keys.
- Footer hints reflect whichever behavior is active where practical (R8).

## Shared primitive inventory (verified 2026-06-08)

All in `crates/jackin-tui`:

- `src/components/scrollable_panel.rs` — `ScrollableList` (builder: `selected`,
  `offset`, `highlight_style`, `highlight_symbol`, `highlight_spacing`,
  `scrollbar`, `scrollbar_style`; `render` / `render_with_block`) and
  `render_selected_lines_in_area(frame, area, lines, selected)` which
  **delegates to `ScrollableList`** with `cursor_follow_offset` and an
  auto scrollbar. It does **not** set a `highlight_symbol` — callers that need
  `▸` currently prepend it manually (the root cause of W1).
- `src/components/select_list.rs` — `render_picker_list(area, buf, rows, selected)`:
  hardcoded `▸ ` gutter, full-width PHOSPHOR_GREEN highlight, selection-follow,
  right-edge scrollbar, separator rows. `SelectListState`/`SelectList` add
  filtering, horizontal scroll + ellipsis clipping, and a "no matches" empty
  state. **This is the canonical picker renderer (R12).**
- `src/scroll.rs` — complete scroll math: `cursor_follow_offset`,
  `scroll_selectable_list`, `clamp_offset_u16`/`effective_offset_u16`,
  `apply_mouse_scroll_u16`, `mouse_scroll_delta(_with_step)`, `full_cell_thumb`,
  `offset_for_track_position(_u16)`, `is_scrollable`, `max_offset(_u16)`.
- `src/components/bottom_chrome.rs` — `bottom_chrome_areas(area)` →
  `{body, hint, spacer, footer}`; `BOTTOM_CHROME_ROWS = 3`.

Known capability gaps in the shared layer (close only the ones a work item
needs; do not gold-plate):

| Capability | ScrollableList | render_picker_list / SelectList |
|---|---|---|
| `▸` cursor | configurable, present | present (hardcoded) |
| Full-width highlight + gutter stop | present (tested) | present |
| Selection-follow offset | present | present |
| Horizontal scroll + clipping | missing | present (SelectList) |
| PageUp/PageDown | missing | missing |
| Empty-state rendering | missing (caller's job) | present ("no matches") |
| Mouse/wheel handling | caller-driven via `scroll.rs` | caller-driven |

## Surface inventory

The finite list behind every "every selectable list" claim. Verify each against
the acceptance criteria of the relevant work items.

| Surface | Location | Work items |
|---|---|---|
| Workspace sidebar — current dir + saved workspaces + instances + `+ New workspace` | `jackin-console/src/tui/screens/workspaces/view.rs`, `jackin/src/console/tui/components/workspace_list.rs` | W3 W4 W5 |
| Sidebar pickers — agent, provider, role | `workspace_list.rs::render_list_sidebar` picker variants | W3 |
| File Browser (4 host contexts: editor, create-prelude, settings-mounts, settings-auth) | `jackin-console/src/tui/components/file_browser/` | W6 W7 |
| 1Password picker — account, vault, item, section, field stages (+ naming stages) | `jackin-console/src/tui/components/op_picker/` | W1 W2 W7 |
| GitHub picker | `jackin-console/src/tui/components/github_picker.rs` | W1 W7 |
| Role picker (+ RoleOverridePicker, AuthRolePicker modal variants) | `jackin-console/src/tui/components/role_picker.rs` | W1 W7 |
| Workdir picker | `jackin-console/src/tui/components/workdir_pick.rs` | W1 W7 |
| Editor / Settings list-like rows (manual `▸` census) | `jackin-console/src/tui/screens/{editor,settings}/view.rs` | W10 (audit only) |

Out of scope here (owned by `PR-495-TUI-SMOKE-FINDINGS.md`): Debug info dialog,
capsule scrollback/scrollbar, redraw tiers, pane text selection.

---

## Phase 0 — Baseline re-verification

### W0 — Re-verify current state and in-flight work

- [x] W0 complete

The original findings predate fixes already on this branch. Known-landed since:

- `25c78c72 fix: share file browser list rendering` — File Browser now renders
  via `ScrollableList` with `▸ ` highlight symbol, full-width highlight,
  border scrollbar, and `cursor_follow_offset` (`file_browser/render.rs`;
  tests `selected_entry_uses_cursor_and_full_content_width_highlight`,
  `overflowing_listing_shows_border_scrollbar_and_preserves_selected_gutter`).
- `2287a90b fix: unify selected list row rendering` — shared selected-row path.
- **In flight (uncommitted at research time):** File Browser wheel scroll —
  `FileBrowserState::scroll_selection(delta)` in `file_browser/state.rs` plus
  routing in `jackin/src/console/tui/input/mouse.rs::try_scroll_file_browser_modal`
  covering all four host contexts, with tests in `mouse_drag_tests.rs` and
  `state/tests.rs`.

Tasks:
1. Run `git log --oneline -15` and `git status --short`; reconcile every work
   item below against what has already landed. Check off anything done.
2. Run the caller audit (W10 commands) once now to get the current baseline.
3. `cargo nextest run -p jackin-tui -p jackin-console` must be green before
   starting.

---

## Phase 1 — Shared primitive gap-closing (`crates/jackin-tui`)

### W1 — Pickers adopt the canonical picker renderer (R1, R2, R3, R11, R12)

- [x] W1 complete

Symptom: GitHub, Role, and Workdir pickers and the 1Password picker stages
show `▸` but the selected background does not fill the row.

Root cause (verified): these pickers build **pre-styled spans** — manual
`"▸ "` prefix and span-level colors — and pass them to
`render_selected_lines_in_area(...)`, which cannot widen a span-styled
highlight to full width. The full-width path exists and is tested
(`render_selected_lines_in_area_highlights_full_width_when_content_fits`),
but only when callers pass *unstyled-selection* lines plus `selected`.
1Password additionally pre-styles in `op_picker/lines.rs` (7 manual `▸` sites).

Current call sites of `render_selected_lines_in_area`:
`op_picker/render.rs`, `github_picker.rs`, `role_picker.rs`, `workdir_pick.rs`.

Canonical pattern (per `reference/tui/components.mdx`): picker row content
flows through the shared picker renderer (`render_picker_list` /
`SelectList`), which owns the `▸` gutter, full-width highlight, gutter
reservation, selection-follow, and scrollbar.

Tasks:
1. Migrate `github_picker.rs`, `role_picker.rs`, `workdir_pick.rs` to the
   shared picker path: rows in, `selected` index in; delete the manual `▸`
   prefixes and selected-row span styling. Multi-column rows (path + `github ·
   branch` annotation) become row content, not styling forks.
2. Strip embedded cursor/highlight styling from `op_picker/lines.rs` for the
   account, vault, item, section, and field stages; pass clean lines +
   `selected` so the shared renderer owns selection visuals. (Stage-specific
   colors for *non-selected* content may remain span-level.)
3. The section stage currently returns an empty `Vec` instead of rendering a
   list — route it through the same path as the other stages.
4. If `render_selected_lines_in_area` keeps non-picker callers, add an opt-in
   `highlight_symbol` parameter (or builder) so no caller ever needs a manual
   `▸` again; otherwise migrate all callers and shrink its API.
5. Keep `render_selected_lines_in_area` only if it remains a thin delegate to
   the shared selectable-list path; it must never reintroduce span-only
   selected styling.

Acceptance:
- All four pickers + five 1Password stages: full-content-width highlight,
  `▸` gutter from the shared renderer, highlight stops before the scrollbar
  gutter, long labels clip without touching the scrollbar column.
- `rg -n '"▸ ?"' crates/jackin-console/src/tui/components/{github_picker,role_picker,workdir_pick}.rs crates/jackin-console/src/tui/components/op_picker/` → 0 hits.
- New/updated buffer tests per picker asserting full-width highlight and
  gutter stop (mirror `scrollable_panel/tests.rs` assertions).
- Existing tests asserting the old span-styled shape are replaced, not
  loosened.

Verify: `cargo nextest run -p jackin-console -p jackin-tui`

### W2 — Picker state normalization (R13)

- [x] W2 complete

Depends on W1 (same files).

Tasks:
1. Filter edits, refresh/reload, and stage changes in the 1Password picker and
   Role picker must clamp selection and scroll offset through the shared
   normalization helpers (`cursor_follow_offset`, `clamp_offset_u16` /
   `scroll_selectable_list` from `scroll.rs`) — no local clamp math.
2. Loading and recoverable-error banners in the 1Password picker must not
   disturb list geometry (selection-follow still correct with the banner rows
   present).
3. Filtered empty states use the shared empty-state rendering (`SelectList`
   "no matches" pattern): no stale cursor, no stale highlight, no stale
   scrollbar.

Acceptance: tests covering filter-shrinks-below-selection,
refresh-removes-selected-row, and banner-present cases for the 1Password
picker; at least one equivalent test for the Role picker.

Verify: `cargo nextest run -p jackin-console`

Implementation evidence:
- Role picker now has a filter-shrinks-below-selection test proving it resets
  to the first matching row instead of retaining a stale out-of-range
  selection.
- 1Password Account and Item stages now have state-level tests proving filter
  edits reset to the first match or clear selection when no rows remain.
- 1Password refresh/load-completion plans now have tests proving row removal
  resets selection to the first available row or clears it when no rows remain.
- The 1Password recoverable-error banner has a render-buffer test proving the
  banner rows do not disturb list geometry: the selected row remains visible,
  uses the shared full-width highlight, and stops before the dialog border.

---

## Phase 2 — Workspace sidebar

### W3 — Sidebar selected-row cursor regression (R1, R10)

- [x] W3 complete

Symptom: left sidebar rows (workspaces, role list, agent picker, provider
picker) render without the `▸` cursor:

```text
│agent-smith                   │      ← broken
│▸ agent-smith                 │      ← expected when sidebar owns focus
```

Current state (verified): the render path *supports* the cursor —
`push_tree_workspace_line` in `jackin-console/src/tui/screens/workspaces/view.rs`
renders `▸` gated on `row.selected && show_cursor`. The regression is in the
flag plumbing, not the renderer.

Tasks:
1. Trace `show_cursor` from `jackin/src/console/tui/components/workspace_list.rs`
   (`list_name_lines` → `workspace_list_name_lines`) back to
   `ManagerState::list_focus_owner` / `list_names_focused()`. Find where the
   focus signal stopped reaching the renderer.
2. Note R1 is focus-gated: no cursor on an *unfocused* sidebar is correct.
   The bug is only "sidebar owns focus and still shows no cursor". Write the
   failing test first to pin the actual broken state.
3. Apply the same check to the sidebar picker variants (agent, provider, role)
   rendered via `render_list_sidebar`.
4. Per the decided cursor rule: when a child modal/prompt is open above the
   sidebar, the sidebar must not show the cursor or bright border.

Acceptance: buffer tests — focused sidebar shows `▸` on the selected row;
unfocused sidebar shows none; sidebar under an open modal shows none.

Verify: `cargo nextest run -p jackin -p jackin-console`

Implementation evidence:
- `render_list_sidebar` now computes a single `sidebar_owns_focus` flag
  (`list_names_focused && list_modal.is_none()`) and passes it into every
  normal and picker-sidebar row renderer.
- `list_name_lines` now receives that focus flag directly, so manual cursor
  construction cannot happen before modal/background focus is known.
- Buffer tests cover focused sidebar cursor, sidebar under a list modal hiding
  the cursor, and selection-follow keeping the visible cursor on the selected
  sentinel row.

### W4 — Sidebar vertical selection-follow (R4, R6, R13)

- [x] W4 complete

Symptom: selection can move below the viewport (e.g. selected row is
`+ New workspace` while the sidebar is still scrolled to the top).

Current state (verified): a real gap. `ManagerState` has `list_names_scroll_x`
but **no vertical offset field** for the names list;
`render_list_names_block` takes visible rows from the top. The vertical
scrollbar renders (`render_vertical_scrollbar`) but has no offset to reflect.

Tasks:
1. Add vertical offset state for the sidebar names list (alongside
   `list_names_scroll_x` in `ManagerState`), reset in `reset_list_scroll()`.
2. Drive it with `cursor_follow_offset` from `jackin-tui/src/scroll.rs` —
   do not write new scroll math (R11). Apply in the render path of
   `render_list_names_block` so the offset also recomputes when content
   changes (refresh, expand/collapse, resize), not only on Up/Down.
3. Clamp through the shared helpers after: selection move, expansion toggle,
   refresh removing rows, terminal resize (`clamp_list_scroll_after_key` is
   the existing hook point in `jackin/src/console/tui/input/list.rs`).
4. The vertical scrollbar must reflect the offset (same `full_cell_thumb`
   geometry as rendering).
5. Current-directory and saved-workspace variants must share the exact same
   code path — symmetric variants, one implementation (DRY rule).

Acceptance: tests — selection moved to `+ New workspace` scrolls the viewport;
selection into expanded instance rows stays visible; refresh that removes rows
clamps selection and offset; resize keeps the selected row visible.

Verify: `cargo nextest run -p jackin -p jackin-console`

### W5 — Sidebar Left/Right key ownership (R7, R8)

- [x] W5 complete

Depends on W4 (vertical offset exists; horizontal already exists as
`ScrollListHorizontal` ±8 via `h`/`l` in `input/list.rs`).

Current state (verified): Left/Right are hardwired to
`CollapseSelectedTree`/`ExpandSelectedTree`; `h`/`l` are the only
horizontal-scroll keys.

Tasks:
1. Implement the decided Left/Right ownership rule (see *Decided rules*):
   expand/collapse when applicable to the selected row, else horizontal scroll
   when overflowing, else no-op. Route through the existing messages
   (`CollapseSelectedTree` / `ExpandSelectedTree` / `ScrollListHorizontal`) —
   the decision lives in `input/list.rs`, not in a new message.
2. Update footer hints to reflect the active behavior (R8) via the existing
   `WorkspaceListFooterMode` builders in
   `jackin-console/src/tui/components/footer_hints.rs`.
3. Codify the extended rule in `docs/content/docs/reference/tui/navigation.mdx`
   in the same commit (hard rule).

Acceptance: tests — Left on an expandable selected row collapses; Right on a
collapsed row expands; Left/Right on a non-expandable row with horizontal
overflow scrolls; without overflow, no-op; hints update.

Verify: `cargo nextest run -p jackin` + docs gate.

Implementation evidence:
- Left/Right now route through the decided ownership rule in `input/list.rs`:
  expandable/collapsible selected rows consume the key first; otherwise an
  overflowing focused sidebar scrolls horizontally; otherwise the key is a
  no-op.
- Footer hints now include a `←→ scroll` fallback when the focused sidebar has
  horizontal overflow and the selected row does not own expand/collapse.
- Focused tests cover expand/collapse preservation, Left/Right horizontal
  fallback in both directions, and the footer hint fallback.

---

## Phase 3 — File Browser completion

### W6 — File Browser interaction parity (R1, R2, R4, R6, R8, R13)

- [x] W6 complete

Current state (verified): largely fixed by `25c78c72` — shared
`ScrollableList`, `▸ ` cursor, full-width highlight, border scrollbar,
selection-follow, with buffer tests. Wheel scroll is in flight (uncommitted:
`scroll_selection` + mouse routing + tests). Hint wiring exists for the main
modal path (`Modal::FileBrowser => state.footer_items()` in
`jackin/src/console/tui/components/footer/modal.rs`) and for settings-mounts
(`settings_mounts_modal_footer_items`).

Remaining tasks:
1. Land/verify the in-flight wheel-scroll work; confirm
   `try_scroll_file_browser_modal` covers **all four** host contexts (editor,
   create-prelude, settings-mounts, settings-auth) and that tests in
   `mouse/mouse_drag_tests.rs` and `file_browser/state/tests.rs` pass.
2. Hints: write one test per host context proving File Browser hints reach the
   footer (the original screenshot showed at least one context dropping them).
   `FileBrowserState::footer_items()` is the single source; the four contexts
   must all route through it (R8, R11).
3. Nested git prompt: when `pending_git_prompt` is open, the prompt owns hints
   (`git_prompt_footer_items()`) and focus visuals; the background browser
   must not keep a bright active border or cursor (decided cursor rule, R10).
   Add a buffer test.
4. Add PageUp/PageDown to `file_browser/input.rs` via shared scroll math
   (`scroll_selectable_list` with viewport-sized delta). Add the hint if
   space allows (R8).
5. Edge cases to cover with tests if not already: parent `../` row selection,
   `(git)` suffix rows under selection (suffix legible on highlight),
   rejection banner present (list geometry intact), hidden-directory mode,
   empty directory (no stale cursor/scrollbar), single-entry directory,
   very narrow modal widths (unicode-width clipping, cursor gutter intact).

Acceptance: all five task groups have passing tests; a deterministic smoke
test in one host context confirms wheel + keys + hints together. The broader
operator smoke remains part of W10.

Verify: `cargo nextest run -p jackin -p jackin-console`

Implementation evidence:
- PageUp/PageDown now route through `FileBrowserState::handle_key_with_page_rows`
  and `page_selection`, which delegates to the shared saturating list-selection
  scroll helper. The host modal handlers compute page rows from the same
  File Browser modal/listing geometry as render/mouse hit-testing.
- File Browser footer hints now advertise `PgUp/PgDn page`.
- Focused tests cover PageUp/PageDown saturation and the four host modal input
  paths compile against the viewport-aware handler.
- Footer tests prove File Browser hints reach all four host paths: list modal,
  create-prelude modal, editor modal, settings mounts, and settings auth
  source-folder picker.
- A render-buffer test proves the nested git prompt suppresses the background
  browser's `▸` cursor and bright border while the child prompt owns focus.
- A smoke-style editor-host test exercises the full W6 interaction bundle in one
  context: the File Browser footer advertises `PgUp/PgDn`, PageDown moves the
  modal selection by visible rows, wheel moves the modal selection again, and
  the background editor scroll offset remains unchanged.

---

## Phase 4 — Mouse routing and scrollbar geometry

### W7 — Wheel capture for list modals + drag geometry audit (R14)

- [x] W7 complete

Current state (verified): topmost-modal wheel capture exists for ContainerInfo
and (in flight) File Browser in `jackin/src/console/tui/input/mouse.rs`.
Scrollbar drag exists for mount blocks (`drag_scrollbar`,
`drag_vertical_scrollbar`, `try_drag_{horizontal,vertical}_scrollbar`).

Tasks:
1. Extend topmost-modal wheel capture to the remaining list modals: 1Password
   picker (all list stages), GitHub picker, Role picker variants, Workdir
   picker. Reuse the File Browser pattern (`scroll_selection`-style state
   method + one routing arm); do not write per-modal scroll math (R11) —
   `scroll_selectable_list` / `apply_mouse_scroll_u16` from `scroll.rs` are
   the primitives.
2. Wheel must never fall through a modal to the panel behind it. Add one
   regression test: wheel over an open picker scrolls the picker, not the
   sidebar.
3. Audit that every scrollbar drag/click hit-test uses the same geometry
   helpers as rendering (`full_cell_thumb`, `offset_for_track_position*`).
   Where a drag path duplicates geometry locally, replace with the shared
   helper. Cover File Browser scrollbar drag if/when its scrollbar is
   draggable; if not draggable, no work.

Acceptance: wheel tests per modal class; drag tests keep passing; no local
thumb-geometry math outside `scroll.rs`
(`rg -n 'thumb' crates/jackin/src/console/tui/input/mouse.rs` shows only
shared-helper calls).

Verify: `cargo nextest run -p jackin`

---

## Phase 5 — Documentation, components, lookbook

### W8 — Codify rules and update component docs (hard rules)

- [x] W8 complete

Tasks:
1. `docs/content/docs/reference/tui/navigation.mdx`: add the extended
   Left/Right ownership rule (W5, if not already landed with W5) and the
   PageUp/PageDown convention if W6 adds it. Write rules as enforceable
   pass/fail statements.
2. `docs/content/docs/reference/tui/navigation.mdx` or `chrome.mdx`: codify
   the decided cursor/background-modal rule (active surface shows `▸`;
   surfaces under a child prompt show neither cursor nor bright border) and
   the topmost-modal wheel-capture rule (R14), and the scrollbar-gutter
   priority rule (R3) if absent.
3. `docs/content/docs/reference/tui/components.mdx`: state explicitly that
   filtered pickers must route rows through the shared picker renderer and
   must not pre-style selection (post-W1 reality).
4. `crates/jackin-tui/COMPONENTS.md`: refresh the ScrollablePanel and
   SelectList entries (call sites, maturity, the "rich host pickers still need
   a generic FilterListPicker" note if W1 changes that picture).
5. Lookbook (`crates/jackin-tui-lookbook`): refresh/extend stories if shared
   component behavior changed — at minimum confirm `select-list/*` and
   `scrollable-panel/*` stories still render the canonical look; add a story
   only for genuinely new shared behavior. Regenerate exports per the crate's
   README/main invocation.
6. Roadmap freshness: update defect statuses in
   `post-restructure-fixes-checklist.mdx` for everything shipped by W1–W7;
   run the sidebar/overview audits from `docs/AGENTS.md`.

Acceptance: docs gate green; every decided rule from this file exists verbatim
(or stronger) in a published TUI page; no rule lives only in this file.

Verify: docs gate commands from *Definition of done*.

Implementation evidence:
- `navigation.mdx` now codifies the extended workspace-manager Left/Right rule,
  File Browser PageUp/PageDown semantics, full-width highlight and scrollbar
  gutter priority, focus-gated `▸`, background child-prompt suppression, and
  topmost-modal wheel/touchpad capture.
- `components.mdx` now states that rich filtered pickers may own their local
  state/filter flow, but must pass neutral rows to the shared picker renderer
  and must not pre-style selection or paint their own selected cursor.
- `crates/jackin-tui/COMPONENTS.md` records the current Filter list picker
  maturity and the future `FilterListPicker<T>` extraction as state-boilerplate
  cleanup, not a reason to bypass shared rendering.
- Lookbook exports were regenerated and the lookbook `--check` command exits 0.
- Roadmap freshness reviewed: the matching roadmap checklist items are either
  already closed from earlier convergence work or deliberately remain open
  pending the live smoke evidence recorded in `PR-495-TUI-SMOKE-FINDINGS.md`;
  no additional roadmap checkbox can be safely changed from W1-W7 alone.

---

## Phase 6 — Repo-wide audit and final sweep

### W9 — Caller and glyph audit (R11)

- [x] W9 complete

Do not fix only the named surfaces; audit all callers.

```bash
# Selected-lines renderer callers — after W1, expect only the shared impl,
# tests, and any documented non-picker delegates:
rg -ln 'render_selected_lines_in_area' --type rust

# Manual ▸ census — after W1/W3, manual cursor construction should exist only in:
#   jackin-tui shared components (+ their tests), the lookbook,
#   and the workspaces tree renderer (documented custom renderer).
rg -ln '▸|\\u\{25b8\}' --type rust crates/ src/

# Local scroll math smell — selection/offset clamping outside jackin-tui:
rg -n 'saturating_sub\(1\)|min\(.*len\(\)' crates/jackin-console/src/tui/components/ | rg -v tests
```

Baseline at research time: manual `▸` sites in `op_picker/lines.rs` (7),
`github_picker.rs`, `role_picker.rs`, `workdir_pick.rs`, `agent_choice.rs`,
`auth_panel.rs`, `editor/view.rs` (9), `settings/view.rs` (9),
`workspaces/view.rs` (4), capsule dialog components.

Tasks:
1. Re-run the audit; classify every remaining hit: shared component / test /
   documented-exception custom renderer / unfixed drift. Fix or annotate.
   A custom renderer that stays custom needs a one-line comment naming why it
   cannot use the shared renderer (per `AGENTS.md` DRY rule).
2. `editor/view.rs` and `settings/view.rs` manual-`▸` sites: audit against
   R1/R2; fix in place only if they are selectable-list rows violating the
   contract — otherwise record them as action-row styling exceptions
   (`action_row_style` is a documented shared pattern).
3. Audit tests that encode the *old* behavior and must be replaced:
   any test asserting selected highlight stops after text content; any
   File Browser test assuming a `Paragraph` listing; any footer test allowing
   a modal with actions to produce no hints.

Acceptance: audit output recorded below in *Evidence*; zero unclassified hits.

Implementation evidence:
- The simple Auth-tab `agent_choice` modal now delegates selected-row chrome to
  `render_picker_lines` instead of hand-painting its own `▸` prefix.
- Inline workspace sidebar pickers now reserve the selected gutter in both
  focused and unfocused states, but paint the `▸` cursor only when focused.
- Remaining production glyph sites are classified as shared components, raw-ANSI
  capsule dialog documentation/adapters, File Browser's `ScrollableList`
  `highlight_symbol` call, or documented structural exceptions for mixed
  form/table/tree row renderers (`editor/view.rs`, `settings/view.rs`,
  `auth_panel.rs`, `workspaces/view.rs`).

### W10 — Full verification and operator smoke

- [ ] W10 complete

Tasks:
1. `cargo nextest run` and `cargo nextest run --all-features` green at
   workspace root.
2. Docs gate green (see *Definition of done*).
3. Operator smoke: `cargo run --bin jackin -- console --debug` — walk:
   sidebar selection past viewport bottom (W4), Left/Right on expandable and
   plain rows (W5), open File Browser in two contexts and wheel-scroll +
   PageDown (W6), open 1Password picker and GitHub picker, filter, wheel (W1,
   W2, W7). Record the run id below.
4. Reconcile PR #495 description: Verify-locally block, summary of shipped
   interaction fixes, roadmap links (per `PULL_REQUESTS.md` and
   `.github/AGENTS.md`).

## Evidence

Append verification evidence here as work completes (test run summaries, audit
output, smoke run ids):

- 2026-06-08 baseline: `cargo nextest run -p jackin-tui -p jackin-console`
  passed 601/601 tests; run id `23e24c35-5df1-472f-959a-bdef20ce970a`.
- W1 picker migration: `cargo nextest run -p jackin-console -p jackin-tui
  github_picker role_picker workdir_pick op_picker::tests` passed 90/90 tests;
  run id `a4ca2307-999b-4d5d-8a77-e513deee33aa`.
- W4/W5 focused sidebar/input coverage: `cargo nextest run -p jackin
  input::list::tests` passed 27/27 tests; run id
  `bdb83b5a-f526-4ed0-9118-959fa8471dde`. `cargo nextest run -p jackin
  input::mouse input::list::tests list_name` later passed 85/85 tests; run id
  `ae52e21b-387b-4a32-a603-c3c5c1ba77b4`.
- W6 File Browser verification after reusing the shared wheel-selection helper:
  `cargo nextest run -p jackin-console file_browser` passed 52/52 tests; run id
  `5f2e8813-34d3-4151-8379-7ff2dadd2e18`. Focused host-context wheel tests
  passed 2/2; run id `6370f28b-3785-4fd3-a93d-556c62b0e8b5`.
- W2 picker normalization partial coverage: `cargo nextest run -p
  jackin-console role_picker op_picker::input` passed 19/19 tests; run id
  `2008b097-db83-4c75-b3c4-b63272030567`.
- W2 picker normalization close-out: `cargo nextest run -p jackin-console
  op_picker role_picker` passed 77/77 tests; run id
  `96fca4d9-c517-4424-8bd0-3f1fd3789ca3`.
- W3 sidebar cursor focus gating: `cargo nextest run -p jackin list_name`
  passed 10/10 tests after the focus-plumbing fix; run id
  `48269751-d6e7-4101-875d-6f6632245061`.
- W6 PageUp/PageDown and host modal input wiring: `cargo nextest run -p
  jackin-console file_browser` passed 54/54 tests; run id
  `341e78e7-bc7e-478c-9441-896b79101187`. `cargo nextest run -p jackin
  input::editor input::prelude input::global_mounts` passed 102/102 tests;
  run id `a6b2b48f-61e2-43f7-8b0f-25c0b967db78`.
- W5/W6 footer and nested-prompt close-out: `cargo nextest run -p
  jackin-console file_browser footer_hints` passed 75/75 tests; run id
  `c7e78a8e-b15b-4684-b25c-1fe7f8b45b3e`. `cargo nextest run -p jackin
  view::frame components::footer input::list::tests` passed 35/35 tests; run
  id `4eaeac66-e199-41da-b5e7-90283d02a110`.
- W6 deterministic editor/File Browser smoke: `cargo nextest run -p jackin
  editor_file_browser_smoke_hints_pagedown_and_wheel_share_modal_context`
  passed 1/1 tests; run id `0dd4d37e-97a2-47ed-b501-e143bd797064`.
- Broad touched-crate gate after W6 deterministic smoke close-out: `cargo
  nextest run -p jackin-tui -p jackin-console -p jackin` passed 1682/1682
  tests; run id `d4413033-ebb9-4b48-a855-eadab299add5`.
- W7 modal wheel capture: `cargo nextest run -p jackin
  input::mouse::mouse_drag_tests` passed 53/53 tests; run id
  `e48d79c9-ecb0-4cf5-b0c8-903361eecd53`.
- Shared list helper coverage: `cargo nextest run -p jackin-console
  list_helpers` passed 7/7 tests; run id
  `5aee0904-8a75-4a89-8305-00c6b423a9b9`.
- Broad touched-crate gate after the production changes: `cargo nextest run -p jackin-tui
  -p jackin-console -p jackin` passed 1658/1658 tests; run id
  `f42bb937-19de-4bb9-8109-d76f551c32cc`.
- Docs gate for the TUI reference updates: from `docs/`, `bun run build`,
  `bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test` all exited
  0.
- Broad touched-crate gate after W2/W3/W6 follow-up work: `cargo nextest run
  -p jackin-tui -p jackin-console -p jackin` passed 1671/1671 tests; run id
  `12b9f8ef-a3d1-4ff8-8936-e9b6efac84e1`.
- Broad touched-crate gate after W2/W5/W6 close-out work: `cargo nextest run
  -p jackin-tui -p jackin-console -p jackin` passed 1680/1680 tests; run id
  `31eebcb0-ab6f-4edf-9f86-1f406bd06c15`.
- W9 agent-choice/shared-renderer and custom-row classification focused
  coverage: `cargo nextest run -p jackin-console agent_choice` passed 6/6
  tests; run id `3c2ff043-d0e7-4c3e-85bf-3033ee2758b0`.
  `cargo nextest run -p jackin-console
  agent_choice settings::view editor::view auth_panel` passed 66/66 tests; run
  id `7b1ba2fa-f843-4018-88b2-5e3f106d705b`. `cargo nextest run -p
  jackin-console workspaces::view` passed 9/9 tests after adding the inline
  picker focus-gated cursor regression; run id
  `cfcb45ad-8b6f-4d5f-b25d-522e32509644`.
- Broad touched-crate gate after W8/W9 close-out work: `cargo nextest run -p
  jackin-tui -p jackin-console -p jackin` passed 1681/1681 tests; run id
  `20238f05-56b4-40a3-9963-f8f0373aaae5`.
- Docs gate after documenting File Browser PageUp/PageDown: from `docs/`,
  `bun run build`, `bun run check:repo-links`, `bunx tsc --noEmit`, and
  `bun test` all exited 0.
- Docs gate after TUI interaction close-out docs: from `docs/`,
  `bun run build`, `bun run check:repo-links`, `bunx tsc --noEmit`, and
  `bun test` all exited 0.
- Docs gate after picker-renderer component-doc clarification: from `docs/`,
  `bun run build`, `bun run check:repo-links`, `bunx tsc --noEmit`, and
  `bun test` all exited 0. Roadmap sidebar audit also passed:
  `bun run check:roadmap-sidebar` checked 90 roadmap pages against 22
  `meta.json` files. The overview coverage audit still reports pre-existing
  roadmap leaves that are not direct overview bullets; no roadmap status changed
  in this W8/W9 close-out.
- Lookbook export check initially reported stale generated previews for
  `select-list-agent-picker.svg`, `toast-selection-copied.svg`, and
  `container-info-debug.svg`. Regenerated with `cargo run -p
  jackin-tui-lookbook -- docs/public/tui-lookbook`; follow-up `cargo run -p
  jackin-tui-lookbook -- --check docs/public/tui-lookbook` exited 0.
- Full workspace gate currently blocked outside this interaction scope:
  `cargo nextest run` failed in `jackin-runtime
  runtime::progress::tests::failure_copy_target_at_hits_each_copyable_row_value`
  and `runtime::progress::tests::failure_copy_target_at_ignores_non_copyable_rows_and_absent_paths`
  (run id `ff69b335-66e6-495b-80ac-199bb9a6db6c`; 3150 passed, 2 failed,
  277 not run due fail-fast). Focused rerun reproduced both failures, run id
  `be8682f8-d0c3-49c8-9e75-cc37c00994c4`. The failing file
  `crates/jackin-runtime/src/runtime/progress/tests.rs` is not touched by this
  TUI interaction work and appears to belong to the separate smoke/debug-info
  stream.
- Fresh full workspace retry after W8/W9 close-out is still blocked by the same
  two `jackin-runtime runtime::progress` copy-target tests: `cargo nextest run`
  failed with 3153 passed, 2 failed, 276 not run due fail-fast; run id
  `4432c8ad-d914-4795-a20b-3c743fd4f26a`.
- Full workspace default gate after the runtime-progress test update now passes:
  `cargo nextest run` passed 3432/3432 tests; run id
  `77469c10-2f03-4387-a3a3-f0d2a0902bf9`.
- Full workspace all-features gate is still blocked outside this interaction
  scope: `cargo nextest run --all-features` failed with 3435 passed and 2
  failed; run id `b0bd3c0d-53fc-4809-9cc7-a62227f4dc57`. The first failure
  was `jackin::dind_e2e jackin_load_sentinel_role_runs_hooks_and_keeps_build_output_off_screen`,
  which timed out waiting for the sentinel report after launch diagnostics
  showed GitHub returned HTTP 404 for the preview `capsule-manifest.json`
  during jackin-capsule binary preparation. The second remaining Docker e2e
  test was still running after cancellation and was terminated with SIGTERM at
  657.193s so no verification process remained running.
- CI-style local all-features retry with
  `JACKIN_CAPSULE_BIN=$PWD/target/debug/jackin-capsule cargo nextest run --all-features`
  is also not usable on this macOS host: run id
  `e906756a-65ff-4490-a469-a50a1b855999` failed with 3435 passed and 2 failed.
  The override bypassed the missing preview manifest, but copied the host
  Darwin `jackin-capsule` into the Linux Docker e2e container; the
  `jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env` case then
  exited 126 while waiting for the capsule daemon. The remaining sentinel Docker
  e2e test was still running after cancellation and was terminated with SIGTERM
  at 266.439s so no verification process remained running.
- Correct macOS Docker e2e capsule export path: `cargo run --bin
  build-jackin-capsule -- --export` succeeded and produced
  `JACKIN_CAPSULE_BIN=/Users/donbeave/.jackin/cache/jackin-capsule/0.6.0-dev_3dc8103/linux-arm64/jackin-capsule`.
  With that Linux capsule binary, the narrow Docker e2e gate
  `cargo nextest run -p jackin --test dind_e2e --all-features --no-fail-fast`
  passed 2/2 tests; run id `a7a31f41-ca58-4cca-9aba-3269abf6e620`.
- Full workspace all-features gate now passes with the exported Linux capsule
  binary: `JACKIN_CAPSULE_BIN=/Users/donbeave/.jackin/cache/jackin-capsule/0.6.0-dev_3dc8103/linux-arm64/jackin-capsule
  cargo nextest run --all-features` passed 3437/3437 tests; run id
  `0f35b6f8-7768-4af6-b5db-d2cd48cf71a5`.
- PR #495 description was updated with a `2026-06-08 TUI interaction findings`
  verification ledger entry. It records W0-W9 as implemented, the focused and
  broad TUI gates, the full default workspace pass, the host-specific
  all-features/Docker e2e blockers, and the remaining W10 operator/Linux e2e
  close-out items without marking W10 complete.
- PR #495 description was updated again with a `2026-06-08 TUI all-features
  close-out` entry after the correct exported Linux capsule path cleared both
  Docker e2e and full all-features gates. The entry leaves W10 operator smoke
  run IDs and final close-out reconciliation open.
- W9 audit snapshot:
  - `rg -ln 'render_selected_lines_in_area' --type rust` now returns only
    `crates/jackin-tui/src/components.rs`,
    `crates/jackin-tui/src/components/scrollable_panel.rs`, and
    `crates/jackin-tui/src/components/scrollable_panel/tests.rs`.
  - Manual `▸`/`\u{25b8}` hits remain only in shared components, tests/lookbook,
    documented structural exceptions (`auth_panel.rs`,
    editor/settings/workspaces view rows, capsule dialog widgets/docs), and
    `file_browser/render.rs`'s `ScrollableList::highlight_symbol` call.
    `agent_choice.rs` no longer appears in the production glyph census.
  - Local scroll-math smell audit still shows only
    `provider_picker.rs:59` (`selected.saturating_sub(1)`, a tiny non-scroll
    provider picker selection clamp) and `editor_rows.rs:187`
    (`budget.saturating_sub(1)`, string truncation budget, not scroll).
  - `rg -n 'thumb' crates/jackin/src/console/tui/input/mouse.rs` returns no
    hits; scrollbar drag paths do not carry local thumb geometry.

---

## Not in scope

This plan is not permission to:

- Redesign colors, typography, borders, or general layout.
- Change footer/status/debug-bar behavior that is already correct (the newer
  scrollbar glyph style, footer key rendering, and debug status/run-id bar are
  confirmed good — do not revert them).
- Touch the F1–F10 findings owned by `PR-495-TUI-SMOKE-FINDINGS.md`.
- Change host-side behavior or persisted config.
- Introduce a new TUI framework or replace Ratatui primitives wholesale.
- Migrate the pending TUI directory moves (`jackin-console` vs
  `jackin/src/console/tui` split is a separate migration; both trees stay).

## Implementation cautions

- Re-verify before working: this branch moves fast; every "Current state"
  above can be stale by the time an item starts.
- Do not regress: vertical scrollbar style, footer key labels, debug
  run-id/status bar, the File Browser tests landed in `25c78c72`, the
  selected-row tests landed in `2287a90b`.
- Scrollbar cells always win over row highlight (R3).
- No new parallel list renderers; extend `ScrollableList`, `select_list`, and
  `scroll.rs` instead (R11). A surface that must stay custom still consumes
  shared scroll math, scrollbar geometry, selected-row styles, and footer
  hints.
- Do not silently change key meanings: hints (R8), tests, and
  `navigation.mdx` move in the same commit.
- Keep visual changes scoped to selection, scroll, and hint consistency.
