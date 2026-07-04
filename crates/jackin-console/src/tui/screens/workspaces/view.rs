// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspaces screen view helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListItem, Paragraph},
};

use crate::tui::components::editor_rows::action_row_style;
use crate::tui::mount_display::MountDisplayRow;
use crate::tui::screens::workspaces::model::ManagerListRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disclosure {
    None,
    Collapsed,
    Expanded,
}

impl Disclosure {
    #[must_use]
    pub const fn for_instances(has_instances: bool, expanded: bool) -> Self {
        if !has_instances {
            Self::None
        } else if expanded {
            Self::Expanded
        } else {
            Self::Collapsed
        }
    }

    #[must_use]
    pub const fn glyph(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Collapsed => Some("▶"),
            Self::Expanded => Some("▼"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListRowTone {
    White,
    Workspace,
    Instance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceListDisplayRow {
    pub label: String,
    pub tone: WorkspaceListRowTone,
    pub disclosure: Disclosure,
    pub selected: bool,
    pub hovered: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal UI state flags (selected, hovered, \
              current_dir_expanded, current_dir_has_instances) — each tracks an \
              independent focus / disclosure signal consumed individually by the \
              row builder. Named-field reads match the direct focus-model idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListDisplayRowFacts {
    pub row: ManagerListRow,
    pub selected: bool,
    pub hovered: bool,
    pub current_dir_expanded: bool,
    pub current_dir_has_instances: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListDisplayRowsFacts<'a> {
    pub visual_rows: &'a [Option<ManagerListRow>],
    pub visual_selected: usize,
    pub hovered_row: Option<ManagerListRow>,
    pub current_dir_expanded: bool,
    pub current_dir_has_instances: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePreviewPanePlan {
    CurrentDirectory,
    NewWorkspace,
    SavedWorkspace(usize),
    Instance {
        workspace_idx: Option<usize>,
        instance_idx: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSidebarPlan {
    InlineProviderPicker,
    LaunchProviderPicker,
    InlineNewSessionPicker,
    InlineAgentPicker,
    InlineRolePicker,
    ListNames,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal sidebar-picker visibility flags (provider, \
              launch_provider, new_session, agent, role) — each tracks an \
              independent inline-picker open state consumed individually by the \
              sidebar planner to pick its `WorkspaceSidebarPlan` variant. Named- \
              field reads match the per-picker detection idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceSidebarFacts {
    pub inline_provider_picker_open: bool,
    pub launch_provider_picker_open: bool,
    pub inline_new_session_picker_open: bool,
    pub inline_agent_picker_open: bool,
    pub inline_role_picker_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListNamesRenderFacts {
    pub area: Rect,
    pub selected_index: usize,
    pub row_count: usize,
    pub scroll_y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListNamesRenderPlan {
    pub viewport_width: usize,
    pub follow_scroll_y: u16,
}

#[must_use]
pub const fn workspace_preview_pane_plan(row: ManagerListRow) -> WorkspacePreviewPanePlan {
    match row {
        ManagerListRow::CurrentDirectory => WorkspacePreviewPanePlan::CurrentDirectory,
        ManagerListRow::NewWorkspace => WorkspacePreviewPanePlan::NewWorkspace,
        ManagerListRow::SavedWorkspace(idx) => WorkspacePreviewPanePlan::SavedWorkspace(idx),
        ManagerListRow::CurrentDirectoryInstance(instance_idx) => {
            WorkspacePreviewPanePlan::Instance {
                workspace_idx: None,
                instance_idx,
            }
        }
        ManagerListRow::WorkspaceInstance(workspace_idx, instance_idx) => {
            WorkspacePreviewPanePlan::Instance {
                workspace_idx: Some(workspace_idx),
                instance_idx,
            }
        }
    }
}

#[must_use]
pub const fn workspace_sidebar_plan(facts: WorkspaceSidebarFacts) -> WorkspaceSidebarPlan {
    if facts.inline_provider_picker_open {
        return WorkspaceSidebarPlan::InlineProviderPicker;
    }
    if facts.launch_provider_picker_open {
        return WorkspaceSidebarPlan::LaunchProviderPicker;
    }
    if facts.inline_new_session_picker_open {
        return WorkspaceSidebarPlan::InlineNewSessionPicker;
    }
    if facts.inline_agent_picker_open {
        return WorkspaceSidebarPlan::InlineAgentPicker;
    }
    if facts.inline_role_picker_open {
        return WorkspaceSidebarPlan::InlineRolePicker;
    }
    WorkspaceSidebarPlan::ListNames
}

#[must_use]
pub const fn workspace_sidebar_owns_focus(list_names_focused: bool, list_modal_open: bool) -> bool {
    list_names_focused && !list_modal_open
}

#[must_use]
pub fn current_directory_display_row(
    disclosure: Disclosure,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: "Current directory".to_owned(),
        tone: WorkspaceListRowTone::White,
        disclosure,
        selected,
        hovered,
    }
}

#[must_use]
pub fn new_workspace_display_row(selected: bool, hovered: bool) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: new_workspace_list_label().to_owned(),
        tone: WorkspaceListRowTone::White,
        disclosure: Disclosure::None,
        selected,
        hovered,
    }
}

/// Backing data a tree instance row needs to render its label: the id, role,
/// and status (status drives the compact `[state]` tag for failed instances).
#[derive(Debug, Clone)]
pub struct InstanceRowLabel {
    pub instance_id: String,
    pub role_key: String,
    pub status: jackin_core::instance::InstanceStatus,
}

#[must_use]
pub fn workspace_instance_list_label(
    instance_id: &str,
    role_key: &str,
    status: jackin_core::instance::InstanceStatus,
) -> String {
    use jackin_core::instance::InstanceStatus as S;
    match status {
        // Live instances read as today; failed/stopped ones carry a compact
        // state tag so the operator can tell them apart in the tree (D15).
        S::Active | S::Running => format!("{instance_id}  {role_key}"),
        other => format!("{instance_id}  {role_key}  [{}]", other.short_label()),
    }
}

#[must_use]
pub fn workspace_instance_display_row(
    instance_id: &str,
    role_key: &str,
    status: jackin_core::instance::InstanceStatus,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: workspace_instance_list_label(instance_id, role_key, status),
        tone: WorkspaceListRowTone::Instance,
        disclosure: Disclosure::None,
        selected,
        hovered,
    }
}

#[must_use]
pub fn workspace_list_display_row_for_row(
    facts: WorkspaceListDisplayRowFacts,
    current_dir_instance: impl FnOnce(usize) -> Option<InstanceRowLabel>,
    saved_workspace: impl FnOnce(usize) -> Option<(String, bool, bool)>,
    workspace_instance: impl FnOnce(usize, usize) -> Option<InstanceRowLabel>,
) -> Option<WorkspaceListDisplayRow> {
    match facts.row {
        ManagerListRow::CurrentDirectory => Some(current_directory_display_row(
            Disclosure::for_instances(facts.current_dir_has_instances, facts.current_dir_expanded),
            facts.selected,
            facts.hovered,
        )),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
            current_dir_instance(inst_idx).map(|row| {
                workspace_instance_display_row(
                    &row.instance_id,
                    &row.role_key,
                    row.status,
                    facts.selected,
                    facts.hovered,
                )
            })
        }
        ManagerListRow::SavedWorkspace(idx) => {
            saved_workspace(idx).map(|(name, expanded, has_instances)| WorkspaceListDisplayRow {
                label: name,
                tone: WorkspaceListRowTone::Workspace,
                disclosure: Disclosure::for_instances(has_instances, expanded),
                selected: facts.selected,
                hovered: facts.hovered,
            })
        }
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => workspace_instance(ws_idx, inst_idx)
            .map(|row| {
                workspace_instance_display_row(
                    &row.instance_id,
                    &row.role_key,
                    row.status,
                    facts.selected,
                    facts.hovered,
                )
            }),
        ManagerListRow::NewWorkspace => {
            Some(new_workspace_display_row(facts.selected, facts.hovered))
        }
    }
}

#[must_use]
pub fn workspace_list_display_rows(
    facts: WorkspaceListDisplayRowsFacts<'_>,
    mut current_dir_instance: impl FnMut(usize) -> Option<InstanceRowLabel>,
    mut saved_workspace: impl FnMut(usize) -> Option<(String, bool, bool)>,
    mut workspace_instance: impl FnMut(usize, usize) -> Option<InstanceRowLabel>,
) -> Vec<Option<WorkspaceListDisplayRow>> {
    facts
        .visual_rows
        .iter()
        .enumerate()
        .map(|(idx, visual_row)| {
            visual_row.as_ref().and_then(|row| {
                workspace_list_display_row_for_row(
                    WorkspaceListDisplayRowFacts {
                        row: *row,
                        selected: idx == facts.visual_selected,
                        hovered: facts.hovered_row == Some(*row),
                        current_dir_expanded: facts.current_dir_expanded,
                        current_dir_has_instances: facts.current_dir_has_instances,
                    },
                    &mut current_dir_instance,
                    &mut saved_workspace,
                    &mut workspace_instance,
                )
            })
        })
        .collect()
}

#[must_use]
pub fn instance_purge_confirm_label(container_base: &str, role_key: Option<&str>) -> String {
    role_key.map_or_else(
        || container_base.to_owned(),
        |role_key| format!("{container_base} ({role_key})"),
    )
}

#[must_use]
pub fn workspace_instance_pane_agent_label(agent: Option<&str>) -> String {
    agent.unwrap_or("shell").to_owned()
}

#[must_use]
pub const fn current_directory_workspace_title() -> &'static str {
    "Current directory"
}

#[must_use]
pub const fn new_workspace_list_label() -> &'static str {
    "+ New workspace"
}

#[must_use]
pub fn picker_sidebar_title(label: &str) -> String {
    format!(" {label} ")
}

#[must_use]
pub fn role_global_mounts_title(role_label: &str) -> String {
    format!(" Role global mounts · {role_label} ")
}

#[must_use]
pub const fn global_mounts_title() -> &'static str {
    " Global mounts "
}

#[must_use]
pub const fn instance_sessions_empty_message(session_load_error: bool) -> &'static str {
    if session_load_error {
        "Sessions unavailable (manifest read error)"
    } else {
        "No sessions recorded"
    }
}

pub fn list_name_lines(
    visual_rows: &[Option<WorkspaceListDisplayRow>],
    viewport: usize,
    show_cursor: bool,
) -> (Vec<Line<'static>>, usize) {
    // Structural exception: workspace names are a mixed tree with disclosure,
    // instance tones, hover fill, and horizontal scroll padding, so they cannot
    // use the flat picker renderer even though they share its cursor contract.
    let mut max_w = viewport;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(visual_rows.len());

    for visual_row in visual_rows {
        let Some(row) = visual_row else {
            lines.push(Line::from(""));
            continue;
        };
        match row.tone {
            WorkspaceListRowTone::Instance => {
                push_tree_instance_line(&mut lines, row, show_cursor, &mut max_w);
            }
            WorkspaceListRowTone::White | WorkspaceListRowTone::Workspace => {
                push_tree_workspace_line(&mut lines, row, show_cursor, &mut max_w);
            }
        }
    }

    let content_w = jackin_tui::components::scrollable_panel::max_line_width(&lines).max(max_w);

    if let Some(selected_idx) = visual_rows
        .iter()
        .position(|row| row.as_ref().is_some_and(|row| row.selected))
        && let Some(line) = lines.get_mut(selected_idx)
    {
        let current_w = jackin_tui::components::scrollable_panel::line_width(line);
        if current_w < content_w {
            let bg = match visual_rows[selected_idx].as_ref().map(|row| row.tone) {
                Some(WorkspaceListRowTone::Instance) => jackin_tui::theme::CYAN,
                _ => jackin_tui::theme::PHOSPHOR_GREEN,
            };
            line.spans.push(Span::styled(
                " ".repeat(content_w - current_w),
                Style::default().bg(bg).fg(Color::Black),
            ));
        }
    }

    if let Some(hovered_idx) = visual_rows
        .iter()
        .position(|row| row.as_ref().is_some_and(|row| row.hovered && !row.selected))
        && let Some(line) = lines.get_mut(hovered_idx)
    {
        for span in &mut line.spans {
            span.style = span.style.bg(jackin_tui::theme::TAB_BG_INACTIVE_HOVER);
        }
        let current_w = jackin_tui::components::scrollable_panel::line_width(line);
        if current_w < content_w {
            line.spans.push(Span::styled(
                " ".repeat(content_w - current_w),
                Style::default().bg(jackin_tui::theme::TAB_BG_INACTIVE_HOVER),
            ));
        }
    }

    (lines, content_w)
}

#[must_use]
pub fn workspace_list_names_render_plan(
    facts: WorkspaceListNamesRenderFacts,
) -> WorkspaceListNamesRenderPlan {
    let viewport_h = usize::from(facts.area.height.saturating_sub(2));
    WorkspaceListNamesRenderPlan {
        viewport_width: jackin_tui::components::scrollable_panel::viewport_width(facts.area),
        follow_scroll_y: jackin_tui::components::scrollable_panel::cursor_follow_offset(
            facts.selected_index,
            facts.row_count,
            viewport_h,
            facts.scroll_y,
        ),
    }
}

pub fn render_list_names_block(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    content_width: usize,
    focused: bool,
    scroll_x: u16,
    scroll_y: u16,
) {
    let content_height = lines.len();
    let viewport_w = jackin_tui::components::scrollable_panel::viewport_width(area);
    let viewport_h = jackin_tui::components::scrollable_panel::viewport_height(area);
    let h_scrollable =
        jackin_tui::components::scrollable_panel::is_scrollable(content_width, viewport_w);
    let v_scrollable =
        jackin_tui::components::scrollable_panel::is_scrollable(content_height, viewport_h);
    let block = jackin_tui::components::Panel::new()
        .focus(if focused {
            jackin_tui::components::PanelFocus::Focused
        } else {
            jackin_tui::components::PanelFocus::Unfocused
        })
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_rows = usize::from(inner.height).min(content_height);
    let offset_y = usize::from(scroll_y).min(content_height.saturating_sub(visible_rows));
    for (row_idx, line) in lines
        .into_iter()
        .skip(offset_y)
        .take(visible_rows)
        .enumerate()
    {
        render_list_name_line(frame, inner, row_idx as u16, line, usize::from(scroll_x));
    }
    if h_scrollable {
        jackin_tui::components::scrollable_panel::render_horizontal_scrollbar(
            frame,
            area,
            content_width,
            scroll_x,
        );
    }
    if v_scrollable {
        jackin_tui::components::scrollable_panel::render_vertical_scrollbar(
            frame,
            area,
            content_height,
            scroll_y,
        );
    }
}

fn render_list_name_line(
    frame: &mut Frame<'_>,
    area: Rect,
    row: u16,
    line: Line<'static>,
    scroll_x: usize,
) {
    const PREFIX_COLS: usize = 3;
    jackin_tui::components::scrollable_panel::render_line_with_fixed_prefix_scroll(
        frame,
        area,
        row,
        line,
        PREFIX_COLS,
        scroll_x,
    );
}

fn row_fg(row: &WorkspaceListDisplayRow) -> Color {
    match row.tone {
        WorkspaceListRowTone::White => jackin_tui::theme::WHITE,
        WorkspaceListRowTone::Workspace => jackin_tui::theme::PHOSPHOR_GREEN,
        WorkspaceListRowTone::Instance => jackin_tui::theme::CYAN,
    }
}

fn push_tree_workspace_line(
    lines: &mut Vec<Line<'static>>,
    row: &WorkspaceListDisplayRow,
    show_cursor: bool,
    max_w: &mut usize,
) {
    let cursor = if row.selected && show_cursor {
        "▸"
    } else {
        " "
    };
    if row.label.starts_with("+ ") {
        let cursor_col = if row.selected && show_cursor {
            "\u{25b8} "
        } else {
            "  "
        };
        *max_w = (*max_w).max(2 + jackin_tui::display_cols(&row.label));
        lines.push(Line::from(vec![
            Span::styled(cursor_col, action_row_style(row.selected)),
            Span::styled(row.label.clone(), action_row_style(row.selected)),
        ]));
        return;
    }
    let disclosure = row.disclosure;
    let color = row_fg(row);
    let line = if let Some(arrow) = disclosure.glyph() {
        let text_w = 1 + 1 + 1 + jackin_tui::display_cols(&row.label);
        *max_w = (*max_w).max(text_w);
        if row.selected {
            Line::from(vec![
                Span::styled(
                    cursor,
                    Style::default()
                        .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                        .fg(Color::Black),
                ),
                Span::styled(
                    arrow,
                    Style::default()
                        .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                        .fg(Color::Black),
                ),
                Span::styled(
                    format!(" {}", row.label),
                    Style::default()
                        .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                        .fg(Color::Black),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(cursor, Style::default().fg(color)),
                Span::styled(arrow, Style::default().fg(color)),
                Span::styled(format!(" {}", row.label), Style::default().fg(color)),
            ])
        }
    } else {
        let text_w = 3 + jackin_tui::display_cols(&row.label);
        *max_w = (*max_w).max(text_w);
        if row.selected {
            Line::from(Span::styled(
                format!("{cursor}  {}", row.label),
                Style::default()
                    .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                    .fg(Color::Black),
            ))
        } else {
            Line::from(Span::styled(
                format!("{cursor}  {}", row.label),
                Style::default().fg(color),
            ))
        }
    };
    lines.push(line);
}

fn push_tree_instance_line(
    lines: &mut Vec<Line<'static>>,
    row: &WorkspaceListDisplayRow,
    show_cursor: bool,
    max_w: &mut usize,
) {
    let cursor = if row.selected && show_cursor {
        "▸"
    } else {
        " "
    };
    let text_w = 1 + 4 + jackin_tui::display_cols(&row.label);
    *max_w = (*max_w).max(text_w);

    let line = if row.selected {
        Line::from(Span::styled(
            format!("{cursor}    {}", row.label),
            Style::default()
                .bg(jackin_tui::theme::CYAN)
                .fg(Color::Black),
        ))
    } else {
        let mut parts = row.label.splitn(2, "  ");
        let instance_id = parts.next().unwrap_or_default();
        let role_key = parts.next().unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!("{cursor}    "),
                Style::default().fg(jackin_tui::theme::CYAN_DIM),
            ),
            Span::styled(
                instance_id.to_owned(),
                Style::default().fg(jackin_tui::theme::CYAN_DIM),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                role_key.to_owned(),
                Style::default().fg(jackin_tui::theme::CYAN),
            ),
        ])
    };
    lines.push(line);
}

#[must_use]
pub fn create_prelude_mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn create_prelude_mount_destination_default(src_display: Option<&str>) -> String {
    src_display.unwrap_or_default().to_owned()
}

#[must_use]
pub fn create_prelude_workspace_name_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Name this workspace", current)
}

#[must_use]
pub fn create_prelude_workspace_name_default(dst: Option<&str>) -> String {
    dst.and_then(|dst| {
        std::path::Path::new(dst)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
    })
    .unwrap_or_default()
}

#[must_use]
pub fn create_prelude_mount_dst_choice_state(
    src: impl Into<String>,
) -> crate::tui::components::mount_dst_choice::MountDstChoiceState {
    crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src)
}

#[must_use]
pub fn create_prelude_workdir_pick_state<M: crate::tui::components::workdir_pick::WorkdirMount>(
    mounts: &[M],
) -> crate::tui::components::workdir_pick::WorkdirPickState {
    crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(mounts)
}

/// Compact running-instances badge (3 rows: border + count line + border).
/// Cyan border and text distinguish live state from config panels.
pub fn render_compact_instances_summary(
    frame: &mut Frame<'_>,
    area: Rect,
    count: usize,
    expanded: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(jackin_tui::theme::CYAN))
        .title(Span::styled(
            " Running ",
            Style::default()
                .fg(jackin_tui::theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ));
    let plural = if count == 1 { "instance" } else { "instances" };
    let line = Line::from(vec![
        Span::styled("  ● ", Style::default().fg(jackin_tui::theme::CYAN)),
        Span::styled(
            format!("{count} {plural} running"),
            Style::default().fg(jackin_tui::theme::CYAN),
        ),
        Span::styled(
            if expanded {
                "  ·  ↓ navigate instances"
            } else {
                "  ·  → expand"
            },
            Style::default().fg(jackin_tui::theme::CYAN_DIM),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![line])
            .block(block)
            .style(Style::default().fg(jackin_tui::theme::CYAN)),
        area,
    );
}

/// Right-pane description shown when cursor is on the "+ New workspace"
/// sentinel. It summarizes what a saved workspace records and why to create it.
pub fn render_sentinel_description_pane(frame: &mut Frame<'_>, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(9)])
        .split(area);

    let intro_block = jackin_tui::components::Panel::new()
        .title(" What is a workspace? ")
        .focus(jackin_tui::components::PanelFocus::Unfocused)
        .block();
    let intro_lines = vec![
        Line::from(Span::styled(
            "  A workspace saves a project boundary once so you",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  can launch roles into it from anywhere \u{2014} without",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  retyping mount paths.",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN),
        )),
    ];
    frame.render_widget(Paragraph::new(intro_lines).block(intro_block), rows[0]);

    let why_block = jackin_tui::components::Panel::new()
        .title(" Why create one? ")
        .focus(jackin_tui::components::PanelFocus::Unfocused)
        .block();
    let bullet_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
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

#[must_use]
pub fn provider_picker_title(container_id: Option<&str>) -> String {
    container_id.map_or_else(
        || " Provider ".to_owned(),
        |container_id| format!(" {container_id} — Provider "),
    )
}

pub fn render_picker_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    labels: Vec<String>,
    selected: Option<usize>,
    focused: bool,
) {
    let block = jackin_tui::components::Panel::new()
        .title(title)
        .focus(if focused {
            jackin_tui::components::PanelFocus::Focused
        } else {
            jackin_tui::components::PanelFocus::Unfocused
        })
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let items: Vec<ListItem<'_>> = labels
        .into_iter()
        .map(|label| ListItem::new(Line::from(label)))
        .collect();
    let mut list = jackin_tui::components::ScrollableList::new(items)
        .style(Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN))
        .highlight_style(
            Style::default()
                .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                .fg(Color::Black),
        )
        .selected(selected)
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);
    list = list.highlight_symbol(if focused { "▸ " } else { "  " });
    list.render(frame.buffer_mut(), inner);
}

pub fn render_provider_picker_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    container_id: Option<&str>,
    labels: Vec<String>,
    selected: usize,
    focused: bool,
) {
    let title = provider_picker_title(container_id);
    render_picker_sidebar(frame, area, &title, labels, Some(selected), focused);
}

pub fn render_role_picker_sidebar<R: crate::tui::components::role_picker::RoleChoice>(
    frame: &mut Frame<'_>,
    area: Rect,
    workspace_name: &str,
    picker: &crate::tui::components::role_picker::RolePickerState<R>,
    focused: bool,
) {
    let title = picker_sidebar_title(workspace_name);
    let labels = picker
        .filtered
        .iter()
        .map(crate::tui::components::role_picker::RoleChoice::key)
        .collect();
    render_picker_sidebar(
        frame,
        area,
        &title,
        labels,
        picker.list_state.selected,
        focused,
    );
}

pub fn render_agent_picker_sidebar<A: crate::tui::components::agent_choice::AgentChoice>(
    frame: &mut Frame<'_>,
    area: Rect,
    role_name: &str,
    picker: &crate::tui::components::agent_choice::AgentChoiceState<A>,
    focused: bool,
) {
    let title = picker_sidebar_title(role_name);
    let labels = picker
        .choices
        .iter()
        .map(|agent| crate::tui::components::agent_choice::agent_picker_label(*agent).to_owned())
        .collect();
    let selected = picker
        .choices
        .iter()
        .position(|agent| *agent == picker.focused);
    render_picker_sidebar(frame, area, &title, labels, selected, focused);
}

pub fn render_general_subpanel(frame: &mut Frame<'_>, area: Rect, workdir_display: &str) {
    let block = jackin_tui::components::Panel::new()
        .title(" General ")
        .focus(jackin_tui::components::PanelFocus::Unfocused)
        .block();
    let lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Working dir ",
            Style::default().fg(jackin_tui::theme::WHITE),
        ),
        Span::raw(workdir_display.to_owned()),
    ])];
    let panel = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN));
    frame.render_widget(panel, area);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEnvRow {
    pub name: String,
    pub scope: Option<String>,
    pub is_op: bool,
}

#[must_use]
pub fn workspace_env_rows(
    ws_config: Option<&jackin_config::WorkspaceConfig>,
) -> Vec<WorkspaceEnvRow> {
    let mut rows = Vec::new();
    if let Some(ws) = ws_config {
        for (key, value) in &ws.env {
            rows.push(WorkspaceEnvRow {
                name: key.clone(),
                scope: None,
                is_op: matches!(value, jackin_config::EnvValue::OpRef(_)),
            });
        }
        for (role, overrides) in &ws.roles {
            for (key, value) in &overrides.env {
                rows.push(WorkspaceEnvRow {
                    name: key.clone(),
                    scope: Some(role.clone()),
                    is_op: matches!(value, jackin_config::EnvValue::OpRef(_)),
                });
            }
        }
    }
    rows
}

pub fn render_environments_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    mut rows: Vec<WorkspaceEnvRow>,
) {
    let block = jackin_tui::components::Panel::new()
        .title(" Environments ")
        .focus(jackin_tui::components::PanelFocus::Unfocused)
        .block();

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

    let inner_width = jackin_tui::components::scrollable_panel::viewport_width(area);
    let lines: Vec<Line<'_>> = rows
        .iter()
        .map(|row| env_row_line(row, inner_width))
        .collect();

    let panel = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN));
    frame.render_widget(panel, area);
}

pub fn render_mounts_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[MountDisplayRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        crate::tui::mount_display::workspace_mount_block_lines(rows),
        scroll_x,
        scroll_y,
        focused,
        Some(" Mounts "),
    );
}

pub fn render_config_mounts_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    mounts: &[jackin_config::MountConfig],
    cache: &crate::mount_info_cache::MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let rows = crate::tui::mount_display::format_config_mount_rows_with_cache(mounts, cache);
    render_mounts_subpanel(frame, area, &rows, scroll_x, scroll_y, focused);
}

pub fn render_global_mounts_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: &[MountDisplayRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        crate::tui::mount_display::global_mount_block_lines(rows),
        scroll_x,
        scroll_y,
        focused,
        Some(title),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_global_mount_rows_section(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: &[&jackin_config::GlobalMountRow],
    cache: &crate::mount_info_cache::MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mounts: Vec<jackin_config::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    let display_rows =
        crate::tui::mount_display::format_config_mount_rows_with_cache(&mounts, cache);
    render_global_mounts_subpanel(
        frame,
        area,
        title,
        &display_rows,
        scroll_x,
        scroll_y,
        focused,
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRoleRow {
    pub name: String,
    pub exists: bool,
    pub is_default: bool,
    pub scoped_mount_count: usize,
}

pub fn render_roles_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    default_role: Option<&str>,
    rows: Vec<WorkspaceRoleRow>,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    let (value_text, value_style): (String, Style) = default_role.map_or_else(
        || {
            (
                "(none)".to_owned(),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            )
        },
        |name| {
            (
                name.to_owned(),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN),
            )
        },
    );
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Default ", Style::default().fg(jackin_tui::theme::WHITE)),
        Span::styled(value_text, value_style),
    ]));
    lines.push(Line::from(""));

    for row in rows {
        let name_style = if row.exists {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)
        };
        let mut spans = vec![Span::styled(format!("  {}", row.name), name_style)];
        if row.is_default {
            spans.push(Span::styled(
                " \u{2605}",
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        if row.scoped_mount_count > 0 {
            spans.push(Span::styled(
                format!("    +{} role mounts", row.scoped_mount_count),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ));
        }
        lines.push(Line::from(spans));
    }

    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        scroll_x,
        scroll_y,
        focused,
        Some(" Roles "),
    );
}

pub fn render_config_roles_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    ws_config: Option<&jackin_config::WorkspaceConfig>,
    config: &jackin_config::AppConfig,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let allowed = ws_config.map_or(&[][..], |w| w.allowed_roles.as_slice());
    let all_allowed = ws_config.is_none_or(crate::workspace::allows_all_agents);
    let default = ws_config.and_then(|w| w.default_role.as_deref());

    let agent_names: Vec<&str> = if all_allowed {
        config.roles.keys().map(String::as_str).collect()
    } else {
        allowed.iter().map(String::as_str).collect()
    };
    let rows = agent_names
        .into_iter()
        .map(|role| WorkspaceRoleRow {
            name: role.to_owned(),
            exists: config.roles.contains_key(role),
            is_default: Some(role) == default,
            scoped_mount_count: role_scoped_mount_count(config, role),
        })
        .collect();
    render_roles_subpanel(frame, area, default, rows, scroll_x, scroll_y, focused);
}

fn role_scoped_mount_count(config: &jackin_config::AppConfig, role: &str) -> usize {
    jackin_core::RoleSelector::parse(role).map_or(0, |selector| {
        config
            .resolve_mount_rows(&selector)
            .into_iter()
            .filter(|row| row.scope.is_some())
            .count()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstancePane {
    pub instance_id: String,
    pub focused: bool,
    pub content: WorkspaceInstancePaneContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceInstancePaneContent {
    Live {
        tabs: Vec<WorkspaceInstanceTab>,
    },
    Sessions {
        rows: Vec<WorkspaceInstanceSessionRow>,
    },
    Empty {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceTab {
    pub index: usize,
    pub label: String,
    pub active: bool,
    pub panes: Vec<WorkspaceInstanceTabPane>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceTabPane {
    pub label: String,
    pub agent_label: String,
    pub state_label: String,
    pub focused: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceSessionRow {
    pub name: String,
    pub agent_runtime: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceLiveTabFacts {
    pub label: String,
    pub focused_pane: u64,
    pub panes: Vec<WorkspaceInstanceLivePaneFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInstanceLivePaneFacts {
    pub session_id: u64,
    pub label: String,
    pub agent: Option<String>,
    pub state_label: String,
}

#[must_use]
pub fn workspace_instance_pane(
    instance_id: String,
    focused: bool,
    content: WorkspaceInstancePaneContent,
) -> WorkspaceInstancePane {
    WorkspaceInstancePane {
        instance_id,
        focused,
        content,
    }
}

#[must_use]
pub fn workspace_instance_live_content(
    active_tab: usize,
    selected_pane: Option<u64>,
    tabs: impl IntoIterator<Item = WorkspaceInstanceLiveTabFacts>,
) -> WorkspaceInstancePaneContent {
    WorkspaceInstancePaneContent::Live {
        tabs: tabs
            .into_iter()
            .enumerate()
            .map(|(tab_idx, tab)| WorkspaceInstanceTab {
                index: tab_idx,
                label: tab.label,
                active: tab_idx == active_tab,
                panes: tab
                    .panes
                    .into_iter()
                    .map(|pane| WorkspaceInstanceTabPane {
                        label: pane.label,
                        agent_label: workspace_instance_pane_agent_label(pane.agent.as_deref()),
                        state_label: pane.state_label,
                        focused: pane.session_id == tab.focused_pane,
                        selected: selected_pane == Some(pane.session_id),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[must_use]
pub fn workspace_instance_session_content(
    session_load_error: bool,
    sessions: impl IntoIterator<Item = WorkspaceInstanceSessionRow>,
) -> WorkspaceInstancePaneContent {
    let rows: Vec<_> = sessions.into_iter().collect();
    if rows.is_empty() {
        WorkspaceInstancePaneContent::Empty {
            message: instance_sessions_empty_message(session_load_error).to_owned(),
        }
    } else {
        WorkspaceInstancePaneContent::Sessions { rows }
    }
}

pub fn render_instance_details_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    pane: &WorkspaceInstancePane,
) {
    let instance_title = format!(" Instance: {} ", pane.instance_id);
    let block = jackin_tui::components::Panel::new()
        .title(&instance_title)
        .focus(if pane.focused {
            jackin_tui::components::PanelFocus::Focused
        } else {
            jackin_tui::components::PanelFocus::Unfocused
        })
        .block();
    let lines = instance_detail_lines(&pane.content);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)),
        area,
    );
}

fn instance_detail_lines(content: &WorkspaceInstancePaneContent) -> Vec<Line<'static>> {
    match content {
        WorkspaceInstancePaneContent::Live { tabs } => live_instance_lines(tabs),
        WorkspaceInstancePaneContent::Sessions { rows } => session_instance_lines(rows),
        WorkspaceInstancePaneContent::Empty { message } => vec![Line::from(Span::styled(
            format!("  {message}"),
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        ))],
    }
}

fn live_instance_lines(tabs: &[WorkspaceInstanceTab]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if tabs.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Daemon reports no tabs",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        )));
        return lines;
    }

    lines.push(Line::from(Span::styled(
        "  Live tab/pane tree (from container daemon)",
        Style::default()
            .fg(jackin_tui::theme::WHITE)
            .add_modifier(Modifier::BOLD),
    )));
    for tab in tabs {
        let prefix = if tab.active { "▸" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {prefix} Tab {}:  ", tab.index + 1),
                Style::default().fg(if tab.active {
                    jackin_tui::theme::PHOSPHOR_GREEN
                } else {
                    jackin_tui::theme::PHOSPHOR_DIM
                }),
            ),
            Span::styled(
                tab.label.clone(),
                Style::default()
                    .fg(jackin_tui::theme::WHITE)
                    .add_modifier(if tab.active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]));
        for pane in &tab.panes {
            let marker = if pane.focused { "●" } else { "○" };
            let cursor_prefix = if pane.selected { "▶ " } else { "  " };
            let label_style = if pane.selected {
                Style::default()
                    .fg(jackin_tui::theme::WHITE)
                    .bg(jackin_tui::theme::PHOSPHOR_DARK)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {cursor_prefix}{marker} "),
                    Style::default().fg(if pane.focused {
                        jackin_tui::theme::PHOSPHOR_GREEN
                    } else {
                        jackin_tui::theme::PHOSPHOR_DIM
                    }),
                ),
                Span::styled(format!("{:<16}", pane.label), label_style),
                Span::styled(
                    format!("  ({}) ", pane.agent_label),
                    Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
                ),
                Span::styled(
                    format!("[{}]", pane.state_label),
                    Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
                ),
            ]));
        }
    }
    lines
}

fn session_instance_lines(rows: &[WorkspaceInstanceSessionRow]) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!("  {:<24}  Agent", "Session"),
        Style::default()
            .fg(jackin_tui::theme::WHITE)
            .add_modifier(Modifier::BOLD),
    ))];
    for row in rows {
        let name = if row.name.chars().count() > 24 {
            let cut: String = row.name.chars().take(23).collect();
            format!("{cut}…")
        } else {
            row.name.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {name:<24}  "),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN),
            ),
            Span::styled(
                row.agent_runtime.clone(),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            ),
        ]));
    }
    lines
}

fn env_row_line(row: &WorkspaceEnvRow, inner_width: usize) -> Line<'static> {
    const SUBPANEL_CONTENT_INDENT: usize = 2;
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
                .fg(jackin_tui::theme::PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        spans.push(Span::raw(marker_text));
    }
    spans.push(Span::raw(gap));
    spans.push(Span::styled(
        row.name.clone(),
        Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN),
    ));

    if let Some(role) = &row.scope {
        let pad_count = if left_visible_width + 1 + role.len() + 1 < inner_width {
            inner_width - left_visible_width - role.len() - 1
        } else {
            1
        };
        spans.push(Span::raw(" ".repeat(pad_count)));
        spans.push(Span::styled(
            role.clone(),
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        ));
    }

    Line::from(spans)
}

/// Concrete footer adapter for the workspace list screen.
pub mod footer;

/// Root-console workspace-list display adapters.
pub mod list;

#[cfg(test)]
mod tests;
