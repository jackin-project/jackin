// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth line building and rendering.

use super::{
    AUTH_LABEL_COL_WIDTH, AuthLineRow, AuthSourceDisplay, AuthSourceFolderDisplay,
    AuthSourceFolderKind, action_row_style, cursor_gutter, disclosure_style, padded_width,
    padded_width_cols, text_width,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;

#[must_use]
pub fn auth_lines(rows: &[AuthLineRow], cursor: usize, show_cursor: bool) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| render_auth_line(show_cursor && (i == cursor), row))
        .collect()
}

#[must_use]
pub fn auth_line_width(row: &AuthLineRow) -> usize {
    match row {
        AuthLineRow::AuthKind { label } => padded_width(&format!("  {label}")),
        AuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let suffix = if *inherited { " (inherited)" } else { "" };
            padded_width(&format!(
                "  {:<AUTH_LABEL_COL_WIDTH$}{mode_label}{suffix}",
                "Mode"
            ))
        }
        AuthLineRow::WorkspaceSource { display } => auth_source_line_width("Source", display, 0),
        AuthLineRow::WorkspaceSourceFolder { display } => {
            source_folder_line_width("Source folder", display, 0)
        }
        AuthLineRow::RoleHeader { role, .. } => padded_width(&format!("\u{25bc} Role: {role}")),
        AuthLineRow::RoleMode { mode_label } => padded_width(&format!(
            "      {:<AUTH_LABEL_COL_WIDTH$}{mode_label}",
            "Mode"
        )),
        AuthLineRow::RoleSource { display } => auth_source_line_width("Source", display, 6),
        AuthLineRow::RoleSourceFolder { display } => {
            source_folder_line_width("Source folder", display, 6)
        }
        AuthLineRow::AddSentinel { .. } => padded_width("  + Override for a role"),
        AuthLineRow::Spacer => 0,
    }
}

pub(crate) fn render_auth_line(selected: bool, row: &AuthLineRow) -> Line<'static> {
    let bold_white =
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong);
    let dim_green = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::TextMuted)
        .fg
        .unwrap_or_default());
    let phosphor = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::Accent)
        .fg
        .unwrap_or_default());

    match row {
        AuthLineRow::AuthKind { label } => Line::from(vec![
            Span::raw(cursor_gutter(selected)),
            Span::styled(label.clone(), bold_white),
        ]),
        AuthLineRow::WorkspaceMode {
            mode_label,
            inherited,
        } => {
            let suffix = if *inherited { " (inherited)" } else { "" };
            Line::from(vec![
                Span::raw(cursor_gutter(selected)),
                Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
                Span::styled(mode_label.clone(), phosphor),
                Span::styled(suffix.to_owned(), dim_green),
            ])
        }
        AuthLineRow::WorkspaceSource { display } => {
            render_auth_source_line("Source", display, 0, selected)
        }
        AuthLineRow::WorkspaceSourceFolder { display } => {
            render_source_folder_line("Source folder", display, 0, selected)
        }
        AuthLineRow::RoleHeader { role, expanded } => {
            let glyph = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
            Line::from(vec![
                Span::styled(glyph.to_owned(), disclosure_style()),
                Span::styled(format!(" Role: {role}"), disclosure_style()),
            ])
        }
        AuthLineRow::RoleMode { mode_label } => Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("{:<AUTH_LABEL_COL_WIDTH$}", "Mode"), bold_white),
            Span::styled(mode_label.clone(), phosphor),
        ]),
        AuthLineRow::RoleSource { display } => render_auth_source_line("Source", display, 6, false),
        AuthLineRow::RoleSourceFolder { display } => {
            render_source_folder_line("Source folder", display, 6, false)
        }
        AuthLineRow::AddSentinel { .. } => {
            let gutter = cursor_gutter(selected);
            Line::from(vec![
                Span::styled(gutter, action_row_style(selected)),
                Span::styled("+ Override for a role", action_row_style(selected)),
            ])
        }
        AuthLineRow::Spacer => Line::from(""),
    }
}

pub(crate) fn source_folder_line_width(
    label: &str,
    display: &AuthSourceFolderDisplay,
    indent: usize,
) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + text_width(&format!("{label:<label_width$}"));
    let value = source_folder_display_text(display);
    padded_width_cols(prefix_width + text_width(&value), gutter_width)
}

pub(crate) fn render_source_folder_line(
    label: &str,
    display: &AuthSourceFolderDisplay,
    indent: usize,
    selected: bool,
) -> Line<'static> {
    let prefix = if indent == 0 {
        cursor_gutter(selected).to_owned()
    } else {
        " ".repeat(indent)
    };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let value = source_folder_display_text(display);
    Line::from(vec![
        Span::raw(prefix),
        Span::styled(
            format!("{label:<label_width$}"),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ),
        Span::styled(
            value,
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default()),
        ),
    ])
}

pub(crate) fn source_folder_display_text(display: &AuthSourceFolderDisplay) -> String {
    match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    }
}

pub(crate) fn auth_source_line_width(
    label: &str,
    display: &AuthSourceDisplay,
    indent: usize,
) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + text_width(&format!("{label:<label_width$}"));
    let value_width = match display {
        AuthSourceDisplay::NotRequired => text_width("not required"),
        AuthSourceDisplay::OpRefPath(path) => {
            text_width("[op] ")
                + jackin_core::parse_op_breadcrumb_path(path).map_or_else(
                    || text_width("<unparseable path - re-pick>"),
                    |parts| crate::tui::op_breadcrumb::breadcrumb_display_width(&parts),
                )
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            text_width(&"\u{25cf}".repeat((*chars).clamp(1, 12)))
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => text_width(&format!("unset  ({env_name} for {mode_label})")),
    };
    padded_width_cols(prefix_width + value_width, gutter_width)
}

pub(crate) fn render_auth_source_line(
    label: &str,
    display: &AuthSourceDisplay,
    indent: usize,
    selected: bool,
) -> Line<'static> {
    let prefix = if indent == 0 {
        cursor_gutter(selected).to_owned()
    } else {
        " ".repeat(indent)
    };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let mut spans = vec![
        Span::raw(prefix),
        Span::styled(
            format!("{label:<label_width$}"),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled(
                "not required",
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled(
                "[op] ",
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ));
            push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ));
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => {
            spans.push(Span::styled(
                format!("unset  ({env_name} for {mode_label})"),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Danger)
                    .fg
                    .unwrap_or_default()),
            ));
        }
    }

    Line::from(spans)
}
