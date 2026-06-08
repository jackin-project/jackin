# Goal — Phase 5: Dialogs, rows & click targets

Parent index: [`../PR-495-FIXES.md`](../PR-495-FIXES.md). HEAD baseline: `f920b29a`.

All five tasks are confirmed real at HEAD. Each fix belongs in a shared helper, not at one caller. **Read `dialogs.mdx` (five-slot padding, modal lifecycle) and `visual-design.mdx` (action-row style, cursor gutter) first.**

## Tasks

| ID | Status | Files / evidence | Helper | Verify | Acceptance |
|---|---|---|---|---|---|
| `DLG-1` | pending | `crates/jackin-console/src/tui/components/file_browser/git_prompt.rs:147` (`git_prompt_rect` = 8/7) vs `:249` (`render_git_prompt` = 7/6); render hand-rolls constraints instead of `dialog_inner_chunks` | `dialog_inner_chunks` | `cargo nextest run -p jackin-console` | `render_git_prompt` uses the five-slot layout; rect height == render height; one blank leading spacer below the top border; URL click rect still maps to the URL row. |
| `DLG-2` | pending | `crates/jackin-console/src/tui/components/file_browser/render.rs:104` (`show_cursor = pending_git_prompt.is_none()`) + `:140-142` (`highlight_symbol` only set when `show_cursor`) → gutter collapses when the child dialog opens | `HighlightSpacing::Always` + constant-width symbol | `cargo nextest run -p jackin-console` | Row text start column is identical whether the git prompt is open or closed; the `▸` glyph is hidden behind the child dialog but its two-cell gutter is reserved. Extend `git_prompt_background_suppresses_browser_cursor_and_active_border`. |
| `DLG-3` | pending | `crates/jackin-console/src/tui/screens/settings/view.rs:477-478` (Kind row, 1-cell cursor) vs `:511,540` (source rows, 2-cell); no shared selectable-row. Parity with workspace-editor Auth per `PRE-3`. | shared selectable-row / cursor-gutter helper | `cargo nextest run -p jackin-console` | Every selectable Auth row reserves the same two-cell gutter; selected shows `▸`, unselected shows `  `; label start column identical for `Mode`, `Source`/`Source folder`, `+ Override for a role`. Fix applies to both Settings and workspace-editor Auth (or the single shared path). |
| `DLG-4` | pending | `crates/jackin-console/src/tui/screens/workspaces/view.rs:77-86` (`new_workspace_display_row`, tone `White`) + `:286-348` (`push_tree_workspace_line` hardcodes `"{cursor}  {label}"`); `+ Add mount` correctly uses `action_row_style` (`settings/view.rs:637`, `editor/view.rs:430`) | `action_row_style(selected)` | `cargo nextest run -p jackin-console` | `+ New workspace` renders through `action_row_style` (same `ACTION_ACCENT` fg, bold-when-selected, gutter). Audit every `+ ` row across list/editor/settings/auth/env/mounts/pickers; all route through the shared helper. |
| `DLG-5` | pending | `crates/jackin-tui/src/components/error_dialog.rs:65` — `body_rows = inner.height.saturating_sub(4)` gives all remaining inner height to the body, so short messages get >1 blank row before `OK` | `ErrorDialog` (shared) | `cargo nextest run -p jackin-tui` | Exactly one blank row between the last message line and `OK` for every caller and the lookbook `error/default` story. Fix in the shared component, not the `Load role failed` caller. |

## Detail

### `DLG-1` — git prompt five-slot + height parity
Render with `dialog_inner_chunks(inner, Some(content_rows))`: leading spacer, content (prompt + optional URL), spacer, action row, trailing spacer. Reconcile the height so `git_prompt_rect` and `render_git_prompt` agree (the hit-test rect and the rendered box must be the same size). Add/adjust a render test: the row below the top border is blank, the prompt starts on the next row, buttons are separated by one spacer, and the URL click rect points at the URL row.

### `DLG-2` — stable gutter behind child dialog
`HighlightSpacing::Always` is set, but `highlight_symbol` is only applied when `show_cursor` is true, so hiding the cursor collapses the reserved column. Keep the symbol width reserved always (render a blank two-cell symbol when suppressed) so the parent list does not reflow when the `Git repository detected` child opens. The parent may dim its border and drop the active marker — but not move text.

### `DLG-3` — one selectable-row primitive for Auth
Route `render_auth_source_line` / `render_auth_source_folder_line` and the Kind row through the same two-cell cursor-gutter helper as the rest of the Auth list. Use the `PRE-3` finding: if Settings and workspace-editor Auth are separate paths, fix both (or unify them) so the parity rule holds. Add coverage with the source row selected and unselected, asserting identical label start columns.

### `DLG-4` — one action-row style for every `+ ` sentinel
`+ New workspace` is hardcoded white; `+ Add mount` uses `action_row_style`. Route the workspace-list sentinel through the same helper (extend it if it cannot currently own row construction + cursor gutter + selected state). Then sweep all `+ ` rows so none hand-roll their own style. Snapshot-compare `+ New workspace` and `+ Add mount` in selected and unselected states.

### `DLG-5` — error dialog one-row spacing
Size the body slot from the estimated wrapped message rows (capped by available space) and reserve exactly: leading spacer (1), body, spacer (1), `OK` (1), trailing spacer (1). Keep wrapping/scroll for overflow per the dialog standard — do not silently clip. Add a shared `ErrorDialog` test that finds the last non-empty message row and the `OK` row and asserts exactly one blank row between them; confirm the lookbook story uses the shared component with no local override, and regenerate its SVG.

## Done definition
- `DLG-1`–`DLG-5` each fixed in the shared helper with a render/geometry test.
- Lookbook stories/SVGs regenerated for any changed shared dialog/row output.
- `cargo nextest run -p jackin-console -p jackin-tui` green.
