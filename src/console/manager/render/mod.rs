//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use super::state::{ManagerListRow, ManagerStage, ManagerState};
use crate::config::AppConfig;
use jackin_tui::HintSpan;

pub mod editor;
pub(super) mod global_mounts;
pub(super) mod list;
pub(super) mod modal;

// input::mouse has no path into the modal submodule — re-exported here so it
// can reach modal_outer_rect via super::super::render.
pub(super) use modal::modal_outer_rect;
// Modal dismissal sequencing requires a render call across a module boundary;
// re-exported here so input handlers can reach render_editor directly.
pub use editor::render_editor;

pub(in crate::console::manager) use crate::console::widgets::scrollable::{
    apply_horizontal_scroll_delta, apply_scroll_delta, clamp_scroll_offset as clamp_scroll_x,
    cursor_follow_offset, horizontal_scrollbar_area, is_scrollable,
    max_offset as max_scroll_offset, scrollbar_offset_for_track_position,
    viewport_height as scroll_viewport_height, viewport_width as scroll_viewport_width,
};
pub(super) use crate::console::widgets::scrollable::{
    line_width, max_line_width, render_scrollable_block,
};
pub(super) use crate::console::widgets::{
    PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, TAB_BG_ACTIVE, TAB_BG_ACTIVE_HOVER,
    TAB_BG_INACTIVE, TAB_BG_INACTIVE_HOVER, WHITE,
};
/// Distinct accent for live-state surfaces (instances, sessions).
/// Cyan contrasts clearly with the phosphor-green config panels.
pub(super) const CYAN: ratatui::style::Color = ratatui::style::Color::Rgb(0, 180, 180);
pub(super) const CYAN_DIM: ratatui::style::Color = ratatui::style::Color::Rgb(0, 120, 120);

// ── Footer hints ───────────────────────────────────────────────────
//
// Footer hints use the shared `HintSpan` vocabulary (jackin-tui) and the
// shared host renderer in `console::widgets::hints`, so the manager footer,
// the launch cockpit, and the in-container multiplexer all read identically.
// Call sites build `Vec<HintSpan<'static>>` directly so the grouping is
// explicit, then hand it to `render_footer`. The manager footer can be long,
// so it uses the wrapped (multi-row) variant of the shared renderer.

/// How many rows the footer needs to display all `items` within `width`
/// columns. Minimum 1. Callers use this to size the footer area before layout.
#[must_use]
pub(super) fn footer_height(items: &[HintSpan<'_>], width: u16) -> u16 {
    crate::console::widgets::hints::wrapped_height(items, width)
}

pub(super) fn render_footer(frame: &mut Frame, area: Rect, items: &[HintSpan<'_>]) {
    crate::console::widgets::hints::render_wrapped(frame, area, items);
}

/// Adjust stored `scroll_y` so the cursor row stays inside the viewport.
/// Returns the effective (clamped, cursor-following) `scroll_y` to use for rendering.
pub(super) fn follow_cursor_y(
    cursor: usize,
    content_height: usize,
    viewport_h: usize,
    stored_scroll_y: u16,
) -> u16 {
    cursor_follow_offset(cursor, content_height, viewport_h, stored_scroll_y)
}

/// Adjust `scroll_y` so `cursor` stays in the editor/settings content viewport.
pub(super) fn cursor_scroll_for_panel(
    cursor: usize,
    scroll_y: u16,
    term: ratatui::layout::Rect,
) -> u16 {
    // 9 = header(3) + tab-strip(2) + block-borders(2) + footer(≈2)
    let viewport_h = (term.height.saturating_sub(9) as usize).max(1);
    // content_height - viewport_h = u16::MAX exactly: max_offset returns u16::MAX without
    // tripping its debug_assert, while the upper clamp on cursor rows stays unreachable.
    let content_height = usize::from(u16::MAX).saturating_add(viewport_h);
    follow_cursor_y(cursor, content_height, viewport_h, scroll_y)
}

#[allow(clippy::too_many_lines)]
pub fn render(
    frame: &mut Frame,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let area = frame.area();
    state.cached_term_size = area;
    if let ManagerStage::Editor(editor) = &mut state.stage {
        clamp_editor_scroll_for_frame(area, editor);
        editor::render_editor(frame, editor, config, state.op_available);
    } else if let ManagerStage::Settings(settings) = &mut state.stage {
        clamp_global_mounts_scroll_for_frame(area, &mut settings.mounts);
        global_mounts::render_settings(frame, settings, state.op_available);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // header (brand pill + 1 spacer row)
                Constraint::Min(10),   // body
                Constraint::Length(2), // footer
            ])
            .split(area);

        render_header(frame, chunks[0], "workspaces");

        if matches!(&state.stage, ManagerStage::List) {
            clamp_list_scroll_for_area(chunks[1], state, config, cwd);
            list::render_list_body(frame, chunks[1], state, config, cwd);
        }

        let footer_items: Vec<HintSpan<'static>> = match &state.stage {
            ManagerStage::List => {
                if state.inline_agent_picker.is_some() {
                    let mut items = vec![
                        HintSpan::Key("\u{2191}\u{2193}"),
                        HintSpan::Sep,
                        HintSpan::Key("Enter"),
                        HintSpan::Text("launch"),
                        HintSpan::GroupSep,
                        HintSpan::Key("Esc"),
                        HintSpan::Text("return to workspaces"),
                    ];
                    if state.list_scroll_focus.is_some() {
                        items.push(HintSpan::GroupSep);
                        items.push(HintSpan::Key("←/→"));
                        items.push(HintSpan::Text("scroll block"));
                    }
                    items
                } else if state.inline_role_picker.is_some() {
                    let mut items = vec![
                        HintSpan::Key("\u{2191}\u{2193}"),
                        HintSpan::Sep,
                        HintSpan::Key("Enter"),
                        HintSpan::Text("launch"),
                        HintSpan::GroupSep,
                        HintSpan::Key("Esc"),
                        HintSpan::Text("return to workspaces"),
                    ];
                    if state.list_scroll_focus.is_some() {
                        items.push(HintSpan::GroupSep);
                        items.push(HintSpan::Key("←/→"));
                        items.push(HintSpan::Text("scroll block"));
                    }
                    items.push(HintSpan::GroupSep);
                    items.push(HintSpan::Key("Q"));
                    items.push(HintSpan::Text("quit"));
                    items
                } else {
                    // Hidden on current-dir and "+ New workspace" rows because
                    // they have no workspace config.
                    let is_instance_row = matches!(
                        state.selected_row(),
                        ManagerListRow::WorkspaceInstance(_, _)
                            | ManagerListRow::CurrentDirectoryInstance(_)
                    );

                    if is_instance_row {
                        if state.preview_focused {
                            // Inside the preview pane: arrow navigation
                            // walks the snapshot's pane tree; Enter
                            // attaches the focused pane; Esc returns
                            // the focus to the instance row itself.
                            vec![
                                HintSpan::Key("\u{2191}\u{2193}"),
                                HintSpan::Text("navigate panes"),
                                HintSpan::Sep,
                                HintSpan::Key("Enter"),
                                HintSpan::Text("attach focused pane"),
                                HintSpan::GroupSep,
                                HintSpan::Key("Esc"),
                                HintSpan::Text("back"),
                                HintSpan::GroupSep,
                                HintSpan::Key("Q"),
                                HintSpan::Text("quit"),
                            ]
                        } else {
                            let has_snapshot = match state.selected_row() {
                                ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => state
                                    .workspace_active_instances(ws_idx)
                                    .get(inst_idx)
                                    .copied()
                                    .is_some_and(|e| {
                                        state.instance_snapshots.contains_key(&e.container_base)
                                    }),
                                ManagerListRow::CurrentDirectoryInstance(inst_idx) => state
                                    .current_dir_active_instances()
                                    .get(inst_idx)
                                    .copied()
                                    .is_some_and(|e| {
                                        state.instance_snapshots.contains_key(&e.container_base)
                                    }),
                                _ => false,
                            };
                            let mut items = vec![
                                HintSpan::Key("\u{2191}\u{2193}"),
                                HintSpan::Sep,
                                HintSpan::Key("Enter"),
                                HintSpan::Text("reconnect"),
                                HintSpan::Sep,
                                HintSpan::Key("N"),
                                HintSpan::Text("new session"),
                                HintSpan::Sep,
                                HintSpan::Key("X"),
                                HintSpan::Text("shell"),
                                HintSpan::Sep,
                                HintSpan::Key("T"),
                                HintSpan::Text("stop"),
                                HintSpan::Sep,
                                HintSpan::Key("P"),
                                HintSpan::Text("purge"),
                            ];
                            if has_snapshot {
                                items.push(HintSpan::Sep);
                                items.push(HintSpan::Key("Tab"));
                                items.push(HintSpan::Text("into preview"));
                            }
                            items.extend([
                                HintSpan::GroupSep,
                                HintSpan::Key("\u{2190}"),
                                HintSpan::Text("back"),
                                HintSpan::GroupSep,
                                HintSpan::Key("Q"),
                                HintSpan::Text("quit"),
                            ]);
                            items
                        }
                    } else {
                        let show_open_hint =
                            matches!(state.selected_row(), ManagerListRow::SavedWorkspace(_))
                                && state
                                    .selected_workspace_summary()
                                    .and_then(|s| config.workspaces.get(&s.name))
                                    .is_some_and(|ws| {
                                        !super::github_mounts::resolve_for_workspace(ws).is_empty()
                                    });

                        let is_saved =
                            matches!(state.selected_row(), ManagerListRow::SavedWorkspace(_));
                        let show_expand_hint = matches!(
                            state.selected_row(),
                            ManagerListRow::SavedWorkspace(i)
                                if !state.workspace_active_instances(i).is_empty()
                                    && !state.is_workspace_expanded(i)
                        );
                        let show_collapse_hint = matches!(
                            state.selected_row(),
                            ManagerListRow::SavedWorkspace(i)
                                if state.is_workspace_expanded(i)
                        );
                        let scroll_focused = state.list_scroll_focus.is_some();

                        let mut items: Vec<HintSpan<'static>> = if scroll_focused {
                            vec![
                                HintSpan::Key("\u{2191}\u{2193}/\u{2190}\u{2192}"),
                                HintSpan::Text("scroll block"),
                                HintSpan::GroupSep,
                                HintSpan::Key("Enter"),
                                HintSpan::Text("launch"),
                                HintSpan::GroupSep,
                            ]
                        } else {
                            vec![
                                HintSpan::Key("\u{2191}\u{2193}"),
                                HintSpan::Sep,
                                HintSpan::Key("Enter"),
                                HintSpan::Text("launch"),
                                HintSpan::GroupSep,
                            ]
                        };
                        if is_saved {
                            items.extend([
                                HintSpan::Key("E"),
                                HintSpan::Text("edit"),
                                HintSpan::Sep,
                            ]);
                        }
                        items.extend([HintSpan::Key("N"), HintSpan::Text("new")]);
                        if is_saved {
                            items.extend([
                                HintSpan::Sep,
                                HintSpan::Key("D"),
                                HintSpan::Text("delete"),
                            ]);
                        }
                        items.extend([
                            HintSpan::Sep,
                            HintSpan::Key("S"),
                            HintSpan::Text("settings"),
                        ]);
                        if show_expand_hint {
                            items.push(HintSpan::Sep);
                            items.push(HintSpan::Key("\u{2192}"));
                            items.push(HintSpan::Text("expand"));
                        }
                        if show_collapse_hint {
                            items.push(HintSpan::Sep);
                            items.push(HintSpan::Key("\u{2190}"));
                            items.push(HintSpan::Text("collapse"));
                        }
                        if show_open_hint {
                            items.push(HintSpan::Sep);
                            items.push(HintSpan::Key("O"));
                            items.push(HintSpan::Text("open in GitHub"));
                        }
                        items.push(HintSpan::GroupSep);
                        items.push(HintSpan::Key("Q"));
                        items.push(HintSpan::Text("quit"));
                        items
                    }
                }
            }
            ManagerStage::CreatePrelude(_) => vec![
                HintSpan::Dyn("Create workspace — follow the prompts".to_string()),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("cancel"),
            ],
            ManagerStage::ConfirmDelete { .. } | ManagerStage::ConfirmInstancePurge { .. } => {
                vec![
                    HintSpan::Key("Y"),
                    HintSpan::Text("yes"),
                    HintSpan::Sep,
                    HintSpan::Key("N"),
                    HintSpan::Text("no"),
                    HintSpan::GroupSep,
                    HintSpan::Key("Esc"),
                    HintSpan::Text("cancel"),
                ]
            }
            ManagerStage::Editor(_) => unreachable!("Editor has its own render path"),
            ManagerStage::Settings(_) => unreachable!("Settings has its own render path"),
        };
        render_footer(frame, chunks[2], &footer_items);
    }

    // List-anchored modal lives on `ManagerState`, not on a stage
    // variant, so the borrow splits separately from stage-anchored
    // modals.
    let is_list_stage = matches!(state.stage, ManagerStage::List);
    if is_list_stage {
        if let Some(modal) = &mut state.list_modal {
            modal::render_modal(frame, modal);
        }
    } else {
        match &mut state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &mut editor.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &mut prelude.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } => {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let modal_area = centered_rect_fixed(area, 60, 7);
                super::super::widgets::confirm::render(frame, modal_area, confirm_state);
            }
            ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } => {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = centered_rect_fixed(area, 70, 9);
                super::super::widgets::confirm::render(frame, modal_area, confirm_state);
            }
            ManagerStage::List => {
                // Handled above via the `is_list_stage` early branch.
            }
            ManagerStage::Settings(settings) => {
                if let Some(popup) = &settings.error_popup {
                    let inner_width = (area.width * 60 / 100).saturating_sub(4);
                    let max_rows = area.height.saturating_sub(2);
                    let h = crate::console::widgets::error_popup::required_height(
                        popup,
                        inner_width,
                        max_rows,
                    );
                    let popup_area = centered_rect_fixed(area, 60, h);
                    crate::console::widgets::error_popup::render(frame, popup_area, popup);
                } else if let Some(modal) = &mut settings.mounts.modal {
                    global_mounts::render_global_mount_modal(frame, modal);
                } else if let Some(modal) = &mut settings.env.modal {
                    global_mounts::render_settings_env_modal(frame, modal);
                } else if let Some(modal) = &mut settings.auth.modal {
                    global_mounts::render_settings_auth_modal(frame, modal);
                }
            }
        }
    }

    if let Some(overlay) = &state.status_overlay {
        let overlay_area = centered_rect_fixed(area, 50, 7);
        super::super::widgets::status_popup::render(frame, overlay_area, overlay);
    }
}

fn clamp_editor_scroll_for_frame(area: Rect, editor: &mut super::state::EditorState<'_>) {
    if editor.active_tab != super::state::EditorTab::Mounts {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);
    clamp_scroll_x(
        list::workspace_mounts_content_width(&editor.pending.mounts),
        scroll_viewport_width(chunks[2]),
        &mut editor.workspace_mounts_scroll_x,
    );
}

fn clamp_global_mounts_scroll_for_frame(
    area: Rect,
    global: &mut super::state::GlobalMountsState<'_>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);
    clamp_scroll_x(
        global_mounts::global_mounts_content_width(&global.pending),
        scroll_viewport_width(chunks[2]),
        &mut global.scroll_x,
    );
}

fn clamp_list_scroll_for_area(
    area: Rect,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let viewport = scroll_viewport_width(columns[1]);

    // Workspace mounts clamp — variant-specific because the source rows
    // differ (synthetic single-mount for CurrentDirectory, persisted
    // list for SavedWorkspace, no block at all for NewWorkspace /
    // WorkspaceInstance).
    match state.selected_row() {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            let cwd = cwd.display().to_string();
            let mounts = [crate::workspace::MountConfig {
                src: cwd.clone(),
                dst: cwd,
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }];
            clamp_scroll_x(
                list::workspace_mounts_content_width(&mounts),
                viewport,
                &mut state.list_mounts_scroll_x,
            );
        }
        ManagerListRow::SavedWorkspace(i) => {
            let Some(summary) = state.workspaces.get(i) else {
                return;
            };
            let Some(workspace) = config.workspaces.get(&summary.name) else {
                return;
            };
            clamp_scroll_x(
                list::workspace_mounts_content_width(&workspace.mounts),
                viewport,
                &mut state.list_mounts_scroll_x,
            );
        }
        ManagerListRow::NewWorkspace | ManagerListRow::WorkspaceInstance(_, _) => {
            state.list_mounts_scroll_x = 0;
        }
    }

    // CurrentDirectory and SavedWorkspace must agree via
    // `global_rows_for_selected_row` on whether the global-mounts block
    // is present and what it contains, so horizontal scroll state
    // survives row switches between them.
    let global_rows = global_rows_for_selected_row(state, config);
    if global_rows.is_empty() {
        state.list_global_mounts_scroll_x = 0;
        state.list_role_global_mounts_scroll_x = 0;
    } else {
        let (global, scoped) = partition_mounts_by_scope(&global_rows);
        clamp_scroll_x(
            list::global_mounts_content_width(&global),
            viewport,
            &mut state.list_global_mounts_scroll_x,
        );
        clamp_scroll_x(
            list::global_mounts_content_width(&scoped),
            viewport,
            &mut state.list_role_global_mounts_scroll_x,
        );
    }

    // Fix 1: Clear stale scroll focus when the focused block no longer
    // overflows after a terminal resize. Checked every render frame so the
    // green border disappears as soon as the content fits in the viewport.
    if state
        .list_scroll_focus
        .is_some_and(|f| !focused_block_still_scrollable(f, columns[1], state, config, cwd))
    {
        state.list_scroll_focus = None;
    }

    // Clamp left-pane name scroll to valid range.
    let left_viewport_w = scroll_viewport_width(columns[0]);
    if left_viewport_w == 0 {
        state.list_names_scroll_x = 0;
    } else {
        let name_content_w = list::list_names_content_width(state, left_viewport_w);
        if is_scrollable(name_content_w, left_viewport_w) {
            let max = max_scroll_offset(name_content_w, left_viewport_w);
            if state.list_names_scroll_x > max {
                state.list_names_scroll_x = max;
            }
        } else {
            state.list_names_scroll_x = 0;
            state.list_names_focused = false;
        }
    }
}

fn workspace_mounts_scrollable(
    mounts: &[crate::workspace::MountConfig],
    viewport_w: usize,
) -> bool {
    let w = list::workspace_mounts_content_width(mounts);
    let content_h = list::workspace_mounts_content_height(mounts);
    let viewport_h = scroll_viewport_height(Rect {
        x: 0,
        y: 0,
        width: 0,
        height: list::mount_block_height(mounts),
    });
    is_scrollable(w, viewport_w) || is_scrollable(content_h, viewport_h)
}

/// Returns `true` when the focused block still overflows the right pane
/// (either horizontally or vertically) after a resize. Used to clear
/// `list_scroll_focus` when the terminal grows large enough that the
/// content fits without scrolling.
fn focused_block_still_scrollable(
    focus: super::state::MountScrollFocus,
    right_pane: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> bool {
    use super::state::{ManagerListRow, MountScrollFocus};
    let viewport_w = scroll_viewport_width(right_pane);

    match focus {
        MountScrollFocus::Workspace => match state.selected_row() {
            ManagerListRow::CurrentDirectory => {
                let cwd_str = cwd.display().to_string();
                let m = crate::workspace::MountConfig {
                    src: cwd_str.clone(),
                    dst: cwd_str,
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                };
                workspace_mounts_scrollable(std::slice::from_ref(&m), viewport_w)
            }
            ManagerListRow::SavedWorkspace(i) => {
                let Some(s) = state.workspaces.get(i) else {
                    return false;
                };
                let Some(ws) = config.workspaces.get(&s.name) else {
                    return false;
                };
                workspace_mounts_scrollable(ws.mounts.as_slice(), viewport_w)
            }
            ManagerListRow::NewWorkspace
            | ManagerListRow::WorkspaceInstance(_, _)
            | ManagerListRow::CurrentDirectoryInstance(_) => false,
        },
        MountScrollFocus::Global | MountScrollFocus::RoleGlobal => {
            // Any row the render path populates must be scrollability-
            // evaluated here, otherwise `list_scroll_focus` clears on
            // every resize tick. Shared source of truth with
            // `clamp_list_scroll_for_area` and `sidebar_inputs_for_*`.
            let global_rows = global_rows_for_selected_row(state, config);
            if global_rows.is_empty() {
                return false;
            }
            let (global, scoped) = partition_mounts_by_scope(&global_rows);
            let mounts = match focus {
                MountScrollFocus::Global => global,
                MountScrollFocus::RoleGlobal => scoped,
                MountScrollFocus::Workspace | MountScrollFocus::Roles => unreachable!(),
            };
            if mounts.is_empty() {
                return false;
            }
            let global_w = list::global_mounts_content_width(mounts.as_slice());
            let global_h = list::global_mounts_content_height(mounts.as_slice());
            let block_h = list::global_mounts_block_height(mounts.as_slice()) as usize;
            let viewport_h = scroll_viewport_height(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: block_h as u16,
            });
            is_scrollable(global_w, viewport_w) || is_scrollable(global_h, viewport_h)
        }
        MountScrollFocus::Roles => {
            let ws_config = match state.selected_row() {
                ManagerListRow::SavedWorkspace(i) => state
                    .workspaces
                    .get(i)
                    .and_then(|s| config.workspaces.get(&s.name)),
                ManagerListRow::CurrentDirectory
                | ManagerListRow::CurrentDirectoryInstance(_)
                | ManagerListRow::NewWorkspace
                | ManagerListRow::WorkspaceInstance(_, _) => None,
            };
            let agent_count = list::agents_block_agent_count(ws_config, config);
            let roles_w = list::agents_block_content_width(ws_config, config);
            let roles_h = 2 + agent_count;
            let block_h = list::agents_block_height(agent_count) as usize;
            let viewport_h = scroll_viewport_height(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: block_h as u16,
            });
            is_scrollable(roles_w, viewport_w) || is_scrollable(roles_h, viewport_h)
        }
    }
}

/// Picker-role resolution shared by every render path that builds
/// global-mount rows. Both the inline role picker (operator currently
/// scrolling a role list) and the inline agent picker (operator
/// drilling into a role's agents) advertise a role; either gives the
/// per-role overlay for the global-mounts block. Returning `None` is
/// the unscoped baseline — the case both "no picker active" and "current
/// directory selected (no saved role binding)" reduce to.
pub(super) fn picker_role_from_state(
    state: &ManagerState<'_>,
) -> Option<crate::selector::RoleSelector> {
    state
        .inline_role_picker
        .as_ref()
        .and_then(|picker| {
            picker
                .list_state
                .selected
                .and_then(|idx| picker.filtered.get(idx).cloned())
        })
        .or_else(|| {
            state
                .inline_agent_picker
                .as_ref()
                .map(|(role, _)| role.clone())
        })
}

/// Global mount rows for whatever row the operator currently has
/// selected on the workspace list. Single source of truth so the render
/// side, the scroll-clamp, and the focused-block-scrollable check
/// always agree on what the rendered block is showing. Returning an
/// empty `Vec` matches "no global-mount block visible right now."
///
/// `CurrentDirectory` and `CurrentDirectoryInstance` reduce to the
/// unscoped baseline because the synthetic current-dir workspace has
/// no role binding — same rule `sidebar_inputs_for_current_dir`
/// applies. `SavedWorkspace` adds the role-scoped overlay when a picker
/// is active. `NewWorkspace` and `WorkspaceInstance` have no global block.
pub(super) fn global_rows_for_selected_row(
    state: &ManagerState<'_>,
    config: &AppConfig,
) -> Vec<crate::config::GlobalMountRow> {
    use super::state::ManagerListRow;
    match state.selected_row() {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            global_rows_for(config, None)
        }
        ManagerListRow::SavedWorkspace(i) => {
            let Some(summary) = state.workspaces.get(i) else {
                return Vec::new();
            };
            if !config.workspaces.contains_key(&summary.name) {
                return Vec::new();
            }
            global_rows_for(config, picker_role_from_state(state).as_ref())
        }
        ManagerListRow::NewWorkspace | ManagerListRow::WorkspaceInstance(_, _) => Vec::new(),
    }
}

/// `None` role → unscoped rows only; `Some(role)` → merged scoped + unscoped.
pub(super) fn global_rows_for(
    config: &AppConfig,
    picker_role: Option<&crate::selector::RoleSelector>,
) -> Vec<crate::config::GlobalMountRow> {
    picker_role.map_or_else(
        || {
            config
                .list_mount_rows()
                .into_iter()
                .filter(|row| row.scope.is_none())
                .collect()
        },
        |role| config.resolve_mount_rows(role),
    )
}

pub(super) fn partition_mounts_by_scope(
    rows: &[crate::config::GlobalMountRow],
) -> (
    Vec<crate::workspace::MountConfig>,
    Vec<crate::workspace::MountConfig>,
) {
    let mut global = Vec::new();
    let mut scoped = Vec::new();
    for row in rows {
        if row.scope.is_none() {
            global.push(row.mount.clone());
        } else {
            scoped.push(row.mount.clone());
        }
    }
    (global, scoped)
}

pub(super) fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    crate::console::widgets::render_brand_header(frame, area, title);
}

/// Like `centered_rect` but takes a fixed number of rows for the height.
/// `pct_w` is still a percentage of the outer width. Rows are clamped to fit.
pub(super) fn centered_rect_fixed(outer: Rect, pct_w: u16, rows: u16) -> Rect {
    let w = outer.width * pct_w / 100;
    let h = rows.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod header_branding_tests {
    //! Pins the product-name rendering convention: the top-of-screen
    //! header must display the name as lowercase + trailing apostrophe
    //! (`jackin'`) in every user-facing string. All-caps `JACKIN` and
    //! apostrophe-less `jackin` are both disallowed for display text —
    //! though `jackin` without an apostrophe still appears in CLI-command
    //! references rendered in backticks (e.g. `` `jackin console` ``), in
    //! filesystem paths like `~/.jackin/`, and in URLs, all of which are
    //! intentionally exempt and not audited here.
    use super::render_header;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    #[test]
    fn tui_header_uses_lowercase_jackin_with_apostrophe() {
        let backend = TestBackend::new(40, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_header(f, Rect::new(0, 0, 40, 1), "workspaces");
        })
        .unwrap();

        let buf = term.backend().buffer();
        let dump: String = buf
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

        assert!(
            dump.contains("jackin'"),
            "header must render 'jackin'' (lowercase + trailing apostrophe); got {dump:?}"
        );
        assert!(
            !dump.contains("JACKIN"),
            "header must not render 'JACKIN' (uppercase); got {dump:?}"
        );
    }
}
