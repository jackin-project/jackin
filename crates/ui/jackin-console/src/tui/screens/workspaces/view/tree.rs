// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace tree line builders.

use super::{WorkspaceListDisplayRow, row_fg};
use crate::tui::components::editor_rows::{action_row_style, cursor_gutter};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub(crate) fn push_tree_workspace_line(
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
        let cursor_col = cursor_gutter(row.selected && show_cursor);
        *max_w = (*max_w).max(2 + termrock::text::display_cols(&row.label));
        lines.push(Line::from(vec![
            Span::styled(cursor_col, action_row_style(row.selected)),
            Span::styled(row.label.clone(), action_row_style(row.selected)),
        ]));
        return;
    }
    let disclosure = row.disclosure;
    let color = row_fg(row);
    let line = if let Some(arrow) = disclosure.glyph() {
        let text_w = 1 + 1 + 1 + termrock::text::display_cols(&row.label);
        *max_w = (*max_w).max(text_w);
        if row.selected {
            Line::from(vec![
                Span::styled(
                    cursor,
                    Style::default()
                        .bg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Accent)
                            .fg
                            .unwrap_or_default())
                        .fg(Color::Black),
                ),
                Span::styled(
                    arrow,
                    Style::default()
                        .bg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Accent)
                            .fg
                            .unwrap_or_default())
                        .fg(Color::Black),
                ),
                Span::styled(
                    format!(" {}", row.label),
                    Style::default()
                        .bg(termrock::style::DesignSystem::default()
                            .style(termrock::style::Role::Accent)
                            .fg
                            .unwrap_or_default())
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
        let text_w = 3 + termrock::text::display_cols(&row.label);
        *max_w = (*max_w).max(text_w);
        if row.selected {
            Line::from(Span::styled(
                format!("{cursor}  {}", row.label),
                Style::default()
                    .bg(termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::Accent)
                        .fg
                        .unwrap_or_default())
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

pub(crate) fn push_tree_instance_line(
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
    let text_w = 1 + 4 + termrock::text::display_cols(&row.label);
    *max_w = (*max_w).max(text_w);

    let line = if row.selected {
        Line::from(Span::styled(
            format!("{cursor}    {}", row.label),
            Style::default()
                .bg(jackin_tui::tokens::CYAN)
                .fg(Color::Black),
        ))
    } else {
        let mut parts = row.label.splitn(2, "  ");
        let instance_id = parts.next().unwrap_or_default();
        let role_key = parts.next().unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!("{cursor}    "),
                Style::default().fg(jackin_tui::tokens::CYAN_DIM),
            ),
            Span::styled(
                instance_id.to_owned(),
                Style::default().fg(jackin_tui::tokens::CYAN_DIM),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                role_key.to_owned(),
                Style::default().fg(jackin_tui::tokens::CYAN),
            ),
        ])
    };
    lines.push(line);
}
