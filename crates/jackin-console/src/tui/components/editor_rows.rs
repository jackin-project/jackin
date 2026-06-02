//! Shared row render helpers for editor/settings tabs.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::theme::{ACTION_ACCENT, DISCLOSURE_ACCENT, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

use crate::tui::components::op_breadcrumb::push_op_breadcrumb_spans;

pub enum SecretValueDisplay<'a> {
    Plain(&'a str),
    OpRefPath(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceDisplay {
    NotRequired,
    OpRefPath(String),
    MaskedPlain { chars: usize },
    Unset { env_name: String, mode_label: String },
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
    let style = Style::default().fg(ACTION_ACCENT);
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

#[must_use]
pub fn disclosure_style() -> Style {
    Style::default()
        .fg(DISCLOSURE_ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn render_tab_strip(
    frame: &mut Frame,
    area: Rect,
    labels: &[(&str, bool)],
    tab_bar_focused: bool,
    hovered: Option<usize>,
) {
    jackin_tui::components::TabStrip::new(labels)
        .focused(tab_bar_focused)
        .hovered(hovered)
        .render(frame, area);
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
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let op_breadcrumb = match value {
        SecretValueDisplay::OpRefPath(path) => crate::tui::op_breadcrumb::parse_path_breadcrumb(path),
        SecretValueDisplay::Plain(_) => None,
    };
    let marker = if op_breadcrumb.is_some() {
        OP_MARKER
    } else {
        NO_MARKER
    };
    let mut spans = vec![
        Span::raw(cursor_col.to_string()),
        Span::styled(marker.to_string(), dim),
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
        Style::default().fg(PHOSPHOR_DIM)
    } else if selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_GREEN)
    };

    let rendered_value: String = if masked {
        MASK.to_string()
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
            plain_str.to_string()
        }
    };
    spans.push(Span::styled(rendered_value, value_style));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_source_display_maps_secret_value_state() {
        assert_eq!(
            auth_source_display(
                Some(AuthSourceValue::Plain("secret".to_string())),
                "API_KEY",
                "api-key",
            ),
            AuthSourceDisplay::MaskedPlain { chars: 6 },
        );
        assert_eq!(
            auth_source_display(
                Some(AuthSourceValue::OpRefPath("Vault/Item/key".to_string())),
                "API_KEY",
                "api-key",
            ),
            AuthSourceDisplay::OpRefPath("Vault/Item/key".to_string()),
        );
        assert_eq!(
            auth_source_display(
                Some(AuthSourceValue::Plain(String::new())),
                "API_KEY",
                "api-key",
            ),
            AuthSourceDisplay::Unset {
                env_name: "API_KEY".to_string(),
                mode_label: "api-key".to_string(),
            },
        );
    }

    #[test]
    fn auth_source_display_returns_not_required_without_env() {
        assert_eq!(
            auth_source_display_for_required_env(
                None,
                Some(AuthSourceValue::Plain("secret".to_string())),
                "ignore",
            ),
            AuthSourceDisplay::NotRequired,
        );
    }
}
