// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};
use termrock::interaction::HitRegion;

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
    let (tabs, mut tabs_state) = tab_strip_parts(labels, tab_bar_focused, hovered);
    let system = termrock::style::DesignSystem::default();
    frame.render_stateful_widget(
        termrock::widgets::Tabs::new(&tabs, &system).gap(termrock::widgets::TAB_GAP),
        area,
        &mut tabs_state,
    );
}

pub(crate) fn tab_strip_hit_regions(
    area: Rect,
    labels: &[(&str, bool)],
    tab_bar_focused: bool,
    hovered: Option<usize>,
) -> Vec<HitRegion<usize>> {
    if area.is_empty() {
        return Vec::new();
    }

    let (tabs, mut tabs_state) = tab_strip_parts(labels, tab_bar_focused, hovered);
    let system = termrock::style::DesignSystem::default();
    let mut buffer = Buffer::empty(area);
    termrock::widgets::Tabs::new(&tabs, &system)
        .gap(termrock::widgets::TAB_GAP)
        .paint(area, &mut buffer, &mut tabs_state);

    let trigger = tabs_state.overflow_trigger;
    let id_limit = labels.len();
    tabs_state
        .regions
        .into_iter()
        .filter(|region| region.id < id_limit)
        .flat_map(|region| {
            let Some(region_area) = intersect_rect(region.area, area) else {
                return Vec::<HitRegion<usize>>::new();
            };
            subtract_rect(region_area, trigger.unwrap_or_default())
                .into_iter()
                .map(move |area| HitRegion {
                    id: region.id,
                    area,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn tab_strip_parts<'a>(
    labels: &[(&'a str, bool)],
    tab_bar_focused: bool,
    hovered: Option<usize>,
) -> (
    Vec<termrock::widgets::Tab<'a, usize>>,
    termrock::widgets::TabsState<usize>,
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
    (tabs, tabs_state)
}

fn intersect_rect(rect: Rect, bounds: Rect) -> Option<Rect> {
    let x = rect.x.max(bounds.x);
    let y = rect.y.max(bounds.y);
    let right = rect.right().min(bounds.right());
    let bottom = rect.bottom().min(bounds.bottom());
    let width = right.saturating_sub(x);
    let height = bottom.saturating_sub(y);
    let intersection = Rect::new(x, y, width, height);
    (!intersection.is_empty()).then_some(intersection)
}

fn subtract_rect(rect: Rect, cut: Rect) -> Vec<Rect> {
    let Some(cut) = intersect_rect(rect, cut) else {
        return vec![rect];
    };

    let mut pieces = Vec::with_capacity(4);
    if cut.y > rect.y {
        pieces.push(Rect::new(
            rect.x,
            rect.y,
            rect.width,
            cut.y.saturating_sub(rect.y),
        ));
    }
    if cut.bottom() < rect.bottom() {
        pieces.push(Rect::new(
            rect.x,
            cut.bottom(),
            rect.width,
            rect.bottom().saturating_sub(cut.bottom()),
        ));
    }
    if cut.x > rect.x {
        pieces.push(Rect::new(
            rect.x,
            cut.y,
            cut.x.saturating_sub(rect.x),
            cut.height,
        ));
    }
    if cut.right() < rect.right() {
        pieces.push(Rect::new(
            cut.right(),
            cut.y,
            rect.right().saturating_sub(cut.right()),
            cut.height,
        ));
    }
    pieces
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
