// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! The reusable part: a widget that paints a `vt100::Screen` into a ratatui `Buffer`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct TermScreen<'a> {
    pub screen: &'a vt100::Screen,
    pub color: bool,
}

fn color(value: vt100::Color) -> Color {
    match value {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn style(cell: &vt100::Cell, with_color: bool) -> Style {
    let mut style = Style::default();
    if with_color {
        style = style.fg(color(cell.fgcolor())).bg(color(cell.bgcolor()));
    }
    for (on, modifier) in [
        (cell.bold(), Modifier::BOLD),
        (cell.dim(), Modifier::DIM),
        (cell.italic(), Modifier::ITALIC),
        (cell.underline(), Modifier::UNDERLINED),
        (cell.inverse(), Modifier::REVERSED),
    ] {
        if on {
            style = style.add_modifier(modifier);
        }
    }
    style
}

impl Widget for TermScreen<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (rows, cols) = self.screen.size();
        for row in 0..area.height.min(rows.get()) {
            for col in 0..area.width.min(cols.get()) {
                let Some(cell) = self.screen.cell(row, col) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let target = &mut buf[(area.x + col, area.y + row)];
                let text = cell.contents();
                target.set_symbol(if text.is_empty() { " " } else { text });
                target.set_style(style(cell, self.color));
            }
        }
    }
}

/// Cursor position inside `area`, or None when the remote hid it or it is outside the pane.
pub fn cursor(screen: &vt100::Screen, area: Rect) -> Option<(u16, u16)> {
    if screen.hide_cursor() {
        return None;
    }
    let (row, col) = screen.cursor_position();
    (row < area.height && col < area.width).then_some((area.x + col, area.y + row))
}
