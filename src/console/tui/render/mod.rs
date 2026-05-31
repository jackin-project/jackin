//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::config::AppConfig;
use crate::console::manager::mount_display::settings_global_mounts_content_width_with_cache;
use crate::console::manager::state::{ManagerListRow, ManagerStage, ManagerState};
use jackin_tui::HintSpan;

pub mod editor;
pub(crate) mod global_mounts;
pub(crate) mod list;
pub(crate) mod modal;
#[cfg(test)]
mod snapshot_tests;

// Modal dismissal sequencing requires a render call across a module boundary;
// re-exported here so input handlers can reach render_editor directly.
pub use editor::render_editor;
pub(super) use jackin_console::layout::centered_rect_fixed;

pub(super) use crate::console::widgets::{
    PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, TAB_BG_INACTIVE_HOVER, WHITE,
};
pub(crate) use jackin_tui::components::scrollable_panel::{
    clamp_scroll_offset as clamp_scroll_x, is_scrollable, max_offset as max_scroll_offset,
    viewport_height as scroll_viewport_height, viewport_width as scroll_viewport_width,
};
pub(super) use jackin_tui::components::scrollable_panel::{
    line_width, max_line_width, render_horizontal_scrollbar, render_line_with_fixed_prefix_scroll,
    render_scrollable_block_at, render_vertical_scrollbar,
};
pub(super) use jackin_tui::theme::{CYAN, CYAN_DIM};

// ── Footer hints ───────────────────────────────────────────────────
//
// Footer hints use the shared `HintSpan` vocabulary (jackin-tui) and the
// shared `jackin_tui::components` renderers, so the manager footer, the launch
// cockpit, and the in-container multiplexer all read identically.
// Call sites build `Vec<HintSpan<'static>>` directly so the grouping is
// explicit, then hand it to `render_footer`. The manager footer can be long,
// so it uses the wrapped (multi-row) variant of the shared renderer.

/// How many rows the footer needs to display all `items` within `width`
/// columns. Minimum 1. Callers use this to size the footer area before layout.
#[must_use]
pub(super) fn footer_height(items: &[HintSpan<'_>], width: u16) -> u16 {
    jackin_tui::components::wrapped_height(items, width)
}

pub(super) fn render_footer(frame: &mut Frame, area: Rect, items: &[HintSpan<'_>]) {
    jackin_tui::components::render_wrapped_hint_bar(frame, area, items);
}

#[doc(hidden)]
pub fn prepare_for_render(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) {
    state.cached_term_size = area;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            let footer = editor::editor_footer_items(editor, config, state.op_available);
            editor.cached_footer_h = footer_height(&footer, area.width).max(1);
            editor::prepare_editor_for_render(area, editor, config);
        }
        ManagerStage::Settings(settings) => {
            let footer = global_mounts::settings_footer_items(settings, state.op_available);
            settings.cached_footer_h = footer_height(&footer, area.width).max(1);
            clamp_global_mounts_scroll_for_frame(area, &mut settings.mounts);
        }
        ManagerStage::List => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Min(10),
                    Constraint::Length(2),
                ])
                .split(area);
            clamp_list_scroll_for_area(chunks[1], state, config, cwd);
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    prepare_visible_modal(area, state);
}

fn prepare_visible_modal(area: Rect, state: &mut ManagerState<'_>) {
    if let Some(modal) = &mut state.list_modal {
        modal::prepare_modal(area, modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = &mut editor.modal {
                modal::prepare_modal(area, modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = &mut prelude.modal {
                modal::prepare_modal(area, modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(crate::console::manager::state::GlobalMountModal::PreviewSave { state }) =
                &mut settings.mounts.modal
            {
                use crate::console::widgets::confirm_save;
                let height = confirm_save::required_height(state).min(area.height);
                let modal_area = centered_rect_fixed(area, 80, height);
                confirm_save::prepare_for_render(modal_area, state);
            }
            if let Some(crate::console::manager::state::SettingsEnvModal::OpPicker { state }) =
                &mut settings.env.modal
            {
                state.tick();
            }
            if let Some(crate::console::manager::state::SettingsAuthModal::OpPicker { state }) =
                &mut settings.auth.modal
            {
                state.tick();
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
}

#[allow(clippy::too_many_lines)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    if let ManagerStage::Editor(editor) = &state.stage {
        editor::render_editor(frame, area, editor, config, state.op_available);
    } else if let ManagerStage::Settings(settings) = &state.stage {
        global_mounts::render_settings(frame, area, settings, state.op_available);
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
            list::render_list_body(frame, chunks[1], state, config, cwd);
        }

        let footer_items: Vec<HintSpan<'static>> = match &state.stage {
            ManagerStage::List => {
                let picker_footer = || {
                    let mut items = vec![
                        HintSpan::Key("\u{2191}\u{2193}"),
                        HintSpan::Sep,
                        HintSpan::Key("↵"),
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
                };
                if state.inline_agent_picker.is_some() {
                    picker_footer()
                } else if state.inline_role_picker.is_some() {
                    // The role picker can quit the app; the agent picker is
                    // reached mid-flow and only returns to workspaces.
                    let mut items = picker_footer();
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
                                HintSpan::Key("↵"),
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
                                HintSpan::Key("↵"),
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
                                items.push(HintSpan::Key("⇥"));
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
                        let show_open_hint = matches!(
                            state.selected_row(),
                            ManagerListRow::SavedWorkspace(_)
                        ) && state
                            .selected_workspace_summary()
                            .and_then(|s| config.workspaces.get(&s.name))
                            .is_some_and(|ws| {
                                !crate::console::manager::github_mounts::resolve_for_workspace(ws)
                                    .is_empty()
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

                        let enter_label =
                            if matches!(state.selected_row(), ManagerListRow::NewWorkspace) {
                                "setup"
                            } else {
                                "launch"
                            };

                        let mut items: Vec<HintSpan<'static>> = if scroll_focused {
                            vec![
                                HintSpan::Key("\u{2191}\u{2193}/\u{2190}\u{2192}"),
                                HintSpan::Text("scroll block"),
                                HintSpan::GroupSep,
                                HintSpan::Key("↵"),
                                HintSpan::Text(enter_label),
                                HintSpan::GroupSep,
                            ]
                        } else {
                            vec![
                                HintSpan::Key("\u{2191}\u{2193}"),
                                HintSpan::Sep,
                                HintSpan::Key("↵"),
                                HintSpan::Text(enter_label),
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
        if let Some(modal) = &state.list_modal {
            modal::render_modal(frame, modal);
        }
    } else {
        match &state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &editor.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &prelude.modal {
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
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::ConfirmInstancePurge {
                state: confirm_state,
                ..
            } => {
                // The two-line prompt is taller than ConfirmDelete's
                // single line, so allocate more rows for the modal.
                let modal_area = centered_rect_fixed(area, 70, 9);
                jackin_tui::components::render_confirm_dialog(frame, modal_area, confirm_state);
            }
            ManagerStage::List => {
                // Handled above via the `is_list_stage` early branch.
            }
            ManagerStage::Settings(settings) => {
                if let Some(popup) = &settings.error_popup {
                    let inner_width = (area.width * 60 / 100).saturating_sub(4);
                    let max_rows = area.height.saturating_sub(2);
                    let h = jackin_tui::components::error_dialog::required_height(
                        popup,
                        inner_width,
                        max_rows,
                    );
                    let popup_area = centered_rect_fixed(area, 60, h);
                    jackin_tui::components::render_error_dialog(frame, popup_area, popup);
                } else if let Some(modal) = &settings.mounts.modal {
                    global_mounts::render_global_mount_modal(frame, modal);
                } else if let Some(modal) = &settings.env.modal {
                    global_mounts::render_settings_env_modal(frame, modal);
                } else if let Some(modal) = &settings.auth.modal {
                    global_mounts::render_settings_auth_modal(frame, modal);
                }
            }
        }
    }

    if let Some(overlay) = &state.status_overlay {
        let overlay_area = centered_rect_fixed(area, 50, 7);
        jackin_tui::components::render_status_popup(frame, overlay_area, overlay);
    }
}

fn clamp_global_mounts_scroll_for_frame(
    area: Rect,
    global: &mut crate::console::manager::state::GlobalMountsState<'_>,
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
        settings_global_mounts_content_width_with_cache(&global.pending, &global.mount_info_cache),
        scroll_viewport_width(chunks[2]),
        &mut global.scroll_x,
    );
}

#[allow(clippy::too_many_lines)]
pub(crate) fn clamp_list_scroll_for_area(
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
    let sidebar_areas = selected_sidebar_scroll_areas(columns[1], state, config, cwd);

    if let Some(areas) = sidebar_areas.as_ref() {
        clamp_scroll_area(areas.workspace, &mut state.list_mounts_scroll_x);
        clamp_scroll_area_y(areas.workspace, &mut state.list_mounts_scroll_y);
        clamp_scroll_area(areas.global, &mut state.list_global_mounts_scroll_x);
        clamp_scroll_area_y(areas.global, &mut state.list_global_mounts_scroll_y);

        if let Some(role_global) = areas.role_global {
            clamp_scroll_area(role_global, &mut state.list_role_global_mounts_scroll_x);
            clamp_scroll_area_y(role_global, &mut state.list_role_global_mounts_scroll_y);
        } else {
            state.list_role_global_mounts_scroll_x = 0;
            state.list_role_global_mounts_scroll_y = 0;
        }

        if let Some(roles) = areas.roles {
            clamp_scroll_area(roles, &mut state.list_roles_scroll_x);
            clamp_scroll_area_y(roles, &mut state.list_roles_scroll_y);
        } else {
            state.list_roles_scroll_x = 0;
            state.list_roles_scroll_y = 0;
        }
    } else {
        state.list_mounts_scroll_x = 0;
        state.list_mounts_scroll_y = 0;
        state.list_global_mounts_scroll_x = 0;
        state.list_global_mounts_scroll_y = 0;
        state.list_role_global_mounts_scroll_x = 0;
        state.list_role_global_mounts_scroll_y = 0;
        state.list_roles_scroll_x = 0;
        state.list_roles_scroll_y = 0;
        state.list_scroll_focus = None;
        if !state.preview_focused {
            state.list_names_focused = true;
        }
    }

    // Fix 1: Clear stale scroll focus when the focused block no longer
    // overflows after a terminal resize. Checked every render frame so the
    // green border disappears as soon as the content fits in the viewport.
    if let Some(focus) = state.list_scroll_focus
        && !focused_block_still_scrollable(focus, sidebar_areas.as_ref())
    {
        state.list_scroll_focus = None;
        state.list_names_focused = true;
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
        }
    }
}

fn selected_sidebar_scroll_areas(
    right_pane: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> Option<list::SidebarScrollAreas> {
    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            let cwd_str = cwd.display().to_string();
            let mounts = [crate::workspace::MountConfig {
                src: cwd_str.clone(),
                dst: cwd_str.clone(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }];
            let inputs = list::sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
            Some(list::compute_sidebar_scroll_areas(
                right_pane, &inputs, config,
            ))
        }
        ManagerListRow::SavedWorkspace(i) => {
            let summary = state.workspaces.get(i).cloned()?;
            config.workspaces.get(&summary.name)?;
            let inputs = list::sidebar_inputs_for_workspace(&summary, config, state);
            Some(list::compute_sidebar_scroll_areas(
                right_pane, &inputs, config,
            ))
        }
        ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::CurrentDirectoryInstance(_) => None,
    }
}

const fn clamp_scroll_area(area: list::SidebarScrollArea, value: &mut u16) {
    clamp_scroll_x(area.content_width, scroll_viewport_width(area.area), value);
}

const fn clamp_scroll_area_y(area: list::SidebarScrollArea, value: &mut u16) {
    clamp_scroll_x(
        area.content_height,
        scroll_viewport_height(area.area),
        value,
    );
}

const fn scroll_area_scrollable(area: list::SidebarScrollArea) -> bool {
    is_scrollable(area.content_width, scroll_viewport_width(area.area))
        || is_scrollable(area.content_height, scroll_viewport_height(area.area))
}

/// Returns `true` when the focused block still overflows the right pane
/// (either horizontally or vertically) after a resize. Used to clear
/// `list_scroll_focus` when the terminal grows large enough that the
/// content fits without scrolling.
fn focused_block_still_scrollable(
    focus: crate::console::manager::state::MountScrollFocus,
    areas: Option<&list::SidebarScrollAreas>,
) -> bool {
    let Some(areas) = areas else {
        return false;
    };
    match focus {
        crate::console::manager::state::MountScrollFocus::Workspace => {
            scroll_area_scrollable(areas.workspace)
        }
        crate::console::manager::state::MountScrollFocus::Global => {
            areas.global.area.height > 0 && scroll_area_scrollable(areas.global)
        }
        crate::console::manager::state::MountScrollFocus::RoleGlobal => {
            areas.role_global.is_some_and(scroll_area_scrollable)
        }
        crate::console::manager::state::MountScrollFocus::Roles => {
            areas.roles.is_some_and(scroll_area_scrollable)
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
    use crate::console::manager::state::ManagerListRow;
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

pub(super) fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    jackin_tui::components::render_brand_header(frame, area, title);
}

#[cfg(test)]
mod list_scroll_clamp_tests {
    use super::{
        clamp_list_scroll_for_area, max_scroll_offset, scroll_viewport_height,
        selected_sidebar_scroll_areas,
    };
    use crate::config::AppConfig;
    use crate::console::manager::state::ManagerState;
    use crate::isolation::MountIsolation;
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use ratatui::layout::{Constraint, Direction, Layout, Rect};

    fn split_mount(idx: usize) -> MountConfig {
        MountConfig {
            src: format!("/host/long/source/path/{idx}"),
            dst: format!("/container/long/destination/path/{idx}"),
            readonly: false,
            isolation: MountIsolation::Shared,
        }
    }

    #[test]
    fn list_vertical_clamp_uses_rendered_sidebar_height() {
        let mut config = AppConfig::default();
        config.workspaces.insert(
            "demo".into(),
            WorkspaceConfig {
                workdir: "/workspace/demo".into(),
                mounts: (0..10).map(split_mount).collect(),
                ..Default::default()
            },
        );
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 1;

        let body = Rect::new(0, 0, 100, 10);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(state.list_split_pct),
                Constraint::Percentage(100u16.saturating_sub(state.list_split_pct)),
            ])
            .split(body);
        let areas = selected_sidebar_scroll_areas(columns[1], &state, &config, tmp.path()).unwrap();
        let rendered_viewport = scroll_viewport_height(areas.workspace.area);
        let desired_viewport = scroll_viewport_height(Rect::new(0, 0, 0, 12));
        assert!(rendered_viewport < desired_viewport);

        let expected = max_scroll_offset(areas.workspace.content_height, rendered_viewport);
        assert!(expected > max_scroll_offset(areas.workspace.content_height, desired_viewport));

        state.list_mounts_scroll_y = u16::MAX;
        clamp_list_scroll_for_area(body, &mut state, &config, tmp.path());

        assert_eq!(state.list_mounts_scroll_y, expected);
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
