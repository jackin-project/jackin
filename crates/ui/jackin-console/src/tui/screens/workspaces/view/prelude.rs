// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Create-prelude states and summaries.

use super::panel;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

#[must_use]
pub fn create_prelude_mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> crate::tui::components::TextInputState<'a> {
    crate::tui::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn create_prelude_mount_destination_default(src_display: Option<&str>) -> String {
    src_display.unwrap_or_default().to_owned()
}

#[must_use]
pub fn create_prelude_workspace_name_input_state<'a>(
    current: impl Into<String>,
) -> crate::tui::components::TextInputState<'a> {
    crate::tui::components::TextInputState::new("Name this workspace", current)
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
        .border_style(Style::default().fg(jackin_tui::tokens::CYAN))
        .title(Span::styled(
            " Running ",
            Style::default()
                .fg(jackin_tui::tokens::CYAN)
                .add_modifier(Modifier::BOLD),
        ));
    let plural = if count == 1 { "instance" } else { "instances" };
    let line = Line::from(vec![
        Span::styled("  ● ", Style::default().fg(jackin_tui::tokens::CYAN)),
        Span::styled(
            format!("{count} {plural} running"),
            Style::default().fg(jackin_tui::tokens::CYAN),
        ),
        Span::styled(
            if expanded {
                "  ·  ↓ navigate instances"
            } else {
                "  ·  → expand"
            },
            Style::default().fg(jackin_tui::tokens::CYAN_DIM),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![line])
            .block(block)
            .style(Style::default().fg(jackin_tui::tokens::CYAN)),
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

    let theme = termrock::style::DesignSystem::default();
    let intro_block = panel(&theme, Some(" What is a workspace? "), false).block();
    let intro_lines = vec![
        Line::from(Span::styled(
            "  A workspace saves a project boundary once so you",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default()),
        )),
        Line::from(Span::styled(
            "  can launch roles into it from anywhere \u{2014} without",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default()),
        )),
        Line::from(Span::styled(
            "  retyping mount paths.",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default()),
        )),
    ];
    frame.render_widget(Paragraph::new(intro_lines).block(intro_block), rows[0]);

    let why_block = panel(&theme, Some(" Why create one? "), false).block();
    let bullet_style = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::Accent)
        .fg
        .unwrap_or_default());
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
