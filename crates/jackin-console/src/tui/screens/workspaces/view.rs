//! Workspaces screen view helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
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
}
