// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Secret line building and rendering.

use super::{
    SECRET_LABEL_COL_WIDTH, SecretEnvLineFrame, SecretLineRow, SecretValueDisplay,
    action_row_style, cursor_gutter, disclosure_style,
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;

#[must_use]
pub fn secret_env_lines<'a, S>(
    rows: &[SecretLineRow<S>],
    frame: SecretEnvLineFrame,
    value_for: impl Fn(&S, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&S, &str) -> bool,
    role_in_registry: impl Fn(&str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(rows.len());

    for (i, row) in rows.iter().enumerate() {
        let selected = frame.show_cursor && (i == frame.cursor);
        let gutter = cursor_gutter(selected);
        match row {
            SecretLineRow::Key { scope, key } => {
                let Some(value) = value_for(scope, key) else {
                    continue;
                };
                let account_owned = jackin_core::is_account_env(key);
                let value = if account_owned {
                    SecretValueDisplay::Plain("[account credential hidden]")
                } else {
                    value
                };
                lines.push(render_secret_key_line(
                    selected,
                    gutter,
                    key,
                    value,
                    account_owned || !is_unmasked(scope, key),
                    frame.area_width,
                    SECRET_LABEL_COL_WIDTH,
                ));
            }
            SecretLineRow::WorkspaceAddSentinel => {
                lines.push(Line::from(Span::styled(
                    format!("{gutter}+ Add environment variable"),
                    action_row_style(selected),
                )));
            }
            SecretLineRow::RoleHeader { role, expanded } => {
                let arrow = if *expanded { "\u{25bc}" } else { "\u{25b6}" };
                let mut spans = vec![
                    Span::raw(format!("{gutter}     ")),
                    Span::styled(arrow, disclosure_style()),
                    Span::styled(
                        format!(" Role: {role}  ({} vars)", role_var_count(role)),
                        disclosure_style(),
                    ),
                ];
                if !role_in_registry(role) {
                    spans.push(Span::styled(
                        "  (not in registry)",
                        Style::default()
                            .fg(termrock::style::DesignSystem::default()
                                .style(termrock::style::Role::TextMuted)
                                .fg
                                .unwrap_or_default())
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
                lines.push(Line::from(spans));
            }
            SecretLineRow::RoleAddSentinel(role) => {
                lines.push(Line::from(Span::styled(
                    format!("{gutter}     + Add {role} environment variable"),
                    action_row_style(selected),
                )));
            }
            SecretLineRow::SectionSpacer => lines.push(Line::from("")),
        }
    }

    lines
}

/// `OpRef` rows skip masking and render as a breadcrumb (3-segment:
/// `vault / item -> field`, 4-segment adds `section`).
#[must_use]
pub fn render_secret_key_line(
    selected: bool,
    cursor_col: &str,
    key: &str,
    value: SecretValueDisplay<'_>,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> Line<'static> {
    pub(crate) const OP_MARKER: &str = "[op] ";
    pub(crate) const NO_MARKER: &str = "     ";
    pub(crate) const MASK: &str =
        "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}";
    pub(crate) const OP_REF_REPICK_PLACEHOLDER: &str = "<unparseable path \u{2014} re-pick>";

    let label_style = if selected {
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong)
    } else {
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default())
    };
    let dim = termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted);
    let op_breadcrumb = match value {
        SecretValueDisplay::OpRefPath(path) => jackin_core::parse_op_breadcrumb_path(path),
        SecretValueDisplay::Plain(_) => None,
    };
    let marker = if op_breadcrumb.is_some() {
        OP_MARKER
    } else {
        NO_MARKER
    };
    let mut spans = vec![
        Span::raw(cursor_col.to_owned()),
        Span::styled(marker.to_owned(), dim),
        Span::styled(format!("{key:label_width$}"), label_style),
        Span::raw("  "),
    ];

    if op_breadcrumb.is_some()
        && let SecretValueDisplay::OpRefPath(path) = value
    {
        push_op_breadcrumb_spans(&mut spans, path);
        return Line::from(spans);
    }

    let plain_str = match value {
        SecretValueDisplay::Plain(value) => value,
        SecretValueDisplay::OpRefPath(_) => OP_REF_REPICK_PLACEHOLDER,
    };

    let value_style = if masked {
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextMuted)
    } else if selected {
        Style::default()
            .fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::BOLD)
    } else {
        termrock::style::DesignSystem::default().style(termrock::style::Role::Accent)
    };

    let rendered_value: String = if masked {
        MASK.to_owned()
    } else {
        let budget = (area_width as usize)
            .saturating_sub(label_width)
            .saturating_sub(8)
            .max(1);
        if plain_str.chars().count() > budget {
            let mut s: String = plain_str.chars().take(budget.saturating_sub(1)).collect();
            s.push('\u{2026}');
            s
        } else {
            plain_str.to_owned()
        }
    };
    spans.push(Span::styled(rendered_value, value_style));
    Line::from(spans)
}
