//! Shared row render helpers for editor/settings tabs.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::theme::{ACTION_ACCENT, DISCLOSURE_ACCENT, PHOSPHOR_GREEN, WHITE};

use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;

pub const AUTH_LABEL_COL_WIDTH: usize = 14;

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
        jackin_tui::theme::BOLD_WHITE
    } else {
        Style::default().fg(WHITE)
    };
    let dim = jackin_tui::theme::DIM;
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
        jackin_tui::theme::DIM
    } else if selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        jackin_tui::theme::GREEN
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

#[cfg(test)]
mod tests;
