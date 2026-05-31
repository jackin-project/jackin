//! List-stage rendering: the left-column workspace list and right-pane
//! details (saved workspace / current-directory / "+ New workspace"
//! sentinel).
#![expect(
    clippy::redundant_pub_crate,
    reason = "manager update code uses selected render geometry helpers through the moved tui facade"
)]
#![expect(
    clippy::too_many_lines,
    reason = "Phase 9 only moves render files; later component splits own shortening these helpers"
)]
#![expect(
    clippy::too_many_arguments,
    reason = "existing row-builder shape is preserved during the directory migration"
)]
#![expect(
    clippy::fn_params_excessive_bools,
    reason = "existing row-builder shape is preserved during the directory migration"
)]

use jackin_tui::components::{Panel, PanelFocus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::{
    CYAN, CYAN_DIM, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, TAB_BG_INACTIVE_HOVER, WHITE,
};
use crate::config::AppConfig;
use crate::console::manager::list_geometry::{
    SidebarInputs, SidebarLayout, compute_sidebar_layout, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace, split_global_mount_rows,
};
#[cfg(test)]
pub(super) use crate::console::manager::list_geometry::{
    global_mounts_content_height, mount_block_height,
};
#[cfg(test)]
pub(super) use crate::console::manager::mount_display::format_mount_rows;
pub(super) use crate::console::manager::mount_display::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, MountDisplayRow, format_mount_rows_with_cache,
    mount_path_width, workspace_mounts_content_width_with_cache,
};
use crate::console::manager::state::{
    ManagerListRow, ManagerState, MountInfoCache, MountScrollFocus, WorkspaceSummary,
};

#[allow(clippy::too_many_lines)]
pub(super) fn render_list_body(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // See ManagerListRow docs for row layout.
    // Split driven by `state.list_split_pct` (default 30), adjustable via
    // mouse-drag on the seam column. Keeps the right pane visible on every
    // row. Row-specific right-pane renderers:
    //   CurrentDirectory  → current-dir details
    //   SavedWorkspace(i) → saved-workspace details
    //   NewWorkspace      → description-of-what-a-workspace-is pane
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let list_area = columns[0];

    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            render_current_dir_details_pane(frame, columns[1], cwd, config, state);
        }
        ManagerListRow::NewWorkspace => {
            render_sentinel_description_pane(frame, columns[1]);
        }
        ManagerListRow::SavedWorkspace(i) => {
            if let Some(ws) = state.workspaces.get(i).cloned() {
                render_details_pane(frame, columns[1], i, &ws, config, state);
            }
        }
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => {
            let instances = state.workspace_active_instances(ws_idx);
            if let Some(entry) = instances.get(inst_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns[1],
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
            let instances = state.current_dir_active_instances();
            if let Some(entry) = instances.get(inst_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns[1],
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
    }

    if let Some(picker) = state.inline_provider_picker.as_ref() {
        let short_id = crate::instance::naming::instance_id_from_container_base(&picker.context)
            .unwrap_or(picker.context.as_str());
        render_provider_picker_sidebar(
            frame,
            list_area,
            Some(short_id),
            picker.providers(),
            picker.selected(),
        );
    } else if let Some(picker) = state.launch_provider_picker.as_ref() {
        render_provider_picker_sidebar(
            frame,
            list_area,
            None,
            picker.providers(),
            picker.selected(),
        );
    } else if let Some((container, picker, _providers)) = state.inline_new_session_picker.as_ref() {
        let short_id = crate::instance::naming::instance_id_from_container_base(container)
            .unwrap_or(container);
        render_agent_picker_sidebar(frame, list_area, short_id, picker, state.list_names_focused);
    } else if let Some((role, picker)) = state.inline_agent_picker.as_ref() {
        render_agent_picker_sidebar(
            frame,
            list_area,
            &role.key(),
            picker,
            state.list_names_focused,
        );
    } else if let Some(picker) = state.inline_role_picker.as_ref() {
        let title = state
            .selected_workspace_summary()
            .map_or("Current directory", |summary| summary.name.as_str());
        render_role_picker_sidebar(frame, list_area, title, picker, state.list_names_focused);
    } else {
        let (list_lines, content_width) =
            list_name_lines(state, super::scroll_viewport_width(list_area));
        render_list_names_block(frame, list_area, list_lines, content_width, state);
    }
}

fn list_name_lines(state: &ManagerState<'_>, viewport: usize) -> (Vec<Line<'static>>, usize) {
    let visual_rows = state.visual_rows_vec();
    let visual_selected = state.visual_selected();
    let mut max_w = viewport;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(visual_rows.len());

    for visual_row in &visual_rows {
        let Some(row) = visual_row else {
            // Non-selectable spacer before "+ New workspace".
            lines.push(Line::from(""));
            continue;
        };
        let is_selected = lines.len() == visual_selected;

        match row {
            ManagerListRow::CurrentDirectory => {
                push_tree_workspace_line(
                    &mut lines,
                    "Current directory",
                    is_selected,
                    state.list_names_focused,
                    WHITE,
                    state.current_dir_expanded,
                    state.has_current_dir_active_instances(),
                    &mut max_w,
                );
            }
            ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
                let instances = state.current_dir_active_instances();
                if let Some(entry) = instances.get(*inst_idx) {
                    push_tree_instance_line(
                        &mut lines,
                        entry,
                        is_selected,
                        state.list_names_focused,
                        &mut max_w,
                    );
                }
            }
            ManagerListRow::SavedWorkspace(i) => {
                let ws = &state.workspaces[*i];
                let expanded = state.is_workspace_expanded(*i);
                let has_instances = state.has_active_instances(*i);
                push_tree_workspace_line(
                    &mut lines,
                    &ws.name,
                    is_selected,
                    state.list_names_focused,
                    PHOSPHOR_GREEN,
                    expanded,
                    has_instances,
                    &mut max_w,
                );
            }
            ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => {
                let instances = state.workspace_active_instances(*ws_idx);
                if let Some(entry) = instances.get(*inst_idx) {
                    push_tree_instance_line(
                        &mut lines,
                        entry,
                        is_selected,
                        state.list_names_focused,
                        &mut max_w,
                    );
                }
            }
            ManagerListRow::NewWorkspace => {
                push_tree_workspace_line(
                    &mut lines,
                    "+ New workspace",
                    is_selected,
                    state.list_names_focused,
                    WHITE,
                    false,
                    false,
                    &mut max_w,
                );
            }
        }
    }

    // Compute scroll range before selected/hover background fill pads rows to
    // the viewport. Highlight padding is visual only; it must not make a
    // content-fitting list become horizontally scrollable.
    let content_w = super::max_line_width(&lines).max(max_w);

    // Extend the selected row's highlight to fill the content width.
    if let Some(line) = lines.get_mut(visual_selected) {
        let current_w = super::line_width(line);
        if current_w < content_w {
            let bg = if matches!(
                visual_rows.get(visual_selected),
                Some(Some(
                    ManagerListRow::WorkspaceInstance(_, _)
                        | ManagerListRow::CurrentDirectoryInstance(_)
                ))
            ) {
                CYAN
            } else {
                PHOSPHOR_GREEN
            };
            line.spans.push(Span::styled(
                " ".repeat(content_w - current_w),
                Style::default().bg(bg).fg(Color::Black),
            ));
        }
    }

    // Hover lift: the row under the pointer (when not the selected row) gets a
    // subtle graphite background — the same "this is clickable" cue the tab
    // strip uses. Selected wins, so a hovered-and-selected row keeps its strong
    // highlight.
    if let Some(hovered) = state.hovered_list_row
        && let Some(h) = visual_rows.iter().position(|r| *r == Some(hovered))
        && h != visual_selected
        && let Some(line) = lines.get_mut(h)
    {
        for span in &mut line.spans {
            span.style = span.style.bg(TAB_BG_INACTIVE_HOVER);
        }
        let current_w = super::line_width(line);
        if current_w < content_w {
            line.spans.push(Span::styled(
                " ".repeat(content_w - current_w),
                Style::default().bg(TAB_BG_INACTIVE_HOVER),
            ));
        }
    }

    (lines, content_w)
}

fn render_list_names_block(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    content_width: usize,
    state: &ManagerState<'_>,
) {
    let content_height = lines.len();
    let viewport_w = super::scroll_viewport_width(area);
    let viewport_h = super::scroll_viewport_height(area);
    let h_scrollable = super::is_scrollable(content_width, viewport_w);
    let v_scrollable = super::is_scrollable(content_height, viewport_h);
    // Focused → PHOSPHOR_GREEN regardless of scrollability (RULE 1: focus-visible border).
    let block = Panel::new()
        .focus(if state.list_names_focused {
            PanelFocus::Focused
        } else {
            PanelFocus::Unfocused
        })
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_rows = usize::from(inner.height).min(content_height);
    for (row_idx, line) in lines.into_iter().take(visible_rows).enumerate() {
        render_list_name_line(
            frame,
            inner,
            row_idx as u16,
            line,
            usize::from(state.list_names_scroll_x),
        );
    }
    if h_scrollable {
        super::render_horizontal_scrollbar(frame, area, content_width, state.list_names_scroll_x);
    }
    if v_scrollable {
        super::render_vertical_scrollbar(frame, area, content_height, 0);
    }
}

fn render_list_name_line(
    frame: &mut Frame,
    area: Rect,
    row: u16,
    line: Line<'static>,
    scroll_x: usize,
) {
    const PREFIX_COLS: usize = 3;
    super::render_line_with_fixed_prefix_scroll(frame, area, row, line, PREFIX_COLS, scroll_x);
}

/// Workspace / sentinel row. Shows `▶`/`▼` disclosure arrow only when the
/// workspace has active instances; rows without instances show no indicator.
fn push_tree_workspace_line(
    lines: &mut Vec<Line<'static>>,
    name: &str,
    selected: bool,
    show_cursor: bool,
    color: Color,
    expanded: bool,
    has_instances: bool,
    max_w: &mut usize,
) {
    let cursor = if selected && show_cursor { "▸" } else { " " };
    // Build line as separate spans so line_width measures display columns
    // correctly for the ▶/▼ glyphs (same approach as the editor render).
    let line = if has_instances {
        let arrow = if expanded { "▼" } else { "▶" };
        let text_w = 1 + 1 + 1 + jackin_tui::display_cols(name);
        *max_w = (*max_w).max(text_w);
        if selected {
            Line::from(vec![
                Span::styled(cursor, Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black)),
                Span::styled(arrow, Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black)),
                Span::styled(
                    format!(" {name}"),
                    Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(cursor, Style::default().fg(color)),
                Span::styled(arrow, Style::default().fg(color)),
                Span::styled(format!(" {name}"), Style::default().fg(color)),
            ])
        }
    } else {
        // Two-space placeholder aligns name column with arrow-rows (cursor+arrow+space = 3).
        let text_w = 3 + jackin_tui::display_cols(name);
        *max_w = (*max_w).max(text_w);
        if selected {
            Line::from(Span::styled(
                format!("{cursor}  {name}"),
                Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black),
            ))
        } else {
            Line::from(Span::styled(
                format!("{cursor}  {name}"),
                Style::default().fg(color),
            ))
        }
    };
    lines.push(line);
}

/// Indented instance row — shows `instance_id` and `role` only; agent and
/// status are visible in the right-panel detail pane when the row is selected.
fn push_tree_instance_line(
    lines: &mut Vec<Line<'static>>,
    entry: &crate::instance::InstanceIndexEntry,
    selected: bool,
    show_cursor: bool,
    max_w: &mut usize,
) {
    let cursor = if selected && show_cursor { "▸" } else { " " };
    let label = format!("{}  {}", entry.instance_id, entry.role_key);
    let text_w = 1 + 4 + jackin_tui::display_cols(&label);
    *max_w = (*max_w).max(text_w);

    let line = if selected {
        Line::from(Span::styled(
            format!("{cursor}    {label}"),
            Style::default().bg(CYAN).fg(Color::Black),
        ))
    } else {
        Line::from(vec![
            Span::styled(format!("{cursor}    "), Style::default().fg(CYAN_DIM)),
            Span::styled(entry.instance_id.clone(), Style::default().fg(CYAN_DIM)),
            Span::styled("  ", Style::default()),
            Span::styled(entry.role_key.clone(), Style::default().fg(CYAN)),
        ])
    };
    lines.push(line);
}

fn render_provider_picker_sidebar(
    frame: &mut Frame,
    area: Rect,
    container_id: Option<&str>,
    providers: &[jackin_protocol::Provider],
    selected: usize,
) {
    let title = provider_picker_title(container_id);
    let block = Panel::new()
        .title(&title)
        .focus(PanelFocus::Unfocused)
        .block();
    let items: Vec<ListItem> = providers
        .iter()
        .map(|provider| ListItem::new(Line::from(provider.label())))
        .collect();
    let list = List::new(items)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");
    let mut list_state = ListState::default();
    list_state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn provider_picker_title(container_id: Option<&str>) -> String {
    container_id.map_or_else(
        || " provider ".to_string(),
        |container_id| format!(" {container_id} — provider "),
    )
}

#[cfg(test)]
mod provider_picker_tests {
    use super::provider_picker_title;

    #[test]
    fn launch_provider_picker_uses_single_word_title() {
        assert_eq!(provider_picker_title(None), " provider ");
    }

    #[test]
    fn inline_provider_picker_keeps_instance_context() {
        assert_eq!(provider_picker_title(Some("abc123")), " abc123 — provider ");
    }
}

fn render_role_picker_sidebar(
    frame: &mut Frame,
    area: Rect,
    workspace_name: &str,
    picker: &crate::console::widgets::role_picker::RolePickerState,
    focused: bool,
) {
    let title = format!(" {workspace_name} ");
    let block = Panel::new()
        .title(&title)
        .focus(if focused {
            PanelFocus::Focused
        } else {
            PanelFocus::Unfocused
        })
        .block();
    let items: Vec<ListItem> = picker
        .filtered
        .iter()
        .map(|role| ListItem::new(Line::from(role.key())))
        .collect();
    let list = List::new(items)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");
    let mut list_state = ListState::default();
    list_state.select(picker.list_state.selected);
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_agent_picker_sidebar(
    frame: &mut Frame,
    area: Rect,
    role_name: &str,
    picker: &crate::console::widgets::agent_choice::AgentChoiceState,
    focused: bool,
) {
    let title = format!(" {role_name} ");
    let block = Panel::new()
        .title(&title)
        .focus(if focused {
            PanelFocus::Focused
        } else {
            PanelFocus::Unfocused
        })
        .block();
    let items: Vec<ListItem> = picker
        .choices
        .iter()
        .map(|agent| {
            ListItem::new(Line::from(
                crate::console::widgets::agent_choice::agent_picker_label(*agent),
            ))
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");
    let mut list_state = ListState::default();
    list_state.select(
        picker
            .choices
            .iter()
            .position(|agent| *agent == picker.focused),
    );
    frame.render_stateful_widget(list, area, &mut list_state);
}

pub(super) fn render_mount_header(path_w: usize) -> Line<'static> {
    // Two-space gutter + two-space gaps match the data-row format so
    // columns never run into each other.
    let mode_col = format!("{:<mw$}", "Mode", mw = MOUNT_MODE_COL_WIDTH);
    let iso_col = format!("{:<iw$}", "Isolation", iw = MOUNT_ISOLATION_COL_WIDTH);
    Line::from(Span::styled(
        format!(
            "  {path:<path_w$}  {mode_col}  {iso_col}  Type",
            path = "Destination"
        ),
        Style::default().fg(WHITE),
    ))
}

pub(super) fn render_mount_lines(rows: &[MountDisplayRow], path_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<path_w$}  ", row.destination)),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<MOUNT_ISOLATION_COL_WIDTH$}", row.isolation),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(
                row.kind.clone(),
                Style::default()
                    .fg(PHOSPHOR_DIM)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(PHOSPHOR_DIM),
            )));
        }
    }
    lines
}

pub(super) fn render_global_mount_header(path_w: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {path:<path_w$}  Mode", path = "Destination"),
        Style::default().fg(WHITE),
    ))
}

pub(super) fn render_global_mount_lines(
    rows: &[MountDisplayRow],
    path_w: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<path_w$}  ", row.destination)),
            Span::styled(row.mode, Style::default().fg(PHOSPHOR_DIM)),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(PHOSPHOR_DIM),
            )));
        }
    }
    lines
}

fn render_sidebar_body(
    frame: &mut Frame,
    layout: &SidebarLayout,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    if let Some(area) = layout.instances {
        render_compact_instances_summary(
            frame,
            area,
            inputs.instance_count,
            inputs.instance_expanded,
        );
    }
    render_general_subpanel(frame, layout.general, inputs.workdir);
    let ws_focused = state.list_scroll_focus == Some(MountScrollFocus::Workspace);
    render_mounts_subpanel(
        frame,
        layout.mounts,
        inputs.mounts,
        &inputs.mount_info_cache,
        state.list_mounts_scroll_x,
        state.list_mounts_scroll_y,
        ws_focused,
    );
    if layout.global.is_some() || layout.role_global.is_some() {
        let global_focused = state.list_scroll_focus;
        let (global_rows, role_global_rows) = split_global_mount_rows(&inputs.global_rows);
        if let Some(area) = layout.global {
            render_global_mount_rows_section(
                frame,
                area,
                " Global mounts ",
                &global_rows,
                state.list_global_mounts_scroll_x,
                state.list_global_mounts_scroll_y,
                global_focused == Some(MountScrollFocus::Global),
            );
        }
        if let Some(area) = layout.role_global {
            let title = format!(" Role global mounts · {} ", inputs.picker_role_label);
            render_global_mount_rows_section(
                frame,
                area,
                &title,
                &role_global_rows,
                state.list_role_global_mounts_scroll_x,
                state.list_role_global_mounts_scroll_y,
                global_focused == Some(MountScrollFocus::RoleGlobal),
            );
        }
    }
    if let Some(area) = layout.env {
        render_environments_subpanel(frame, area, inputs.ws_config);
    }
    if let Some(area) = layout.roles {
        let roles_focused = state.list_scroll_focus == Some(MountScrollFocus::Roles);
        render_agents_subpanel_scrollable(
            frame,
            area,
            inputs.ws_config,
            config,
            state.list_roles_scroll_x,
            state.list_roles_scroll_y,
            roles_focused,
        );
    }
}

fn render_details_pane(
    frame: &mut Frame,
    area: Rect,
    _ws_idx: usize,
    ws: &WorkspaceSummary,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let inputs = sidebar_inputs_for_workspace(ws, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

/// Cursor on the synthetic "Current directory" row — mirrors
/// `workspace::current_dir_workspace`: src=dst=cwd, rw, any role.
fn render_current_dir_details_pane(
    frame: &mut Frame,
    area: Rect,
    cwd: &std::path::Path,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let cwd_str = cwd.display().to_string();
    let mounts = [crate::workspace::MountConfig {
        src: cwd_str.clone(),
        dst: cwd_str.clone(),
        readonly: false,
        isolation: crate::isolation::MountIsolation::Shared,
    }];
    let inputs = sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

/// Compact running-instances badge (3 rows: border + count line + border).
/// Cyan border and text distinguish live state from config panels.
fn render_compact_instances_summary(frame: &mut Frame, area: Rect, count: usize, expanded: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            " Running ",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));
    let plural = if count == 1 { "instance" } else { "instances" };
    let line = Line::from(vec![
        Span::styled("  ● ", Style::default().fg(CYAN)),
        Span::styled(
            format!("{count} {plural} running"),
            Style::default().fg(CYAN),
        ),
        Span::styled(
            if expanded {
                "  ·  ↓ navigate instances"
            } else {
                "  ·  → expand"
            },
            Style::default().fg(CYAN_DIM),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![line])
            .block(block)
            .style(Style::default().fg(CYAN)),
        area,
    );
}

/// Right-panel shown when operator selects an instance row in the tree.
/// When the daemon's bind-mounted socket gives us a live snapshot we
/// render the tab/pane tree (active tab marked, focused pane marked,
/// per-pane agent + state); otherwise we fall back to the on-disk
/// manifest sessions, and finally to a "no sessions recorded" hint
/// when neither is available.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn render_instance_details_pane(
    frame: &mut Frame,
    area: Rect,
    entry: &crate::instance::InstanceIndexEntry,
    sessions: &[crate::instance::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&crate::runtime::snapshot::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) {
    let instance_title = format!(" Instance: {} ", entry.instance_id);
    let block = Panel::new()
        .title(&instance_title)
        .focus(if preview_focused {
            PanelFocus::Focused
        } else {
            PanelFocus::Unfocused
        })
        .block();

    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(snapshot) = snapshot {
        let active_tab = snapshot.active_tab as usize;
        if snapshot.tabs.is_empty() {
            lines.push(Line::from(Span::styled(
                "  Daemon reports no tabs",
                Style::default().fg(PHOSPHOR_DIM),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "  Live tab/pane tree (from container daemon)",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )));
            for (tab_idx, tab) in snapshot.tabs.iter().enumerate() {
                let active = tab_idx == active_tab;
                let prefix = if active { "▸" } else { " " };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {prefix} Tab {}:  ", tab_idx + 1),
                        Style::default().fg(if active { PHOSPHOR_GREEN } else { PHOSPHOR_DIM }),
                    ),
                    Span::styled(
                        tab.label.clone(),
                        Style::default().fg(WHITE).add_modifier(if active {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                    ),
                ]));
                for pane in &tab.panes {
                    let focused = pane.session_id == tab.focused_pane;
                    let selected = selected_pane == Some(pane.session_id);
                    let marker = if focused { "●" } else { "○" };
                    let cursor_prefix = if selected { "▶ " } else { "  " };
                    let agent_label = pane.agent.clone().unwrap_or_else(|| "shell".to_string());
                    let state_label = pane.state.label();
                    let label_style = if selected {
                        Style::default()
                            .fg(WHITE)
                            .bg(PHOSPHOR_DARK)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(PHOSPHOR_GREEN)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("    {cursor_prefix}{marker} "),
                            Style::default().fg(if focused {
                                PHOSPHOR_GREEN
                            } else {
                                PHOSPHOR_DIM
                            }),
                        ),
                        Span::styled(format!("{:<16}", pane.label), label_style),
                        Span::styled(
                            format!("  ({agent_label}) "),
                            Style::default().fg(PHOSPHOR_DIM),
                        ),
                        Span::styled(
                            format!("[{state_label}]"),
                            Style::default().fg(PHOSPHOR_DIM),
                        ),
                    ]));
                }
            }
        }
    } else if sessions.is_empty() {
        let msg = if session_load_error {
            "  Sessions unavailable (manifest read error)"
        } else {
            "  No sessions recorded"
        };
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(PHOSPHOR_DIM),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  {:<24}  Agent", "Session"),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        )));
        for session in sessions {
            let name = if session.tmux_name.chars().count() > 24 {
                let cut: String = session.tmux_name.chars().take(23).collect();
                format!("{cut}…")
            } else {
                session.tmux_name.clone()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {name:<24}  "),
                    Style::default().fg(PHOSPHOR_GREEN),
                ),
                Span::styled(
                    session.agent_runtime.clone(),
                    Style::default().fg(PHOSPHOR_DIM),
                ),
            ]));
        }
    }

    // Inline footer hints inside a pane body violate the TUI design
    // rule "keyboard hints live in the footer bar only" (see
    // reference/tui-design-decisions.mdx); these keys are surfaced
    // by the list-stage footer items in render/mod.rs.

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(PHOSPHOR_GREEN)),
        area,
    );
}

/// Right-pane description shown when the cursor is on the "+ New workspace"
/// sentinel. Explains what a workspace is and why the operator might create
/// one — compacted from `docs/src/content/docs/guides/workspaces.mdx`
/// sections "What is a workspace?" + "Why save a workspace?".
fn render_sentinel_description_pane(frame: &mut Frame, area: Rect) {
    // Two stacked sub-panels so the section titles render as block titles
    // with the same PHOSPHOR_DARK border used by General/Mounts/Roles.
    // The "What is a workspace?" intro is short (fits in 4 rows); the
    // rest of the area hosts the bullet list.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // "What is a workspace?" intro (2 text rows + 2 borders + 1 pad)
            Constraint::Min(9),    // "Why create one?" bullets
        ])
        .split(area);

    let intro_block = Panel::new()
        .title(" What is a workspace? ")
        .focus(PanelFocus::Unfocused)
        .block();
    let intro_lines = vec![
        Line::from(Span::styled(
            "  A workspace saves a project boundary once so you",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  can launch roles into it from anywhere \u{2014} without",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  retyping mount paths.",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
    ];
    frame.render_widget(Paragraph::new(intro_lines).block(intro_block), rows[0]);

    let why_block = Panel::new()
        .title(" Why create one? ")
        .focus(PanelFocus::Unfocused)
        .block();
    let bullet_style = Style::default().fg(PHOSPHOR_GREEN);
    let bullets = [
        "Name a project once, launch from any cwd",
        "Keep extra mounts consistent across sessions",
        "Reuse one boundary with different role classes",
        "Set a default role or restrict which classes apply",
        "Let `jackin console` auto-detect and preselect it",
    ];
    let why_lines: Vec<Line<'static>> = bullets
        .iter()
        .map(|b| Line::from(Span::styled(format!("  \u{2022} {b}"), bullet_style)))
        .collect();
    frame.render_widget(Paragraph::new(why_lines).block(why_block), rows[1]);
}

fn render_general_subpanel(frame: &mut Frame, area: Rect, workdir: &str) {
    let block = Panel::new()
        .title(" General ")
        .focus(PanelFocus::Unfocused)
        .block();

    // Each content row is prefixed with two spaces to match the Mounts and
    // Roles sub-panels (see `SUBPANEL_CONTENT_INDENT`). Without the prefix the
    // label sat flush against the block's left border, breaking column
    // alignment with the other two blocks in the same pane.
    let lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("Working dir ", Style::default().fg(WHITE)),
        Span::raw(crate::tui::shorten_home(workdir)),
    ])];

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

/// Number of leading spaces every content row in the General / Mounts /
/// Environments / Roles sub-panels is prefixed with, so the first visible
/// character lines up across all blocks (at
/// `border_col + SUBPANEL_CONTENT_INDENT`). Pinned by
/// `subpanel_content_column_alignment` in the visual regression tests.
const SUBPANEL_CONTENT_INDENT: usize = 2;

fn render_mounts_subpanel(
    frame: &mut Frame,
    area: Rect,
    mounts: &[crate::workspace::MountConfig],
    cache: &MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mut lines: Vec<Line> = Vec::new();
    if mounts.is_empty() {
        lines.push(render_mount_header(mount_path_width(&[])));
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(PHOSPHOR_DIM),
        )));
    } else {
        let rows = format_mount_rows_with_cache(mounts, cache);
        let path_w = mount_path_width(&rows);
        lines.push(render_mount_header(path_w));
        lines.extend(render_mount_lines(&rows, path_w));
    }
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        scroll_x,
        scroll_y,
        focused,
        Some(" Mounts "),
    );
}

fn render_global_mount_rows_section(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    rows: &[&crate::config::GlobalMountRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mut lines = Vec::new();
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(PHOSPHOR_DIM),
        )));
    } else {
        let mounts: Vec<crate::workspace::MountConfig> =
            rows.iter().map(|row| row.mount.clone()).collect();
        let cache = MountInfoCache::default();
        cache.refresh_mounts(&mounts);
        let display_rows = format_mount_rows_with_cache(&mounts, &cache);
        let path_w = mount_path_width(&display_rows);
        lines.push(render_global_mount_header(path_w));
        lines.extend(render_global_mount_lines(&display_rows, path_w));
    }
    super::render_scrollable_block_at(frame, area, lines, scroll_x, scroll_y, focused, Some(title));
}

/// One row in the flat Environments preview list.
struct EnvRow {
    /// The env-key name (left-aligned in the middle column).
    name: String,
    /// `None` for a workspace-level key, `Some(role_name)` for a per-role
    /// override. Workspace-level rows render with an empty right column;
    /// per-role rows show the role name on the right in `PHOSPHOR_DIM`.
    scope: Option<String>,
    /// `true` when the value is an `op://...` reference, so the row gets
    /// a leading `[op] ` marker. The value itself never renders.
    is_op: bool,
}

/// Right-pane Environments block — flat alphabetical list, one row per
/// (env name, scope) entry.
///
/// ```text
///        API_KEY
///        DB_URL
///        DEBUG                   agent-smith
///        LOG_LEVEL               agent-brown
///  [op]  STRIPE_KEY
///  [op]  TEST                    agent-smith
/// ```
///
/// Workspace-level keys (`WorkspaceConfig.env`) and per-role override
/// keys (`WorkspaceRoleOverride.env`) are merged into a single list,
/// sorted alphabetically by name. When the same name appears at both
/// scopes, the workspace row comes first; role rows for tied names
/// then sort alphabetically by role name. Each row has a fixed left
/// marker column (`[op] ` / 5 spaces) matching the editor's
/// Environments-tab alignment, the env key in the middle, and the
/// role name on the right (workspace rows leave the right column
/// blank). Values themselves never appear — only key names.
///
/// Caller is expected to have already verified at least one env entry
/// exists at any scope (via `workspace_has_any_env`) before calling —
/// the layout omits this block entirely when the workspace has no env
/// vars, so the renderer no longer falls back to a placeholder line.
fn render_environments_subpanel(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
) {
    let block = Panel::new()
        .title(" Environments ")
        .focus(PanelFocus::Unfocused)
        .block();

    let mut rows: Vec<EnvRow> = Vec::new();
    if let Some(ws) = ws_config {
        for (key, value) in &ws.env {
            rows.push(EnvRow {
                name: key.clone(),
                scope: None,
                is_op: matches!(value, crate::operator_env::EnvValue::OpRef(_)),
            });
        }
        for (role, overrides) in &ws.roles {
            for (key, value) in &overrides.env {
                rows.push(EnvRow {
                    name: key.clone(),
                    scope: Some(role.clone()),
                    is_op: matches!(value, crate::operator_env::EnvValue::OpRef(_)),
                });
            }
        }
    }

    // Alphabetical, ties: workspace before role; role-vs-role by name.
    rows.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| match (&a.scope, &b.scope) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => x.cmp(y),
            })
    });

    let inner_width = super::scroll_viewport_width(area);
    let lines: Vec<Line> = rows
        .iter()
        .map(|row| env_row_line(row, inner_width))
        .collect();

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

/// `<indent><[op] | 5 spaces><space><name>...<pad>...<role>` —
/// role is right-aligned to `inner_width`; dropped if the left
/// content already fills the row.
fn env_row_line(row: &EnvRow, inner_width: usize) -> Line<'static> {
    let outer_indent = " ".repeat(SUBPANEL_CONTENT_INDENT);
    let marker_text: &'static str = if row.is_op { "[op] " } else { "     " };
    let gap = " ";
    let left_visible_width = outer_indent.len() + marker_text.len() + gap.len() + row.name.len();

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(5);
    spans.push(Span::raw(outer_indent));
    if row.is_op {
        spans.push(Span::styled(
            marker_text,
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        spans.push(Span::raw(marker_text));
    }
    spans.push(Span::raw(gap));
    spans.push(Span::styled(
        row.name.clone(),
        Style::default().fg(PHOSPHOR_GREEN),
    ));

    if let Some(role) = &row.scope {
        // Reserve a 1-cell gap to the right border; when too narrow,
        // fall back to a single-space gap and let Paragraph clip.
        let pad_count = if left_visible_width + 1 + role.len() + 1 < inner_width {
            inner_width - left_visible_width - role.len() - 1
        } else {
            1
        };
        spans.push(Span::raw(" ".repeat(pad_count)));
        spans.push(Span::styled(
            role.clone(),
            Style::default().fg(PHOSPHOR_DIM),
        ));
    }

    Line::from(spans)
}

#[cfg(test)]
fn render_agents_subpanel(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) {
    render_agents_subpanel_scrollable(frame, area, ws_config, config, 0, 0, false);
}

fn render_agents_subpanel_scrollable(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let allowed = ws_config.map_or(&[][..], |w| w.allowed_roles.as_slice());
    let all_allowed = ws_config.is_none_or(crate::console::manager::agent_allow::allows_all_agents);
    let default = ws_config.and_then(|w| w.default_role.as_deref());

    let mut lines: Vec<Line> = Vec::new();
    let (value_text, value_style): (String, Style) = default.map_or_else(
        || ("(none)".to_string(), Style::default().fg(PHOSPHOR_DIM)),
        |name| (name.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
    );
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Default ", Style::default().fg(WHITE)),
        Span::styled(value_text, value_style),
    ]));
    lines.push(Line::from(""));

    let agent_names: Vec<&str> = if all_allowed {
        config.roles.keys().map(String::as_str).collect()
    } else {
        allowed.iter().map(String::as_str).collect()
    };
    let name_style = |role: &str| {
        if config.roles.contains_key(role) {
            Style::default().fg(PHOSPHOR_GREEN)
        } else {
            Style::default().fg(PHOSPHOR_DIM)
        }
    };
    for role in &agent_names {
        let is_default = Some(*role) == default;
        let mut spans = vec![Span::styled(format!("  {role}"), name_style(role))];
        if is_default {
            spans.push(Span::styled(" \u{2605}", Style::default().fg(PHOSPHOR_DIM)));
        }
        if let Ok(selector) = crate::selector::RoleSelector::parse(role) {
            let scoped_count = config
                .resolve_mount_rows(&selector)
                .into_iter()
                .filter(|row| row.scope.is_some())
                .count();
            if scoped_count > 0 {
                spans.push(Span::styled(
                    format!("    +{scoped_count} role mounts"),
                    Style::default().fg(PHOSPHOR_DIM),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        scroll_x,
        scroll_y,
        focused,
        Some(" Roles "),
    );
}

#[cfg(test)]
mod list_name_scroll_tests {
    use super::{PHOSPHOR_GREEN, TAB_BG_INACTIVE_HOVER, render_list_body};
    use crate::config::AppConfig;
    use crate::console::manager::list_geometry::list_names_content_width;
    use crate::console::manager::state::{ManagerListRow, ManagerState};
    use crate::workspace::WorkspaceConfig;
    use jackin_tui::components::scrollable_panel::max_offset;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn config_with_long_workspace_name() -> AppConfig {
        let mut config = AppConfig::default();
        config.workspaces.insert(
            "chainargos-blockchain-nodes".into(),
            WorkspaceConfig::default(),
        );
        config
    }

    fn config_with_short_selected_and_long_sibling() -> AppConfig {
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("jackin".into(), WorkspaceConfig::default());
        config.workspaces.insert(
            "chainargos-blockchain-nodes".into(),
            WorkspaceConfig::default(),
        );
        config
    }

    fn config_with_sidebar_names_that_fit_wide_pane() -> AppConfig {
        let mut config = AppConfig::default();
        for name in [
            "chainargos",
            "chainargos-blockchain-nodes",
            "jackin",
            "parallax",
            "scentbird",
        ] {
            config
                .workspaces
                .insert(name.into(), WorkspaceConfig::default());
        }
        config
    }

    #[test]
    fn list_names_content_width_includes_trailing_scroll_padding() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let state = ManagerState::from_config(&config, tmp.path());

        // Rows without active instances: cursor(1) + 2 spaces + name(27) = 30 cols.
        // The selected highlight adds a trailing-padding span: 30 + 3 = 33.
        let width = list_names_content_width(&state, 19);

        assert_eq!(width, 33);
        assert_eq!(max_offset(width, 19), 14);
    }

    #[test]
    fn list_name_render_clamps_scroll_to_rendered_width() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.list_names_scroll_x = u16::MAX;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        super::super::clamp_list_scroll_for_area(
            Rect::new(0, 0, 70, 24),
            &mut state,
            &config,
            tmp.path(),
        );

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        assert_eq!(state.list_names_scroll_x, 14);
    }

    #[test]
    fn list_name_horizontal_scroll_keeps_selected_prefix_visible() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 1;
        state.list_names_scroll_x = 8;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 2)].symbol(), "▸");
        assert_eq!(buffer[(1, 2)].bg, PHOSPHOR_GREEN);
        assert_eq!(buffer[(2, 2)].bg, PHOSPHOR_GREEN);
        assert_eq!(buffer[(3, 2)].bg, PHOSPHOR_GREEN);
        for x in 1..20 {
            assert_eq!(buffer[(x, 2)].bg, PHOSPHOR_GREEN, "x={x}");
        }
    }

    #[test]
    fn list_name_horizontal_scroll_keeps_hover_background_full_width() {
        let config = config_with_long_workspace_name();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 0;
        state.hovered_list_row = Some(ManagerListRow::SavedWorkspace(0));
        state.list_names_scroll_x = 8;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for x in 1..20 {
            assert_eq!(buffer[(x, 2)].bg, TAB_BG_INACTIVE_HOVER, "x={x}");
        }
    }

    #[test]
    fn hovered_fitting_list_name_does_not_make_sidebar_horizontally_scrollable() {
        let config = config_with_sidebar_names_that_fit_wide_pane();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.hovered_list_row = Some(ManagerListRow::SavedWorkspace(0));

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 120, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for x in 1..35 {
            assert!(
                !["━", "·"].contains(&buffer[(x, 23)].symbol()),
                "unexpected horizontal scrollbar at x={x}"
            );
        }
    }

    #[test]
    fn list_name_horizontal_scroll_keeps_short_selected_background_full_width() {
        let config = config_with_short_selected_and_long_sibling();
        let tmp = tempfile::tempdir().unwrap();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 2;
        state.list_names_scroll_x = 12;
        state.list_names_focused = true;

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_list_body(frame, Rect::new(0, 0, 70, 24), &state, &config, tmp.path());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 3)].symbol(), "▸");
        for x in 1..20 {
            assert_eq!(buffer[(x, 3)].bg, PHOSPHOR_GREEN, "x={x}");
        }
    }
}

#[cfg(test)]
mod mount_table_tests {
    use super::{
        MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, MountDisplayRow, format_mount_rows,
        mount_path_width, render_mount_header, render_mount_lines,
    };
    use crate::workspace::MountConfig;

    /// Collapse a `Line` into a single plain string (concat of all span contents).
    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    /// Return the character index of the start of the `Mode` column (i.e. the
    /// "M" in "Mode" for the header, or the first char of "ro"/"rw" for a data
    /// row). Both are found at: `"  " + path_w + "  "` — so the index equals
    /// `2 + path_w + 2` for a header and for data rows that have no selection
    /// prefix (and the selection prefix is always two chars too — "▸ " or
    /// "  " — so the column boundary is stable).
    fn mode_col_start(line: &ratatui::text::Line<'_>) -> usize {
        let s = line_text(line);
        // The Mode column is the first two-letter "rw"/"ro" after the gap,
        // or the literal "Mode" for the header. Scan for the first non-space
        // character after the gap-of-two-spaces that follows the path.
        // Simpler: find the offset of the two-space gap before Mode.
        // Header: "  Path<pad>  Mode<pad>Type"
        // Data:   "  path<pad>  rw<pad>type"
        // In both cases the left edge of "Mode"/"rw" is exactly 2 + path_w + 2
        // from the start — we recover it by scanning for the first non-space
        // char at position >= 4 (past the left gutter + at least one path char).
        // Instead, just look for the substring "  M" (Mode header) or "  r"
        // (data row, always "rw"/"ro" starting with r).
        for (i, c) in s.chars().enumerate() {
            if i < 4 {
                continue;
            }
            if c == 'M' || c == 'r' {
                // Make sure this is preceded by the two-space gap — the first
                // such occurrence past the left gutter is the column boundary.
                let prev_two: String = s.chars().skip(i.saturating_sub(2)).take(2).collect();
                if prev_two == "  " {
                    return i;
                }
            }
        }
        panic!("mode column not found in line: {s:?}");
    }

    fn mount_row(
        destination: &str,
        mode: &'static str,
        isolation: &'static str,
        kind: &str,
    ) -> MountDisplayRow {
        MountDisplayRow {
            destination: destination.into(),
            host_source: None,
            mode,
            isolation,
            kind: kind.into(),
        }
    }

    #[test]
    fn header_and_data_rows_share_path_column_width() {
        // Short path + long path forces path_w to be the length of the long one.
        let rows = vec![
            mount_row("~/short", "rw", "shared", "git · main"),
            mount_row(
                "~/Projects/very/deeply/nested/directory",
                "ro",
                "worktree",
                "dir",
            ),
        ];
        let path_w = mount_path_width(&rows);
        assert!(path_w >= "~/Projects/very/deeply/nested/directory".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);

        let header_mode_col = mode_col_start(&header);
        let data0_mode_col = mode_col_start(&data[0]);
        let data1_mode_col = mode_col_start(&data[1]);

        assert_eq!(
            header_mode_col, data0_mode_col,
            "header 'mode' column must align with data row 0"
        );
        assert_eq!(
            header_mode_col, data1_mode_col,
            "header 'mode' column must align with data row 1"
        );
    }

    #[test]
    fn single_row_still_uses_minimum_column_width() {
        // Single short mount — path_w should stay at the floor so the
        // table is still visibly tabular.
        let rows = vec![mount_row(
            "~/Projects/ChainArgos/blockchain-nodes",
            "rw",
            "shared",
            "git · main",
        )];
        let path_w = mount_path_width(&rows);
        assert_eq!(path_w, "~/Projects/ChainArgos/blockchain-nodes".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        assert_eq!(mode_col_start(&header), mode_col_start(&data[0]));
    }

    #[test]
    fn empty_rows_uses_floor_for_header() {
        // Empty case: header should still render with the floor width and
        // include the two-space gap between every column.
        let path_w = mount_path_width(&[]);
        assert_eq!(path_w, "Destination".len());
        let header = render_mount_header(path_w);
        // "  <path padded>  <mode padded>  <iso padded>  Type"
        let expected = format!(
            "  {path:<path_w$}  {mode:<mw$}  {iso:<iw$}  Type",
            path = "Destination",
            mode = "Mode",
            iso = "Isolation",
            path_w = path_w,
            mw = MOUNT_MODE_COL_WIDTH,
            iw = MOUNT_ISOLATION_COL_WIDTH,
        );
        let s = line_text(&header);
        assert_eq!(s, expected);
    }

    #[test]
    fn header_has_two_space_gap_between_columns() {
        // Regression for the "Mode Type" spacing bug, extended to cover the
        // new `Isolation` column: header must emit a literal two-space gap
        // between every column (Mode → Isolation → Type), mirroring the gap
        // data rows emit between `rw`/`ro`, the isolation label, and the
        // kind. Additionally pins the type-column alignment: the `Type`
        // header label must start at the same character offset as the data
        // row's kind label.
        let rows = vec![mount_row("~/p", "rw", "shared", "folder")];
        let path_w = mount_path_width(&rows);
        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        let header_text = line_text(&header);
        let data_text = line_text(&data[0]);
        // Header should have "Mode" followed by gap+padding to the isolation column.
        assert!(
            header_text.contains("Isolation"),
            "expected header to contain 'Isolation'; got {header_text:?}"
        );
        let header_type_offset = header_text.find("Type").expect("header has 'Type'");
        let data_kind_offset = data_text.find("folder").expect("data row has 'folder'");
        assert_eq!(
            header_type_offset, data_kind_offset,
            "Type column misaligned: header at {header_type_offset}, data at {data_kind_offset}"
        );
    }

    /// Worktree mounts must surface an `Iso = worktree` badge in the data
    /// row. Per the per-mount-isolation spec the badge renders the canonical
    /// spelling for every mount (no blank for `shared`).
    #[test]
    fn mount_row_renders_isolation_badge_for_worktree() {
        let m = MountConfig {
            src: "/tmp/x".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Worktree,
        };
        let rows = format_mount_rows(std::slice::from_ref(&m));
        assert_eq!(rows.len(), 1);
        let path_w = mount_path_width(&rows);
        let lines = render_mount_lines(&rows, path_w);
        let text = line_text(&lines[0]);
        assert!(
            text.contains("worktree"),
            "missing worktree badge: {text:?}"
        );
    }

    /// Shared mounts must also surface a literal `shared` badge — the spec's
    /// canonical-spelling rule means `shared` is rendered explicitly rather
    /// than blank, so operators always see which strategy applies.
    #[test]
    fn mount_row_renders_isolation_badge_for_shared() {
        let m = MountConfig {
            src: "/tmp/x".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };
        let rows = format_mount_rows(std::slice::from_ref(&m));
        assert_eq!(rows.len(), 1);
        let path_w = mount_path_width(&rows);
        let lines = render_mount_lines(&rows, path_w);
        let text = line_text(&lines[0]);
        assert!(text.contains("shared"), "missing shared badge: {text:?}");
    }
}

#[cfg(test)]
mod mount_block_height_tests {
    //! Pins the Mounts sub-panel height formula shared by
    //! `render_details_pane` and `render_current_dir_details_pane`. Guards
    //! against the "phantom empty row" regression where a fixed
    //! `Constraint::Length(5)` over-allocated by 1 for a single-mount
    //! current-directory workspace.
    use super::{global_mounts_content_height, mount_block_height};
    use crate::workspace::MountConfig;

    fn mount(path: &str) -> MountConfig {
        MountConfig {
            src: path.into(),
            dst: path.into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }

    #[test]
    fn empty_mounts_reserves_row_for_none_placeholder() {
        // 0 data rows + "(none)" placeholder (1 row) + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[]), 4);
    }

    #[test]
    fn single_mount_fits_in_four_rows() {
        // Regression: the current-dir pane used to hard-code `Length(5)`
        // which left an extra empty line inside the block. Correct total
        // for a 1-mount workspace is 1 data + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[mount("/tmp/a")]), 4);
    }

    #[test]
    fn multiple_mounts_scale_linearly() {
        assert_eq!(mount_block_height(&[mount("/tmp/a"), mount("/tmp/b")]), 5);
        assert_eq!(
            mount_block_height(&[mount("/a"), mount("/b"), mount("/c")]),
            6
        );
    }

    #[test]
    fn many_mounts_clamp_to_twelve() {
        let mounts: Vec<MountConfig> = (0..20).map(|i| mount(&format!("/m/{i}"))).collect();
        assert_eq!(mount_block_height(&mounts), 12);
    }

    #[test]
    fn global_mount_heights_match_rendered_line_count() {
        let same_path = mount("/cache/shared");
        let split_path = MountConfig {
            src: "/host/cache".into(),
            dst: "/container/cache".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        };

        assert_eq!(global_mounts_content_height(&[same_path]), 2);
        assert_eq!(global_mounts_content_height(&[split_path]), 3);
        assert_eq!((global_mounts_content_height(&[]) + 2).min(12), 3);
    }
}

#[cfg(test)]
mod subpanel_padding_tests {
    //! Visual regression tests pinning the leading-padding convention shared
    //! by the General / Mounts / Roles sub-panels. All three render content
    //! rows starting at the same column so the first visible character of
    //! the three blocks, giving the right pane a tidy left edge.
    use super::{
        SUBPANEL_CONTENT_INDENT, render_agents_subpanel, render_environments_subpanel,
        render_general_subpanel, render_mounts_subpanel,
    };
    use crate::config::AppConfig;
    use crate::console::manager::state::{MountInfoCache, WorkspaceSummary};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    /// Scan the first content row inside a sub-panel block (y = 1, skipping
    /// the top border at y = 0) for the first cell holding a printable
    /// non-space character, skipping the left vertical border. Returns the
    /// offset of that character *from the left border* — i.e. the indent —
    /// so values can be compared against `SUBPANEL_CONTENT_INDENT` directly.
    fn first_content_indent(terminal: &Terminal<TestBackend>) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        // Locate the left border column first so the returned value is the
        // relative indent, not the absolute column.
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, 1)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            let sym = buf[(x, 1)].symbol();
            if sym.is_empty() || sym == " " {
                continue;
            }
            return Some((x - border_x - 1) as usize);
        }
        None
    }

    fn buffer_text(buf: &Buffer) -> String {
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        joined
    }

    fn summary() -> WorkspaceSummary {
        WorkspaceSummary {
            name: "demo".into(),
            workdir: "/tmp/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_role_count: 0,
            default_role: None,
            last_role: None,
        }
    }

    fn ws_config_with_allowed(names: &[&str], default: Option<&str>) -> WorkspaceConfig {
        WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/tmp/demo".into(),
            mounts: vec![],
            allowed_roles: names.iter().map(|s| (*s).into()).collect(),
            default_role: default.map(String::from),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        }
    }

    /// The first visible character of row 0 inside each sub-panel block
    /// must sit at the shared `SUBPANEL_CONTENT_INDENT`. Without the General
    /// block's two-space prefix the `w` of `workdir` rendered at column 1
    /// (flush with the border) while Mounts/Roles rendered at column 2.
    #[test]
    fn subpanel_content_column_alignment() {
        // General
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 40, 4), &summary().workdir);
        })
        .unwrap();
        let general_col = first_content_indent(&term).expect("general has content");

        // Mounts
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let cache = MountInfoCache::default();
            render_mounts_subpanel(f, Rect::new(0, 0, 40, 4), &[], &cache, 0, 0, false);
        })
        .unwrap();
        let mounts_col = first_content_indent(&term).expect("mounts has content");

        // Roles, "any role" branch (no allowed list)
        let cfg = AppConfig::default();
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 4), None, &cfg);
        })
        .unwrap();
        let agents_any_col = first_content_indent(&term).expect("roles 'any' has content");

        assert_eq!(
            general_col, SUBPANEL_CONTENT_INDENT,
            "General first char at col {general_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            mounts_col, SUBPANEL_CONTENT_INDENT,
            "Mounts first char at col {mounts_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            agents_any_col, SUBPANEL_CONTENT_INDENT,
            "Roles (any) first char at col {agents_any_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
    }

    /// Scan row `y` inside a sub-panel block for the first cell whose
    /// symbol equals `needle`, returning the offset from the left border.
    /// Used to locate the trailing star glyph on a default-role row.
    fn find_symbol_indent(terminal: &Terminal<TestBackend>, y: u16, needle: &str) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            if buf[(x, y)].symbol() == needle {
                return Some((x - border_x - 1) as usize);
            }
        }
        None
    }

    /// Scan row `y` for the last printable non-space/border cell and
    /// return its relative offset from the left border. Used to confirm
    /// a non-default row has no trailing suffix past the name.
    fn last_printable_indent(terminal: &Terminal<TestBackend>, y: u16) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let right_border_x = ((border_x + 1)..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let mut last: Option<usize> = None;
        for x in (border_x + 1)..right_border_x {
            let sym = buf[(x, y)].symbol();
            if !sym.is_empty() && sym != " " {
                last = Some((x - border_x - 1) as usize);
            }
        }
        last
    }

    /// Non-default role rows render the name starting at
    /// `SUBPANEL_CONTENT_INDENT` (col 2 from the border). With the
    /// trailing-star convention no glyph precedes the name.
    ///
    /// With the lean Roles block (env detail moved to the
    /// Environments block), the sub-panel lays out for two allowed
    /// roles (alpha default, beta non-default):
    ///   y=0 top border
    ///   y=1 `  Default <name>`
    ///   y=2 blank spacer
    ///   y=3 alpha row (default)
    ///   y=4 beta row (non-default)
    #[test]
    fn agents_subpanel_non_default_agent_name_starts_at_col_2() {
        let ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        cfg.roles
            .insert("beta".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(40, 7);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 7), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the beta row (y=4).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on beta row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("beta row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "non-default role name should start at col {SUBPANEL_CONTENT_INDENT}, got {name_col}"
        );

        // And there must be no trailing star on the non-default row.
        let last_col = last_printable_indent(&term, 4).expect("beta row has content");
        // `beta` is 4 chars starting at col 2 ⇒ last printable at col 5.
        // A trailing star would push last_col to col 7 (space + star).
        assert_eq!(
            last_col,
            SUBPANEL_CONTENT_INDENT + "beta".len() - 1,
            "non-default role row must have no trailing suffix past the name",
        );
    }

    /// Default role row carries a trailing star glyph positioned after
    /// the role name (separated by a space), not a leading star.
    ///
    /// Roles sub-panel layout: top border at y=0, `Default <name>` at
    /// y=1, blank at y=2, first role row at y=3. For a single-allowed
    /// workspace that role IS the default.
    #[test]
    fn agents_subpanel_default_agent_has_trailing_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        let star_col = find_symbol_indent(&term, 3, "\u{2605}")
            .expect("default role row should contain a star glyph");
        let expected = SUBPANEL_CONTENT_INDENT + "alpha".len() + 1;
        assert_eq!(
            star_col, expected,
            "default role star should trail the name at col {expected}, got {star_col}"
        );
    }

    /// Default role row's name column matches non-default rows (and the
    /// `SUBPANEL_CONTENT_INDENT` convention). The trailing star must not
    /// shift the name right.
    ///
    /// y=1 is the `Default <role>` row, whose label also starts at
    /// `SUBPANEL_CONTENT_INDENT`. The invariant the test pins (every
    /// content row starts at col 2) still holds — what we're confirming
    /// is that the block's leading indent is consistent. We check the
    /// role row explicitly to guard against the trailing-star breaking
    /// the name-column alignment.
    #[test]
    fn agents_subpanel_default_agent_name_starts_at_col_2_regardless_of_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the alpha row (y=3).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on alpha row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("alpha row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "default role name should start at col {SUBPANEL_CONTENT_INDENT} even with the trailing star, got {name_col}"
        );
    }

    // ── General sub-panel: Last-used row was already removed ──────────

    /// The General sub-panel no longer shows `Last used` — it only renders
    /// `Working dir`. Guards against a regression that reintroduces the row
    /// and grows the block back to 4 rows.
    #[test]
    fn general_subpanel_no_longer_shows_last_used() {
        let mut s = summary();
        s.last_role = Some("alpha".into());

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 60, 4), &s.workdir);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "General sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    // ── Roles sub-panel: Default row + per-role overrides ───────────

    /// Render the Roles sub-panel into a `TestBackend` of the given size
    /// and return one row of the buffer at `y` as a plain string. Used
    /// throughout this section to scrape per-row text after layout shifts.
    fn render_agents_row(
        ws: Option<&crate::workspace::WorkspaceConfig>,
        cfg: &AppConfig,
        width: u16,
        height: u16,
        y: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, width, height), ws, cfg);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf[(x, y)].symbol());
        }
        row
    }

    /// The Roles sub-panel renders `Default <role>` at the top, above
    /// the blank spacer and the per-role rows.
    #[test]
    fn agents_subpanel_shows_default_at_top() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
        assert!(
            row.contains("Default"),
            "Roles row 1 must hold `Default`; got {row:?}"
        );
        assert!(
            row.contains("alpha"),
            "Roles row 1 must hold the default role name; got {row:?}"
        );
    }

    /// When `default_role` is `None`, the Default row shows `(none)`.
    #[test]
    fn agents_subpanel_default_none_renders_placeholder() {
        let ws = ws_config_with_allowed(&[], None);
        let cfg = AppConfig::default();

        let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
        assert!(
            row.contains("Default") && row.contains("(none)"),
            "Default row should show `(none)` when no default role is set; got {row:?}"
        );
    }

    /// `Last used` must no longer appear anywhere in the Roles
    /// sub-panel — it was demoted as part of the preview cleanup that
    /// nested per-role overrides under each role name.
    #[test]
    fn agents_subpanel_no_longer_shows_last_used() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.last_role = Some("beta".into());
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(60, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 8), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "Roles sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    /// The Roles block is now a lean default + name list; per-role
    /// env overrides moved to the consolidated Environments block.
    /// This test pins that the Roles sub-panel does NOT mention any
    /// override key names — the keys belong only in the Environments
    /// block now.
    #[test]
    fn preview_agents_block_no_longer_lists_overrides() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut overrides = crate::workspace::WorkspaceRoleOverride::default();
        overrides.env.insert("API_KEY".into(), "literal".into());
        overrides
            .env
            .insert("LOG_LEVEL".into(), "op://Vault/Item/field".into());
        ws.roles.insert("alpha".into(), overrides);

        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(60, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 8), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        // Per-role override keys must NOT appear in the Roles block —
        // they live in the Environments block now.
        assert!(
            !joined.contains("API_KEY"),
            "override key API_KEY must NOT appear in the Roles block; got {joined}"
        );
        assert!(
            !joined.contains("LOG_LEVEL"),
            "override key LOG_LEVEL must NOT appear in the Roles block; got {joined}"
        );
        assert!(
            !joined.contains("[op]"),
            "`[op]` marker must NOT appear in the Roles block; got {joined}"
        );
        assert!(
            !joined.contains("(no overrides)"),
            "`(no overrides)` placeholder must NOT appear in the Roles block; got {joined}"
        );
        // Default + role name still render.
        assert!(
            joined.contains("Default") && joined.contains("alpha"),
            "Roles block must still show default + role name; got {joined}"
        );
    }

    /// When `allowed_roles` is empty (the "all roles allowed"
    /// shorthand), the preview lists every globally-configured role —
    /// matching what the editor's Roles tab shows. No `any role`
    /// placeholder.
    #[test]
    fn preview_agents_block_lists_all_global_agents_when_allowed_empty() {
        let ws = ws_config_with_allowed(&[], None);
        let mut cfg = AppConfig::default();
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        cfg.roles
            .insert("beta".into(), crate::config::RoleSource::default());

        let backend = TestBackend::new(60, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 12), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("alpha"),
            "alpha should be listed under all-allowed shorthand; got {joined}"
        );
        assert!(
            joined.contains("beta"),
            "beta should be listed under all-allowed shorthand; got {joined}"
        );
        assert!(
            !joined.contains("any role"),
            "old `any role` placeholder should be gone; got {joined}"
        );
    }

    // ── Environments sub-panel ─────────────────────────────────────────

    /// Render the Environments sub-panel into a fresh `TestBackend` of
    /// the given size and return the joined-with-newlines screen text.
    fn render_env_to_string(
        ws: &crate::workspace::WorkspaceConfig,
        width: u16,
        height: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, width, height), Some(ws));
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        joined
    }

    /// The Environments preview block lists workspace-level env keys in
    /// alphabetical order. Key names only — plain or op:// values never
    /// render. Op:// values get a `[op]` marker matching the editor
    /// convention.
    #[test]
    fn preview_includes_environments_block_with_workspace_env_keys() {
        let mut ws = ws_config_with_allowed(&[], None);
        ws.env.insert("DB_URL".into(), "postgres://...".into());
        ws.env.insert("API_KEY".into(), "literal-secret".into());

        let joined = render_env_to_string(&ws, 60, 6);
        assert!(
            joined.contains("Environments"),
            "block title `Environments` must appear; got {joined}"
        );
        assert!(
            joined.contains("API_KEY"),
            "API_KEY env key must appear; got {joined}"
        );
        assert!(
            joined.contains("DB_URL"),
            "DB_URL env key must appear; got {joined}"
        );
        // Sub-section header from the previous layout must NOT appear in
        // the flat list.
        assert!(
            !joined.contains("All roles:"),
            "flat layout must not render the `All roles:` sub-header; got {joined}"
        );
        // Values must never appear in the preview.
        assert!(
            !joined.contains("postgres://"),
            "plain env values must not render; got {joined}"
        );
        assert!(
            !joined.contains("literal-secret"),
            "plain env values must not render; got {joined}"
        );
    }

    /// The Environments preview is one flat list sorted alphabetically
    /// by env name. Workspace-level rows have an empty right column;
    /// per-role override rows show the role name on the right.
    #[test]
    fn preview_environments_block_lists_envs_alphabetically_with_agent_on_right() {
        let mut ws = ws_config_with_allowed(&["beta", "alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());
        ws.env.insert("DB_URL".into(), "postgres://...".into());

        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let mut beta_overrides = crate::workspace::WorkspaceRoleOverride::default();
        beta_overrides.env.insert("DEBUG".into(), "1".into());
        ws.roles.insert("beta".into(), beta_overrides);

        let joined = render_env_to_string(&ws, 60, 14);
        // No sub-headers in the flat layout.
        assert!(
            !joined.contains("All roles:"),
            "flat layout must not render `All roles:`; got {joined}"
        );
        assert!(
            !joined.contains("alpha:"),
            "flat layout must not render `<role>:` sub-headers; got {joined}"
        );
        assert!(
            !joined.contains("beta:"),
            "flat layout must not render `<role>:` sub-headers; got {joined}"
        );

        // Find each name's y-row to pin alphabetical ordering across scopes.
        let mut api_y: Option<u16> = None;
        let mut db_y: Option<u16> = None;
        let mut debug_y: Option<u16> = None;
        let mut log_y: Option<u16> = None;
        for (y, row) in joined.lines().enumerate() {
            if api_y.is_none() && row.contains("API_KEY") {
                api_y = Some(y as u16);
            }
            if db_y.is_none() && row.contains("DB_URL") {
                db_y = Some(y as u16);
            }
            if debug_y.is_none() && row.contains("DEBUG") {
                debug_y = Some(y as u16);
            }
            if log_y.is_none() && row.contains("LOG_LEVEL") {
                log_y = Some(y as u16);
            }
        }
        let api = api_y.expect("API_KEY row must appear");
        let db = db_y.expect("DB_URL row must appear");
        let debug = debug_y.expect("DEBUG row must appear");
        let log = log_y.expect("LOG_LEVEL row must appear");
        assert!(
            api < db && db < debug && debug < log,
            "rows must be alphabetical: API_KEY < DB_URL < DEBUG < LOG_LEVEL; \
             got y=({api},{db},{debug},{log})"
        );

        // Role labels live on the right edge of their row.
        for row in joined.lines() {
            if row.contains("DEBUG") {
                assert!(
                    row.contains("beta"),
                    "DEBUG row must show `beta` on the right; got {row}"
                );
            }
            if row.contains("LOG_LEVEL") {
                assert!(
                    row.contains("alpha"),
                    "LOG_LEVEL row must show `alpha` on the right; got {row}"
                );
            }
        }
    }

    /// Roles listed in `allowed_roles` but with no env overrides do
    /// NOT contribute rows to the Environments block — their absence is
    /// the signal that they have no overrides. The Roles block still
    /// lists them.
    #[test]
    fn preview_environments_block_omits_agents_without_overrides() {
        let mut ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());
        // Only alpha has overrides; beta is in the allowed list but
        // has no overrides.
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let joined = render_env_to_string(&ws, 60, 10);
        assert!(
            joined.contains("alpha"),
            "alpha has overrides — its name must appear on its row; got {joined}"
        );
        assert!(
            !joined.contains("beta"),
            "beta has no overrides — its name must NOT appear in the Environments block; got {joined}"
        );
    }

    /// A workspace-level env key renders a row with the key name and an
    /// empty right column (no role label).
    #[test]
    fn preview_environments_flat_row_workspace_level_has_no_agent_label() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());

        let joined = render_env_to_string(&ws, 60, 4);
        // The row containing API_KEY must not also contain "alpha".
        let api_row = joined
            .lines()
            .find(|r| r.contains("API_KEY"))
            .expect("API_KEY row must appear");
        assert!(
            !api_row.contains("alpha"),
            "workspace-level row must not show an role label; got `{api_row}`"
        );
    }

    /// A per-role override env key renders a row with the key on the
    /// left and the role name on the right.
    #[test]
    fn preview_environments_flat_row_per_agent_has_agent_label_on_right() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let joined = render_env_to_string(&ws, 60, 4);
        let log_row = joined
            .lines()
            .find(|r| r.contains("LOG_LEVEL"))
            .expect("LOG_LEVEL row must appear");
        assert!(
            log_row.contains("alpha"),
            "per-role row must show the role name; got `{log_row}`"
        );
        // Role name sits to the right of the key name on the same row.
        let key_pos = log_row.find("LOG_LEVEL").unwrap();
        let agent_pos = log_row.find("alpha").unwrap();
        assert!(
            agent_pos > key_pos,
            "role label must come AFTER the key name on the row; got key@{key_pos}, role@{agent_pos}"
        );
    }

    /// Per-role rows show the role label one cell before the right
    /// border, not flush against it. The cell at `inner_width - 1`
    /// (i.e. the column just inside the right border) must be a space.
    #[test]
    fn preview_environments_agent_label_has_one_cell_right_padding() {
        let mut ws = ws_config_with_allowed(&["agent-brown"], Some("agent-brown"));
        let mut brown = crate::workspace::WorkspaceRoleOverride::default();
        brown.env.insert("TEST5".into(), "v".into());
        ws.roles.insert("agent-brown".into(), brown);

        let width: u16 = 60;
        let backend = TestBackend::new(width, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, width, 4), Some(&ws));
        })
        .unwrap();
        let buf = term.backend().buffer();

        // Find the row containing TEST5; role label `agent-brown`
        // must end one cell before the right border so the cell at
        // x = width - 2 (i.e. the one just inside the right border
        // at x = width - 1) is a space, and the label's last char
        // sits at x = width - 3.
        let mut found_row: Option<u16> = None;
        for y in 0..buf.area.height {
            let row: String = (0..width).map(|x| buf[(x, y)].symbol()).collect();
            if row.contains("TEST5") {
                found_row = Some(y);
                break;
            }
        }
        let y = found_row.expect("TEST5 row must render");

        // Right border is at x = width - 1 (the `│` glyph).
        // The cell immediately inside (x = width - 2) must be blank
        // — that's the 1-cell padding the operator asked for.
        let cell_inside_border = buf[(width - 2, y)].symbol();
        assert_eq!(
            cell_inside_border,
            " ",
            "cell at x={} (one inside right border) must be a space — \
             role label should have 1-cell right padding; got {:?}",
            width - 2,
            cell_inside_border
        );

        // And the role label's last char (`n` of `agent-brown`)
        // must sit at x = width - 3 — the cell just before the pad.
        let label_last = buf[(width - 3, y)].symbol();
        assert_eq!(
            label_last,
            "n",
            "last char of `agent-brown` must sit at x={} (one cell \
             before the right border); got {:?}",
            width - 3,
            label_last
        );
    }

    /// The same env name at workspace and role scope renders TWO
    /// distinct rows: workspace first (empty right column), role
    /// second (with role label).
    #[test]
    fn preview_environments_same_key_in_workspace_and_agent_renders_two_rows() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "workspace-value".into());
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("API_KEY".into(), "role-value".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let joined = render_env_to_string(&ws, 60, 6);
        let api_rows: Vec<&str> = joined.lines().filter(|r| r.contains("API_KEY")).collect();
        assert_eq!(
            api_rows.len(),
            2,
            "API_KEY must appear in TWO rows (workspace + alpha); got rows={api_rows:?}"
        );
        // Workspace row first (no role label), role row second.
        assert!(
            !api_rows[0].contains("alpha"),
            "first API_KEY row must be workspace-level (no role label); got `{}`",
            api_rows[0]
        );
        assert!(
            api_rows[1].contains("alpha"),
            "second API_KEY row must be the role override (alpha label); got `{}`",
            api_rows[1]
        );
    }

    /// Rows sort alphabetically by name regardless of scope. Workspace
    /// keys and per-role keys interleave when their names interleave.
    #[test]
    fn preview_environments_sorts_alphabetically_across_scopes() {
        let mut ws = ws_config_with_allowed(&["agent-smith", "agent-brown"], Some("agent-smith"));
        ws.env.insert("DB_URL".into(), "postgres://...".into());
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut smith = crate::workspace::WorkspaceRoleOverride::default();
        smith.env.insert("DEBUG".into(), "1".into());
        ws.roles.insert("agent-smith".into(), smith);

        let mut brown = crate::workspace::WorkspaceRoleOverride::default();
        brown.env.insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("agent-brown".into(), brown);

        let joined = render_env_to_string(&ws, 60, 8);
        // Capture the y-row of each env-key name and assert ordering.
        let mut order: Vec<(&str, usize)> = Vec::new();
        for (y, row) in joined.lines().enumerate() {
            for key in ["API_KEY", "DB_URL", "DEBUG", "LOG_LEVEL"] {
                if row.contains(key) && !order.iter().any(|(k, _)| *k == key) {
                    order.push((key, y));
                }
            }
        }
        let names: Vec<&str> = order.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            vec!["API_KEY", "DB_URL", "DEBUG", "LOG_LEVEL"],
            "rows must be sorted alphabetically across workspace and role scopes; got {order:?}"
        );
    }

    /// Op:// references in the workspace env get a leading `[op]` marker.
    /// The bare reference itself (e.g. "<op://Vault/Item/field>") must
    /// never appear — only the marker tag.
    #[test]
    fn preview_environments_marks_op_references_with_op_marker() {
        let mut ws = ws_config_with_allowed(&[], None);
        ws.env.insert(
            "STRIPE_KEY".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc-vault/abc-item/field".into(),
                path: "Vault/Item/field".into(),
                account: None,
            }),
        );

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, 60, 4), Some(&ws));
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("[op]"),
            "op:// reference must be tagged with `[op]` marker; got {joined}"
        );
        assert!(
            joined.contains("STRIPE_KEY"),
            "key name must still appear next to `[op]`; got {joined}"
        );
        assert!(
            !joined.contains("op://"),
            "raw op:// reference must never render in the preview; got {joined}"
        );
    }

    /// When the workspace has zero env entries at every scope
    /// (workspace-level AND per-role overrides), the right-pane
    /// Environments preview block is omitted entirely — no header, no
    /// body, no border. The Roles block fills the freed space.
    #[test]
    fn preview_omits_environments_block_when_workspace_has_no_env_vars() {
        // Empty workspace env, no role overrides.
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::manager::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            !joined.contains("Environments"),
            "Environments block must NOT render when the workspace has no env vars; got {joined}"
        );
        assert!(
            !joined.contains("(no environment variables)"),
            "the placeholder line must NOT appear (block is omitted entirely); got {joined}"
        );
    }

    #[test]
    fn preview_shows_unscoped_global_mounts_without_role_ambiguity_text() {
        let ws = ws_config_with_allowed(&["alpha", "beta"], None);
        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        cfg.roles
            .insert("beta".into(), crate::config::RoleSource::default());
        cfg.add_mount(
            "cargo",
            crate::workspace::MountConfig {
                src: "/tmp/cargo".into(),
                dst: "/home/agent/.cargo".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            None,
        );
        cfg.add_mount(
            "beta-only",
            crate::workspace::MountConfig {
                src: "/tmp/beta".into(),
                dst: "/beta".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            Some("beta"),
        );

        let backend = TestBackend::new(72, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::manager::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 72, 24), 0, &summary(), &cfg, &state);
        })
        .unwrap();

        let joined = buffer_text(term.backend().buffer());
        assert!(joined.contains("Global mounts"), "{joined}");
        assert!(joined.contains(".cargo"), "{joined}");
        assert!(!joined.contains("selected role affects"), "{joined}");
        assert!(!joined.contains("/beta"), "{joined}");
        assert!(joined.contains("+1 role mounts"), "{joined}");
    }

    /// The Environments block appears as soon as ANY env entry exists
    /// at the workspace level, even if no per-role override is set.
    #[test]
    fn preview_includes_environments_block_when_only_workspace_env_set() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::manager::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("Environments"),
            "Environments block header must appear when the workspace env is non-empty; got {joined}"
        );
        assert!(
            joined.contains("API_KEY"),
            "the workspace env key must render; got {joined}"
        );
    }

    #[test]
    fn preview_shows_compact_running_badge_for_active_instances() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let mut state = crate::console::manager::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        state.instances = vec![
            crate::instance::InstanceIndexEntry {
                instance_id: "k7p9m2xq".into(),
                container_base: "jackin-demo-alpha-k7p9m2xq".into(),
                workspace_name: Some("demo".into()),
                workspace_label: "demo".into(),
                workdir: "/workspace/demo".into(),
                role_key: "alpha".into(),
                agent_runtime: "claude".into(),
                status: crate::instance::InstanceStatus::Active,
                updated_at: "2026-05-11T00:00:00Z".into(),
            },
            crate::instance::InstanceIndexEntry {
                instance_id: "done0001".into(),
                container_base: "jackin-demo-alpha-done0001".into(),
                workspace_name: Some("demo".into()),
                workspace_label: "demo".into(),
                workdir: "/workspace/demo".into(),
                role_key: "alpha".into(),
                agent_runtime: "claude".into(),
                status: crate::instance::InstanceStatus::CleanExited,
                updated_at: "2026-05-11T00:00:00Z".into(),
            },
        ];

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(72, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 72, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let joined = buffer_text(term.backend().buffer());
        // Compact badge shows the "Running" block title and instance count.
        assert!(joined.contains("Running"), "{joined}");
        assert!(joined.contains("1 instance running"), "{joined}");
        // CleanExited instances are not shown in the compact summary.
        assert!(
            !joined.contains("done0001"),
            "cleanly exited instances must not appear: {joined}"
        );
    }

    /// The Environments block appears when at least one per-role
    /// override is set, even if the workspace-level env map is empty.
    #[test]
    fn preview_includes_environments_block_when_only_per_agent_overrides_set() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut alpha_overrides = crate::workspace::WorkspaceRoleOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.roles.insert("alpha".into(), alpha_overrides);

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 0,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::manager::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("Environments"),
            "Environments block header must appear when only per-role overrides exist; got {joined}"
        );
        assert!(
            joined.contains("LOG_LEVEL"),
            "the per-role override key must render; got {joined}"
        );
    }

    /// The right-pane preview blocks render in the order
    /// General → Mounts → Environments → Roles. Pinned by scraping the
    /// block-title labels off a full-pane render and confirming their
    /// y-order.
    #[test]
    fn preview_block_order_is_general_mounts_environments_agents() {
        // Build a workspace with a mount, an env var, and an role so
        // every block has visible content.
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.workdir = "/workspace/demo".into();
        ws.mounts.push(crate::workspace::MountConfig {
            src: "/tmp/demo".into(),
            dst: "/workspace/demo".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        });
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.roles
            .insert("alpha".into(), crate::config::RoleSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_role_count: 1,
            default_role: Some("alpha".into()),
            last_role: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        let state = crate::console::manager::state::ManagerState::from_config(
            &cfg,
            std::path::Path::new("/tmp"),
        );
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), 0, &summary, &cfg, &state);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        // For each block, find the y-row that holds its title (titles
        // are unique strings so we can scrape by row content).
        let mut general_y: Option<u16> = None;
        let mut mounts_y: Option<u16> = None;
        let mut envs_y: Option<u16> = None;
        let mut agents_y: Option<u16> = None;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if general_y.is_none() && row.contains(" General ") {
                general_y = Some(y);
            }
            if mounts_y.is_none() && row.contains(" Mounts ") {
                mounts_y = Some(y);
            }
            if envs_y.is_none() && row.contains(" Environments ") {
                envs_y = Some(y);
            }
            if agents_y.is_none() && row.contains(" Roles ") {
                agents_y = Some(y);
            }
        }

        let g = general_y.expect("General block title must appear");
        let m = mounts_y.expect("Mounts block title must appear");
        let e = envs_y.expect("Environments block title must appear");
        let a = agents_y.expect("Roles block title must appear");
        assert!(
            g < m && m < e && e < a,
            "block order must be General < Mounts < Environments < Roles; got y=({g},{m},{e},{a})"
        );
    }
}
