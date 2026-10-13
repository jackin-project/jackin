// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Cursor, tab-strip, and width primitives.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

pub const AUTH_LABEL_COL_WIDTH: usize = 14;
pub const SECRET_LABEL_COL_WIDTH: usize = 22;

#[must_use]
pub const fn cursor_gutter(selected: bool) -> &'static str {
    if selected { "\u{25b8} " } else { "  " }
}

#[must_use]
pub fn cursor_span(selected: bool) -> Span<'static> {
    if selected {
        Span::styled(
            cursor_gutter(true),
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        )
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
        termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong)
    } else {
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default())
    };
    let value_style = match (selected, emphasis) {
        (true, FieldEmphasis::SelectedValue) => Style::default()
            .fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default()),
    };
    Line::from(vec![
        Span::raw(cursor_gutter(selected).to_owned()),
        Span::styled(format!("{indent}{label:<label_width$}"), label_style),
        Span::styled(value.into(), value_style),
    ])
}

pub fn render_tab_strip(
    frame: &mut Frame<'_>,
    area: Rect,
    labels: &[(&str, bool)],
    tab_bar_focused: bool,
    hovered: Option<usize>,
) {
    let tabs = labels
        .iter()
        .enumerate()
        .map(|(id, (label, active))| termrock::widgets::Tab::new(id, label).active(*active))
        .collect::<Vec<_>>();
    let mut tabs_state = termrock::widgets::TabsState::new();
    tabs_state.selected = tabs.iter().find(|tab| tab.active).map(|tab| tab.id);
    tabs_state.hovered = hovered;
    tabs_state.focused = tab_bar_focused;
    frame.render_stateful_widget(
        &termrock::widgets::Tabs::new(&tabs, &termrock::style::DesignSystem::default())
            .gap(termrock::widgets::TAB_GAP),
        area,
        &mut tabs_state,
    );
}

pub(crate) fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

pub(crate) const fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

pub(crate) fn text_width(text: &str) -> usize {
    termrock::text::display_cols(text)
}
