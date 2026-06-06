//! Custom pane-body widget for rendering terminal screen content into a Ratatui Buffer.
//!
//! Blits `DamageGrid` cells directly into the Ratatui Buffer so the existing
//! `SocketBackend` diff mechanism handles terminal output.

use jackin_term::{Color as TermColor, GridPatch, GridSnapshot, SnapCell};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    widgets::Widget,
};

#[derive(Debug)]
pub(crate) enum PaneBodyContent<'a> {
    Full(&'a GridSnapshot),
    Patch(&'a GridPatch),
}

/// A Ratatui widget that renders terminal body cells into the given area.
#[derive(Debug)]
pub struct PaneBodyWidget<'a> {
    content: PaneBodyContent<'a>,
}

impl<'a> PaneBodyWidget<'a> {
    #[must_use]
    pub const fn new(snapshot: &'a GridSnapshot) -> Self {
        Self {
            content: PaneBodyContent::Full(snapshot),
        }
    }

    #[must_use]
    pub const fn from_patch(patch: &'a GridPatch) -> Self {
        Self {
            content: PaneBodyContent::Patch(patch),
        }
    }
}

impl Widget for PaneBodyWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.content {
            PaneBodyContent::Full(snapshot) => render_full(snapshot, area, buf),
            PaneBodyContent::Patch(patch) => render_patch(patch, area, buf),
        }
    }
}

fn render_full(snapshot: &GridSnapshot, area: Rect, buf: &mut Buffer) {
    for row in 0..area.height {
        for col in 0..area.width {
            let buf_cell = &mut buf[(area.x + col, area.y + row)];

            if row < snapshot.rows && col < snapshot.cols {
                let Some(cell) = snapshot.cell(row, col) else {
                    buf_cell.reset();
                    continue;
                };
                render_cell(buf_cell, cell);
            } else {
                buf_cell.reset();
            }
        }
    }
}

fn render_patch(patch: &GridPatch, area: Rect, buf: &mut Buffer) {
    for row in 0..area.height {
        let Some(cells) = patch.row(row) else {
            continue;
        };
        for col in 0..area.width {
            let buf_cell = &mut buf[(area.x + col, area.y + row)];

            if row < patch.rows && col < patch.cols {
                let Some(cell) = cells.get(col as usize) else {
                    buf_cell.reset();
                    continue;
                };
                render_cell(buf_cell, cell);
            } else {
                buf_cell.reset();
            }
        }
    }
}

fn render_cell(buf_cell: &mut ratatui::buffer::Cell, cell: &SnapCell) {
    if cell.is_wide_continuation {
        buf_cell.reset();
        return;
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
