// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared row render helpers for editor/settings tabs.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use termrock::style::{ACTION_ACCENT, DISCLOSURE_ACCENT, PHOSPHOR_GREEN, WHITE};

use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;

pub const AUTH_LABEL_COL_WIDTH: usize = 14;
pub const SECRET_LABEL_COL_WIDTH: usize = 22;

#[must_use]
pub const fn cursor_gutter(selected: bool) -> &'static str {
    if selected { "\u{25b8} " } else { "  " }
}

#[must_use]
pub fn cursor_span(selected: bool) -> Span<'static> {
    if selected {
        Span::styled(cursor_gutter(true), termrock::style::BOLD_WHITE)
    } else {
        Span::raw(cursor_gutter(false))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldEmphasis {
    Normal,
    SelectedValue,
}

#[must_use]
pub fn labeled_field_line(
    selected: bool,
    indent: &str,
    label: &str,
    label_width: usize,
    value: impl Into<String>,
    emphasis: FieldEmphasis,
) -> Line<'static> {
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let value_style = match (selected, emphasis) {
        (true, FieldEmphasis::SelectedValue) => Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(PHOSPHOR_GREEN),
    };
    Line::from(vec![
        Span::raw(cursor_gutter(selected).to_owned()),
        Span::styled(format!("{indent}{label:<label_width$}"), label_style),
        Span::styled(value.into(), value_style),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretValueDisplay<'a> {
    Plain(&'a str),
    OpRefPath(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceDisplay {
    NotRequired,
    OpRefPath(String),
    MaskedPlain {
        chars: usize,
    },
    Unset {
        env_name: String,
        mode_label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceFolderKind {
    Default,
    Explicit,
    Inherited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSourceFolderDisplay {
    pub kind: AuthSourceFolderKind,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceValue {
    Plain(String),
    OpRefPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthLineRow {
    AuthKind { label: String },
    WorkspaceMode { mode_label: String, inherited: bool },
    WorkspaceSource { display: AuthSourceDisplay },
    WorkspaceSourceFolder { display: AuthSourceFolderDisplay },
    RoleHeader { role: String, expanded: bool },
    RoleMode { mode_label: String },
    RoleSource { display: AuthSourceDisplay },
    RoleSourceFolder { display: AuthSourceFolderDisplay },
    AddSentinel { eligible: usize },
    Spacer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretLineRow<S> {
    Key { scope: S, key: String },
    WorkspaceAddSentinel,
    RoleHeader { role: String, expanded: bool },
    RoleAddSentinel(String),
    SectionSpacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretEnvLineFrame {
    pub cursor: usize,
    pub show_cursor: bool,
    pub area_width: u16,
}

#[must_use]
pub fn auth_source_display(
    value: Option<AuthSourceValue>,
    env_name: impl Into<String>,
    mode_label: impl Into<String>,
) -> AuthSourceDisplay {
    match value {
        Some(AuthSourceValue::Plain(value)) if !value.is_empty() => {
            AuthSourceDisplay::MaskedPlain {
                chars: value.chars().count(),
            }
        }
        Some(AuthSourceValue::OpRefPath(path)) => AuthSourceDisplay::OpRefPath(path),
        _ => AuthSourceDisplay::Unset {
            env_name: env_name.into(),
            mode_label: mode_label.into(),
        },
    }
}

#[must_use]
pub fn auth_source_display_for_required_env(
    required_env_name: Option<&str>,
    value: Option<AuthSourceValue>,
    mode_label: impl Into<String>,
) -> AuthSourceDisplay {
    let Some(env_name) = required_env_name else {
        return AuthSourceDisplay::NotRequired;
    };
    auth_source_display(value, env_name, mode_label)
}

#[must_use]
pub fn action_row_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .bg(PHOSPHOR_GREEN)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACTION_ACCENT)
    }
}

#[must_use]
pub fn disclosure_style() -> Style {
    Style::default()
        .fg(DISCLOSURE_ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn render_tab_strip(
    frame: &mut Frame<'_>,
    area: Rect,
    labels: &[(&str, bool)],
    tab_bar_focused: bool,
    hovered: Option<usize>,
) {
    frame.render_widget(
        jackin_tui::components::TabStrip::new(labels)
            .focused(tab_bar_focused)
            .hovered(hovered),
        area,
    );
}

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

fn render_auth_line(selected: bool, row: &AuthLineRow) -> Line<'static> {
    let bold_white = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let dim_green = Style::default().fg(termrock::style::PHOSPHOR_DIM);
    let phosphor = Style::default().fg(PHOSPHOR_GREEN);

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

fn source_folder_line_width(
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

fn render_source_folder_line(
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
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, Style::default().fg(termrock::style::PHOSPHOR_DIM)),
    ])
}

fn source_folder_display_text(display: &AuthSourceFolderDisplay) -> String {
    match display.kind {
        AuthSourceFolderKind::Default => format!("default: {}", display.path),
        AuthSourceFolderKind::Explicit => display.path.clone(),
        AuthSourceFolderKind::Inherited => format!("inherited: {}", display.path),
    }
}

fn auth_source_line_width(label: &str, display: &AuthSourceDisplay, indent: usize) -> usize {
    let gutter_width = if indent == 0 { 2 } else { indent };
    let label_width = label.len().max(AUTH_LABEL_COL_WIDTH);
    let prefix_width = gutter_width + text_width(&format!("{label:<label_width$}"));
    let value_width = match display {
        AuthSourceDisplay::NotRequired => text_width("not required"),
        AuthSourceDisplay::OpRefPath(path) => {
            text_width("[op] ")
                + crate::tui::op_breadcrumb::parse_path_breadcrumb(path).map_or_else(
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

fn render_auth_source_line(
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
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ];

    match display {
        AuthSourceDisplay::NotRequired => {
            spans.push(Span::styled(
                "not required",
                Style::default().fg(termrock::style::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::OpRefPath(path) => {
            spans.push(Span::styled(
                "[op] ",
                Style::default().fg(termrock::style::PHOSPHOR_DIM),
            ));
            push_op_breadcrumb_spans(&mut spans, path);
        }
        AuthSourceDisplay::MaskedPlain { chars } => {
            spans.push(Span::styled(
                "\u{25cf}".repeat((*chars).clamp(1, 12)),
                Style::default().fg(termrock::style::PHOSPHOR_DIM),
            ));
        }
        AuthSourceDisplay::Unset {
            env_name,
            mode_label,
        } => {
            spans.push(Span::styled(
                format!("unset  ({env_name} for {mode_label})"),
                Style::default().fg(termrock::style::DANGER_RED),
            ));
        }
    }

    Line::from(spans)
}

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
                lines.push(render_secret_key_line(
                    selected,
                    gutter,
                    key,
                    value,
                    !is_unmasked(scope, key),
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
                            .fg(termrock::style::PHOSPHOR_DIM)
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
    const OP_MARKER: &str = "[op] ";
    const NO_MARKER: &str = "     ";
    const MASK: &str =
        "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}";
    const OP_REF_REPICK_PLACEHOLDER: &str = "<unparseable path \u{2014} re-pick>";

    let label_style = if selected {
        termrock::style::BOLD_WHITE
    } else {
        Style::default().fg(WHITE)
    };
    let dim = termrock::style::DIM;
    let op_breadcrumb = match value {
        SecretValueDisplay::OpRefPath(path) => {
            crate::tui::op_breadcrumb::parse_path_breadcrumb(path)
        }
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
        termrock::style::DIM
    } else if selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        termrock::style::GREEN
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

fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

const fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
}

#[cfg(test)]
mod tests;
