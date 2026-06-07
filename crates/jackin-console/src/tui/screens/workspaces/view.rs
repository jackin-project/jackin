//! Workspaces screen view helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListItem, Paragraph},
};

use crate::tui::mount_display::MountDisplayRow;

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
    pub expanded: bool,
    pub has_instances: bool,
    pub selected: bool,
    pub hovered: bool,
}

#[must_use]
pub fn current_directory_display_row(
    expanded: bool,
    has_instances: bool,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: "Current directory".to_owned(),
        tone: WorkspaceListRowTone::White,
        expanded,
        has_instances,
        selected,
        hovered,
    }
}

#[must_use]
pub fn new_workspace_display_row(selected: bool, hovered: bool) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: new_workspace_list_label().to_owned(),
        tone: WorkspaceListRowTone::White,
        expanded: false,
        has_instances: false,
        selected,
        hovered,
    }
}

#[must_use]
pub fn workspace_instance_list_label(instance_id: &str, role_key: &str) -> String {
    format!("{instance_id}  {role_key}")
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
    let disclosure = Disclosure::for_instances(row.has_instances, row.expanded);
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
        || " provider ".to_owned(),
        |container_id| format!(" {container_id} — provider "),
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

#[cfg(test)]
mod tests;
