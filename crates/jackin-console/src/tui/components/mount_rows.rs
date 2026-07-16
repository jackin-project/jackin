// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared mount-table row render helpers.

use crate::tui::mount_display::MountDisplayRow;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// "Mode" header is 4 chars; pad row values so the Type column stays aligned.
pub const MOUNT_MODE_COL_WIDTH: usize = 4;

/// Width of the `Isolation` column, pinned to the widest known value/header.
pub const MOUNT_ISOLATION_COL_WIDTH: usize = 9;

#[must_use]
pub fn render_mount_header(path_w: usize) -> Line<'static> {
    let mode_col = format!("{:<mw$}", "Mode", mw = MOUNT_MODE_COL_WIDTH);
    let iso_col = format!("{:<iw$}", "Isolation", iw = MOUNT_ISOLATION_COL_WIDTH);
    Line::from(Span::styled(
        format!(
            "  {path:<path_w$}  {mode_col}  {iso_col}  Type",
            path = "Destination"
        ),
        Style::default().fg(termrock::Theme::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default()),
    ))
}

#[must_use]
pub fn render_mount_lines(rows: &[MountDisplayRow], path_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<path_w$}  ", row.destination)),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                termrock::Theme::default().style(termrock::style::Role::TextMuted),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<MOUNT_ISOLATION_COL_WIDTH$}", row.isolation),
                termrock::Theme::default().style(termrock::style::Role::TextMuted),
            ),
            Span::raw("  "),
            Span::styled(
                row.kind.clone(),
                Style::default()
                    .fg(termrock::Theme::default()
                        .style(termrock::style::Role::TextMuted)
                        .fg
                        .unwrap_or_default())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                termrock::Theme::default().style(termrock::style::Role::TextMuted),
            )));
        }
    }
    lines
}

#[must_use]
pub fn render_global_mount_header(path_w: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {path:<path_w$}  Mode", path = "Destination"),
        Style::default().fg(termrock::Theme::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default()),
    ))
}

#[must_use]
pub fn render_global_mount_lines(rows: &[MountDisplayRow], path_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<path_w$}  ", row.destination)),
            Span::styled(
                row.mode.to_owned(),
                termrock::Theme::default().style(termrock::style::Role::TextMuted),
            ),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                termrock::Theme::default().style(termrock::style::Role::TextMuted),
            )));
        }
    }
    lines
}

#[cfg(test)]
mod tests;
