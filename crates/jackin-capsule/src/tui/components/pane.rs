//! Custom pane-body widget for rendering terminal screen content into a Ratatui Buffer.
//!
//! Blits `DamageGrid` cells directly into the Ratatui Buffer so the existing
//! `SocketBackend` diff mechanism handles terminal output.

use jackin_term::{Color as TermColor, GridSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    widgets::Widget,
};

/// A Ratatui widget that renders a [`GridSnapshot`] (from `DamageGrid::dump()`)
/// into the given area.
#[derive(Debug)]
pub struct PaneBodyWidget<'a> {
    snapshot: &'a GridSnapshot,
}

impl<'a> PaneBodyWidget<'a> {
    #[must_use]
    pub const fn new(snapshot: &'a GridSnapshot) -> Self {
        Self { snapshot }
    }
}

impl Widget for PaneBodyWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let screen_rows = self.snapshot.rows;
        let screen_cols = self.snapshot.cols;

        for row in 0..area.height {
            for col in 0..area.width {
                let buf_cell = &mut buf[(area.x + col, area.y + row)];

                if row < screen_rows && col < screen_cols {
                    let Some(cell) = self.snapshot.cell(row, col) else {
                        buf_cell.reset();
                        continue;
                    };
                    if cell.is_wide_continuation {
                        buf_cell.reset();
                        continue;
                    }

                    if cell.text.is_empty() {
                        buf_cell.set_char(' ');
                    } else {
                        buf_cell.set_symbol(&cell.text);
                    }

                    buf_cell.set_fg(term_color(cell.fg));
                    buf_cell.set_bg(term_color(cell.bg));

                    let mut modifier = Modifier::empty();
                    if cell.bold {
                        modifier |= Modifier::BOLD;
                    }
                    if cell.italic {
                        modifier |= Modifier::ITALIC;
                    }
                    if cell.underline {
                        modifier |= Modifier::UNDERLINED;
                    }
                    if cell.inverse {
                        modifier |= Modifier::REVERSED;
                    }
                    if cell.dim {
                        modifier |= Modifier::DIM;
                    }
                    buf_cell.modifier = modifier;
                } else {
                    buf_cell.reset();
                }
            }
        }
    }
}

fn term_color(color: TermColor) -> Color {
    match color {
        TermColor::Default => Color::Reset,
        TermColor::Idx(idx) => Color::Indexed(idx),
        TermColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests;
