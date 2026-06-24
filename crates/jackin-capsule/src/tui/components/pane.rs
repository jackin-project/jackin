//! Custom pane-body widget for rendering terminal screen content into a Ratatui Buffer.
//!
//! Blits `DamageGrid` cells directly into the Ratatui Buffer so the existing
//! `SocketBackend` diff mechanism handles terminal output.

use crate::tui::socket_backend::term_color;
use std::num::NonZeroU16;

use jackin_term::{Cell as TermCell, Color as TermColor, GridSnapshot, GridView, SnapCell};
use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::Rect,
    style::Modifier,
    widgets::Widget,
};

#[derive(Debug)]
pub(crate) enum PaneBodyContent<'a> {
    Full(&'a GridSnapshot),
    View(&'a GridView<'a>),
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
    pub const fn view(view: &'a GridView<'a>) -> Self {
        Self {
            content: PaneBodyContent::View(view),
        }
    }
}

impl Widget for PaneBodyWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.content {
            PaneBodyContent::Full(snapshot) => render_full(snapshot, area, buf),
            PaneBodyContent::View(view) => render_view(view, area, buf),
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
    debug_assert_pane_area_well_formed(area, buf);
}

fn render_view(view: &GridView<'_>, area: Rect, buf: &mut Buffer) {
    for row in 0..area.height {
        for col in 0..area.width {
            let buf_cell = &mut buf[(area.x + col, area.y + row)];

            if row < view.rows && col < view.cols {
                let Some(cell) = view.cell(row, col) else {
                    buf_cell.reset();
                    continue;
                };
                render_cell(buf_cell, cell);
            } else {
                buf_cell.reset();
            }
        }
    }
    debug_assert_pane_area_well_formed(area, buf);
}

trait PaneCell {
    fn text(&self) -> &str;
    fn is_wide(&self) -> bool;
    fn is_wide_continuation(&self) -> bool;
    fn fg(&self) -> TermColor;
    fn bg(&self) -> TermColor;
    fn bold(&self) -> bool;
    fn italic(&self) -> bool;
    fn underline(&self) -> bool;
    fn inverse(&self) -> bool;
    fn dim(&self) -> bool;
    fn strikethrough(&self) -> bool;
    fn slow_blink(&self) -> bool;
    fn rapid_blink(&self) -> bool;
    fn conceal(&self) -> bool;
}

impl PaneCell for SnapCell {
    fn text(&self) -> &str {
        &self.text
    }

    fn is_wide(&self) -> bool {
        self.is_wide
    }

    fn is_wide_continuation(&self) -> bool {
        self.is_wide_continuation
    }

    fn fg(&self) -> TermColor {
        self.fg
    }

    fn bg(&self) -> TermColor {
        self.bg
    }

    fn bold(&self) -> bool {
        self.bold
    }

    fn italic(&self) -> bool {
        self.italic
    }

    fn underline(&self) -> bool {
        self.underline
    }

    fn inverse(&self) -> bool {
        self.inverse
    }

    fn dim(&self) -> bool {
        self.dim
    }

    fn strikethrough(&self) -> bool {
        self.strikethrough
    }

    fn slow_blink(&self) -> bool {
        self.slow_blink
    }

    fn rapid_blink(&self) -> bool {
        self.rapid_blink
    }

    fn conceal(&self) -> bool {
        self.conceal
    }
}

impl PaneCell for TermCell {
    fn text(&self) -> &str {
        self.contents()
    }

    fn is_wide(&self) -> bool {
        self.is_wide
    }

    fn is_wide_continuation(&self) -> bool {
        self.is_wide_continuation
    }

    fn fg(&self) -> TermColor {
        self.fgcolor()
    }

    fn bg(&self) -> TermColor {
        self.bgcolor()
    }

    fn bold(&self) -> bool {
        self.bold()
    }

    fn italic(&self) -> bool {
        self.italic()
    }

    fn underline(&self) -> bool {
        self.underline()
    }

    fn inverse(&self) -> bool {
        self.inverse()
    }

    fn dim(&self) -> bool {
        self.dim()
    }

    fn strikethrough(&self) -> bool {
        self.strikethrough()
    }

    fn slow_blink(&self) -> bool {
        self.slow_blink()
    }

    fn rapid_blink(&self) -> bool {
        self.rapid_blink()
    }

    fn conceal(&self) -> bool {
        self.conceal()
    }
}

fn render_cell(buf_cell: &mut ratatui::buffer::Cell, cell: &impl PaneCell) {
    if cell.is_wide_continuation() {
        buf_cell.reset();
        return;
    }

    buf_cell.set_diff_option(CellDiffOption::None);
    if cell.text().is_empty() {
        buf_cell.set_char(' ');
    } else {
        buf_cell.set_symbol(cell.text());
    }
    if cell.is_wide() {
        let width = NonZeroU16::new(2).expect("wide model cells have non-zero width");
        buf_cell.set_diff_option(CellDiffOption::ForcedWidth(width));
    }

    buf_cell.set_fg(term_color(cell.fg()));
    buf_cell.set_bg(term_color(cell.bg()));

    let mut modifier = Modifier::empty();
    if cell.bold() {
        modifier |= Modifier::BOLD;
    }
    if cell.italic() {
        modifier |= Modifier::ITALIC;
    }
    if cell.underline() {
        modifier |= Modifier::UNDERLINED;
    }
    if cell.inverse() {
        modifier |= Modifier::REVERSED;
    }
    if cell.dim() {
        modifier |= Modifier::DIM;
    }
    if cell.strikethrough() {
        modifier |= Modifier::CROSSED_OUT;
    }
    if cell.slow_blink() {
        modifier |= Modifier::SLOW_BLINK;
    }
    if cell.rapid_blink() {
        modifier |= Modifier::RAPID_BLINK;
    }
    if cell.conceal() {
        modifier |= Modifier::HIDDEN;
    }
    buf_cell.modifier = modifier;
}

fn debug_assert_pane_area_well_formed(area: Rect, buf: &Buffer) {
    if !cfg!(debug_assertions) {
        return;
    }
    for row in 0..area.height {
        for col in 0..area.width {
            let cell = &buf[(area.x + col, area.y + row)];
            match cell.diff_option {
                CellDiffOption::Skip => {
                    debug_assert!(
                        false,
                        "pane body must not use CellDiffOption::Skip at ({col},{row})"
                    );
                }
                CellDiffOption::ForcedWidth(width) => {
                    let width = width.get();
                    debug_assert!(
                        width > 1,
                        "pane body must not force width 1 at ({col},{row})"
                    );
                    debug_assert!(
                        col + width <= area.width,
                        "forced-width cell at ({col},{row}) exceeds pane area width {}",
                        area.width
                    );
                    for tail in 1..width {
                        let tail_cell = &buf[(area.x + col + tail, area.y + row)];
                        debug_assert_eq!(
                            tail_cell.diff_option,
                            CellDiffOption::None,
                            "forced-width tail at ({},{row}) must be an ordinary blank cell",
                            col + tail
                        );
                        debug_assert_eq!(
                            tail_cell.symbol(),
                            " ",
                            "forced-width tail at ({},{row}) must be blank",
                            col + tail
                        );
                    }
                }
                CellDiffOption::None | CellDiffOption::AlwaysUpdate => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
