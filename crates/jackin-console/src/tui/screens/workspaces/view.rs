//! Workspaces screen view helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    mount_display::{MountDisplayRow, mount_path_width},
    tui::components::mount_rows::{
        render_global_mount_header, render_global_mount_lines, render_mount_header,
        render_mount_lines,
    },
};

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

#[must_use]
pub fn workspace_delete_confirm_state(name: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!("Delete \"{name}\"?"))
}

#[must_use]
pub fn instance_purge_confirm_state(label: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!(
        "Purge \"{label}\"?\nThis removes the role container, DinD sidecar, volume, network, AND local recovery state. Cannot be undone."
    ))
}

#[must_use]
pub fn create_prelude_mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn create_prelude_workspace_name_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Name this workspace", current)
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
pub fn render_compact_instances_summary(frame: &mut Frame, area: Rect, count: usize, expanded: bool) {
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
pub fn render_sentinel_description_pane(frame: &mut Frame, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(9),
        ])
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
        || " provider ".to_string(),
        |container_id| format!(" {container_id} — provider "),
    )
}

pub fn render_picker_sidebar(
    frame: &mut Frame,
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
    let items: Vec<ListItem> = labels
        .into_iter()
        .map(|label| ListItem::new(Line::from(label)))
        .collect();
    let list = List::new(items)
        .block(block)
        .style(Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN))
        .highlight_style(
            Style::default()
                .bg(jackin_tui::theme::PHOSPHOR_GREEN)
                .fg(Color::Black),
        )
        .highlight_symbol("▸ ");
    let mut list_state = ListState::default();
    list_state.select(selected);
    frame.render_stateful_widget(list, area, &mut list_state);
}

pub fn render_general_subpanel(frame: &mut Frame, area: Rect, workdir_display: &str) {
    let block = jackin_tui::components::Panel::new()
        .title(" General ")
        .focus(jackin_tui::components::PanelFocus::Unfocused)
        .block();
    let lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("Working dir ", Style::default().fg(jackin_tui::theme::WHITE)),
        Span::raw(workdir_display.to_string()),
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
    frame: &mut Frame,
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
    let lines: Vec<Line> = rows
        .iter()
        .map(|row| env_row_line(row, inner_width))
        .collect();

    let panel = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN));
    frame.render_widget(panel, area);
}

pub fn render_mounts_subpanel(
    frame: &mut Frame,
    area: Rect,
    rows: &[MountDisplayRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mut lines: Vec<Line> = Vec::new();
    if rows.is_empty() {
        lines.push(render_mount_header(mount_path_width(&[])));
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        )));
    } else {
        let path_w = mount_path_width(rows);
        lines.push(render_mount_header(path_w));
        lines.extend(render_mount_lines(rows, path_w));
    }
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        scroll_x,
        scroll_y,
        focused,
        Some(" Mounts "),
    );
}

pub fn render_global_mounts_subpanel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    rows: &[MountDisplayRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mut lines = Vec::new();
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
        )));
    } else {
        let path_w = mount_path_width(rows);
        lines.push(render_global_mount_header(path_w));
        lines.extend(render_global_mount_lines(rows, path_w));
    }
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        scroll_x,
        scroll_y,
        focused,
        Some(title),
    );
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
mod tests {
    use super::*;

    #[test]
    fn workspace_delete_confirm_state_names_workspace() {
        let state = workspace_delete_confirm_state("alpha");

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm");
        };
        assert_eq!(prompt, "Delete \"alpha\"?");
    }

    #[test]
    fn instance_purge_confirm_state_names_label_and_scope() {
        let state = instance_purge_confirm_state("role/dev");

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm");
        };
        assert!(prompt.starts_with("Purge \"role/dev\"?"));
        assert!(prompt.contains("local recovery state"));
    }

    #[test]
    fn create_prelude_input_helpers_name_fields() {
        let dst = create_prelude_mount_destination_input_state("/workspace");
        let name = create_prelude_workspace_name_input_state("project");

        assert_eq!(dst.label, "Destination");
        assert_eq!(dst.value(), "/workspace");
        assert_eq!(name.label, "Name this workspace");
        assert_eq!(name.value(), "project");
    }

    #[test]
    fn create_prelude_mount_dst_choice_uses_source() {
        let state = create_prelude_mount_dst_choice_state("/host/project");

        assert_eq!(state.src, "/host/project");
    }

    #[test]
    fn launch_provider_picker_uses_single_word_title() {
        assert_eq!(provider_picker_title(None), " provider ");
    }

    #[test]
    fn inline_provider_picker_keeps_instance_context() {
        assert_eq!(provider_picker_title(Some("abc123")), " abc123 — provider ");
    }
}
