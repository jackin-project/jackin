// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Instance detail line builders.

use super::{
    WorkspaceEnvRow, WorkspaceInstancePaneContent, WorkspaceInstanceSessionRow,
    WorkspaceInstanceTab, workspace_instance_pane_identity_label,
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub(crate) fn instance_detail_lines(content: &WorkspaceInstancePaneContent) -> Vec<Line<'static>> {
    match content {
        WorkspaceInstancePaneContent::Live { tabs } => live_instance_lines(tabs),
        WorkspaceInstancePaneContent::Sessions { rows } => session_instance_lines(rows),
        WorkspaceInstancePaneContent::Empty { message } => vec![Line::from(Span::styled(
            format!("  {message}"),
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default()),
        ))],
    }
}

pub(crate) fn live_instance_lines(tabs: &[WorkspaceInstanceTab]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if tabs.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Daemon reports no tabs",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default()),
        )));
        return lines;
    }

    lines.push(Line::from(Span::styled(
        "  Live tab/pane tree (from container daemon)",
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
    )));
    for tab in tabs {
        let prefix = if tab.active { "▸" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {prefix} Tab {}:  ", tab.index + 1),
                Style::default().fg(if tab.active {
                    termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::Accent)
                        .fg
                        .unwrap_or_default()
                } else {
                    termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::TextMuted)
                        .fg
                        .unwrap_or_default()
                }),
            ),
            Span::styled(
                tab.label.clone(),
                if tab.active {
                    termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::TextStrong)
                } else {
                    termrock::style::DesignSystem::default().style(termrock::style::Role::Text)
                },
            ),
        ]));
        for pane in &tab.panes {
            let marker = if pane.focused { "●" } else { "○" };
            let cursor_prefix = if pane.selected { "▶ " } else { "  " };
            let label_style = if pane.selected {
                Style::default()
                    .fg(termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::Text)
                        .fg
                        .unwrap_or_default())
                    .bg(termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::ScrollTrack)
                        .fg
                        .unwrap_or_default())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default())
            };
            let identity_label = workspace_instance_pane_identity_label(
                pane.account_id.as_deref(),
                pane.config_id.as_deref(),
            );
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {cursor_prefix}{marker} "),
                    Style::default().fg(if pane.focused {
                        termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Accent)
                            .fg
                            .unwrap_or_default()
                    } else {
                        termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::TextMuted)
                            .fg
                            .unwrap_or_default()
                    }),
                ),
                Span::styled(format!("{:<16}", pane.label), label_style),
                Span::styled(
                    format!("  ({identity_label}) "),
                    Style::default().fg(termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::TextMuted)
                        .fg
                        .unwrap_or_default()),
                ),
                Span::styled(
                    format!("[{}]", pane.state_label),
                    Style::default().fg(termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::TextMuted)
                        .fg
                        .unwrap_or_default()),
                ),
            ]));
        }
    }
    lines
}

pub(crate) fn session_instance_lines(rows: &[WorkspaceInstanceSessionRow]) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!("  {:<24}  Identity", "Session"),
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
    ))];
    for row in rows {
        let name = if row.name.chars().count() > 24 {
            let cut: String = row.name.chars().take(23).collect();
            format!("{cut}…")
        } else {
            row.name.clone()
        };
        let identity_label = if row.account_id.is_some() || row.config_id.is_some() {
            workspace_instance_pane_identity_label(
                row.account_id.as_deref(),
                row.config_id.as_deref(),
            )
        } else if row.agent_runtime.is_empty() {
            workspace_instance_pane_identity_label(None, None)
        } else {
            row.agent_runtime.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {name:<24}  "),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default()),
            ),
            Span::styled(
                identity_label,
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ),
        ]));
    }
    lines
}

pub(crate) fn env_row_line(row: &WorkspaceEnvRow, inner_width: usize) -> Line<'static> {
    pub(crate) const SUBPANEL_CONTENT_INDENT: usize = 2;
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
                .fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default())
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        spans.push(Span::raw(marker_text));
    }
    spans.push(Span::raw(gap));
    spans.push(Span::styled(
        row.name.clone(),
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default()),
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
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default()),
        ));
    }

    Line::from(spans)
}
