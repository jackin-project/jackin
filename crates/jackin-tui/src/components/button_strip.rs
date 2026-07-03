//! Shared centered button row.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

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
}

impl Widget for ButtonStrip<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.line())
            .alignment(Alignment::Center)
            .render(area, buf);
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
        spans.push(Span::styled(format!("  {}  ", item.label), style));
    }
    Line::from(spans)
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
