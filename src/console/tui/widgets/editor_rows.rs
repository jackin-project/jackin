//! Shared row render helpers for editor/settings tabs.

use crate::console::widgets::{
    PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, op_breadcrumb::push_op_breadcrumb_spans,
};
use crate::operator_env::EnvValue;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::theme::{ACTION_ACCENT, DISCLOSURE_ACCENT};

pub(crate) fn action_row_style(selected: bool) -> Style {
    let style = Style::default().fg(ACTION_ACCENT);
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

pub(crate) fn disclosure_style() -> Style {
    Style::default()
        .fg(DISCLOSURE_ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn render_tab_strip(
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
/// `vault / item → field`, 4-segment adds `section`).
pub(crate) fn render_secret_key_line(
    selected: bool,
    cursor_col: &str,
    key: &str,
    value: &EnvValue,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> Line<'static> {
    const OP_MARKER: &str = "[op] ";
    const NO_MARKER: &str = "     ";
    const MASK: &str = "●●●●●●●●●●●";
    const OP_REF_REPICK_PLACEHOLDER: &str = "<unparseable path \u{2014} re-pick>";

    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let dim = Style::default().fg(PHOSPHOR_DIM);
    let op_breadcrumb = match value {
        EnvValue::OpRef(r) => {
            crate::console::manager::op_breadcrumb::parse_path_breadcrumb(&r.path)
        }
        EnvValue::Plain(_) => None,
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
        && let EnvValue::OpRef(r) = value
    {
        push_op_breadcrumb_spans(&mut spans, &r.path);
        return Line::from(spans);
    }

    let plain_str = match value {
        EnvValue::Plain(s) => s.as_str(),
        EnvValue::OpRef(_) => OP_REF_REPICK_PLACEHOLDER,
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
            s.push('…');
            s
        } else {
            plain_str.to_string()
        }
    };
    spans.push(Span::styled(rendered_value, value_style));
    Line::from(spans)
}
