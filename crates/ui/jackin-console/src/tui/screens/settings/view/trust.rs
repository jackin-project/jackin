// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Trust tab lines.

use super::truncate;

use super::super::model::SettingsTrustRow;
use super::super::model::SettingsTrustState;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

#[must_use]
pub fn trust_lines(
    rows: &[SettingsTrustRow],
    selected_row: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "  Role                         Trust      Git",
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default()),
    ))];
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default()),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (selected_row == i);
        let mut style = if selected {
            Style::default()
                .fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
        };
        if !selected && hovered_row == Some(i) {
            style = style.bg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TabInactiveHovered)
                .bg
                .unwrap_or_default());
        }
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let trust = if row.trusted { "trusted" } else { "untrusted" };
        lines.push(Line::from(Span::styled(
            format!(
                "{prefix}{:<28} {:<10} {}",
                truncate(&row.role, 28),
                trust,
                row.git
            ),
            style,
        )));
    }
    lines
}

#[must_use]
pub fn trust_state_lines(
    state: &SettingsTrustState,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    trust_lines(&state.pending, state.selected, hovered_row, show_cursor)
}
