// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared centered button row.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::display_cols;
use crate::theme::{INK, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonStripItem<'a> {
    pub label: &'a str,
    pub disabled: bool,
}

impl<'a> ButtonStripItem<'a> {
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            disabled: false,
        }
    }

    #[must_use]
    pub const fn disabled(label: &'a str) -> Self {
        Self {
            label,
            disabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ButtonStrip<'a> {
    items: &'a [ButtonStripItem<'a>],
    focused: usize,
    gap: &'a str,
}

impl<'a> ButtonStrip<'a> {
    #[must_use]
    pub const fn new(items: &'a [ButtonStripItem<'a>]) -> Self {
        Self {
            items,
            focused: 0,
            gap: "    ",
        }
    }

    #[must_use]
    pub const fn focused(mut self, focused: usize) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub const fn gap(mut self, gap: &'a str) -> Self {
        self.gap = gap;
        self
    }

    #[must_use]
    pub fn line(self) -> Line<'static> {
        button_strip_line(self.items, self.focused, self.gap)
    }

    #[must_use]
    pub fn button_rects(self, area: Rect) -> Vec<Rect> {
        button_rects(area, self.items, self.gap)
    }
}

impl Widget for ButtonStrip<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let rects = self.button_rects(area);
        for (idx, (item, rect)) in self.items.iter().zip(rects.iter()).enumerate() {
            if rect.width == 0 {
                continue;
            }
            buf.set_string(
                rect.x,
                rect.y,
                button_label(item.label),
                button_style(idx == self.focused, item.disabled),
            );
        }
    }
}

#[must_use]
pub fn button_strip_line(
    items: &[ButtonStripItem<'_>],
    focused: usize,
    gap: &str,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(gap.to_owned()));
        }
        let style = button_style(idx == focused, item.disabled);
        spans.push(Span::styled(button_label(item.label), style));
    }
    Line::from(spans)
}

#[must_use]
pub fn button_rects(area: Rect, items: &[ButtonStripItem<'_>], gap: &str) -> Vec<Rect> {
    let total_cols = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let button_cols = display_cols(&button_label(item.label));
            let gap_cols = if idx == 0 { 0 } else { display_cols(gap) };
            button_cols.saturating_add(gap_cols)
        })
        .sum::<usize>();
    let start = area.x.saturating_add(
        u16::try_from(usize::from(area.width).saturating_sub(total_cols) / 2).unwrap_or(0),
    );
    let mut x = start;
    let mut rects = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            x = x.saturating_add(u16::try_from(display_cols(gap)).unwrap_or(u16::MAX));
        }
        let width = u16::try_from(display_cols(&button_label(item.label))).unwrap_or(u16::MAX);
        rects.push(Rect {
            x,
            y: area.y,
            width,
            height: area.height.min(1),
        });
        x = x.saturating_add(width);
    }
    rects
}

#[must_use]
pub fn button_label(label: &str) -> String {
    format!("  {label}  ")
}

#[must_use]
pub fn button_style(focused: bool, disabled: bool) -> Style {
    if disabled {
        return Style::default()
            .fg(PHOSPHOR_DARK)
            .add_modifier(Modifier::DIM);
    }
    if focused {
        Style::default()
            .bg(WHITE)
            .fg(INK)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests;
