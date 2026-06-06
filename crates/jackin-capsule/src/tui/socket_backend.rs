//! Custom Ratatui backend that serializes frame diffs to the attach socket.
//!
//! Ratatui owns the buffer double-buffering and diff computation.
//! This backend converts the per-cell diff stream produced by
//! `Terminal::draw` into cursor-positioned SGR escape sequences and
//! accumulates them in a `Vec<u8>`. The caller flushes the buffer to
//! the attach socket via [`SocketBackend::take_output`].
//!
//! Chrome, dialogs, and pane bodies all render through this backend today.
//! Ratatui's previous buffer is the only pane-body diff state.

use std::io;

use jackin_term::{Cell as TermCell, Color as TermColor, GridPatch};
use ratatui::{
    backend::{Backend, ClearType},
    buffer::Cell,
    layout::{Position, Size},
    style::{Color, Modifier},
};

/// Ratatui backend that buffers output for delivery to the attach socket.
#[derive(Debug)]
pub struct SocketBackend {
    /// Terminal size reported to Ratatui. Updated via `resize`.
    size: (u16, u16),
    /// Accumulated ANSI escape sequences for the current frame.
    output: Vec<u8>,
    /// Tracks the style applied at the cursor position so adjacent cells
    /// with the same style don't re-emit SGR sequences.
    current_style: CellStyle,
}

/// Compact style summary for change-tracking only — enough detail to
/// decide whether a new SGR sequence is needed between cells.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CellStyle {
    fg: Color,
    bg: Color,
    modifiers: Modifier,
}

impl CellStyle {
    fn from_cell(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            modifiers: cell.modifier,
        }
    }
}

impl SocketBackend {
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            size: (cols, rows),
            output: Vec::with_capacity(65536),
            current_style: CellStyle::default(),
        }
    }

    /// Update the terminal size. Called when the daemon receives a resize event.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
        self.current_style = CellStyle::default();
    }

    /// Take the accumulated output, leaving the buffer empty.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Drain the accumulated output into an existing buffer, preserving this
    /// backend's allocation for the next frame.
    pub fn drain_output_into(&mut self, target: &mut Vec<u8>) {
        target.extend_from_slice(&self.output);
        self.output.clear();
    }

    /// Draw a terminal-grid dirty patch directly into the backend output.
    ///
    /// This bypasses Ratatui's `Terminal::draw` diff allocation for live
    /// pane-body-only updates. Full frames, chrome, dialogs, and selection
    /// still go through the Ratatui buffer path.
    pub fn draw_grid_patch(&mut self, area: ratatui::layout::Rect, patch: &GridPatch<'_>) {
        let mut cursor_row: Option<u16> = None;
        let mut cursor_col: Option<u16> = None;
        let width = area.width.min(patch.cols);
        for (row, cells) in patch.changed_rows() {
            if row >= area.height {
                continue;
            }
            for col in 0..width {
                let Some(cell) = cells.get(col as usize) else {
                    self.draw_symbol_at(
                        area.x + col,
                        area.y + row,
                        " ",
                        CellStyle::default(),
                        &mut cursor_row,
                        &mut cursor_col,
                    );
                    continue;
                };
                if cell.is_wide_continuation {
                    continue;
                }
                let (symbol, style) = {
                    let symbol = if cell.contents().is_empty() {
                        " "
                    } else {
                        cell.contents()
                    };
                    (symbol, CellStyle::from_term_cell(cell))
                };
                self.draw_symbol_at(
                    area.x + col,
                    area.y + row,
                    symbol,
                    style,
                    &mut cursor_row,
                    &mut cursor_col,
                );
            }
        }
    }

    /// Write the SGR sequence for `style` if it differs from the last one emitted.
    fn apply_style(&mut self, style: CellStyle) {
        if style == self.current_style {
            return;
        }
        // Full reset then re-apply rather than tracking incremental changes.
        // Slightly verbose but simpler and correct across cell sequences.
        self.output.extend_from_slice(b"\x1b[0m");

        // Modifiers
        if style.modifiers.contains(Modifier::BOLD) {
            self.output.extend_from_slice(b"\x1b[1m");
        }
        if style.modifiers.contains(Modifier::DIM) {
            self.output.extend_from_slice(b"\x1b[2m");
        }
        if style.modifiers.contains(Modifier::ITALIC) {
            self.output.extend_from_slice(b"\x1b[3m");
        }
        if style.modifiers.contains(Modifier::UNDERLINED) {
            self.output.extend_from_slice(b"\x1b[4m");
        }
        if style.modifiers.contains(Modifier::REVERSED) {
            self.output.extend_from_slice(b"\x1b[7m");
        }

        // Foreground
        write_color_sgr(&mut self.output, style.fg, false);

        // Background
        write_color_sgr(&mut self.output, style.bg, true);

        self.current_style = style;
    }

    fn draw_symbol_at(
        &mut self,
        x: u16,
        y: u16,
        symbol: &str,
        style: CellStyle,
        cursor_row: &mut Option<u16>,
        cursor_col: &mut Option<u16>,
    ) {
        let row = y + 1;
        let col = x + 1;
        if *cursor_row != Some(row) || *cursor_col != Some(col) {
            self.output.extend_from_slice(b"\x1b[");
            push_number(&mut self.output, u32::from(row));
            self.output.push(b';');
            push_number(&mut self.output, u32::from(col));
            self.output.push(b'H');
            *cursor_row = Some(row);
        }
        self.apply_style(style);
        self.output.extend_from_slice(symbol.as_bytes());

        use unicode_width::UnicodeWidthStr;
        let width = symbol.width() as u16;
        if width != 0 {
            *cursor_col = Some(col + width);
        }
    }
}

impl CellStyle {
    fn from_term_cell(cell: &TermCell) -> Self {
        let mut modifiers = Modifier::empty();
        if cell.bold() {
            modifiers |= Modifier::BOLD;
        }
        if cell.dim() {
            modifiers |= Modifier::DIM;
        }
        if cell.italic() {
            modifiers |= Modifier::ITALIC;
        }
        if cell.underline() {
            modifiers |= Modifier::UNDERLINED;
        }
        if cell.inverse() {
            modifiers |= Modifier::REVERSED;
        }
        Self {
            fg: term_color(cell.fgcolor()),
            bg: term_color(cell.bgcolor()),
            modifiers,
        }
    }
}

const fn term_color(color: TermColor) -> Color {
    match color {
        TermColor::Default => Color::Reset,
        TermColor::Idx(idx) => Color::Indexed(idx),
        TermColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn write_color_sgr(buf: &mut Vec<u8>, color: Color, is_bg: bool) {
    let base = if is_bg { 40u8 } else { 30u8 };
    match color {
        Color::Reset => {
            // Let the reset at the top of apply_style handle it.
        }
        Color::Black => push_sgr(buf, base),
        Color::Red => push_sgr(buf, base + 1),
        Color::Green => push_sgr(buf, base + 2),
        Color::Yellow => push_sgr(buf, base + 3),
        Color::Blue => push_sgr(buf, base + 4),
        Color::Magenta => push_sgr(buf, base + 5),
        Color::Cyan => push_sgr(buf, base + 6),
        Color::White => push_sgr(buf, base + 7),
        Color::DarkGray => push_sgr(buf, base + 60),
        Color::LightRed => push_sgr(buf, base + 61),
        Color::LightGreen => push_sgr(buf, base + 62),
        Color::LightYellow => push_sgr(buf, base + 63),
        Color::LightBlue => push_sgr(buf, base + 64),
        Color::LightMagenta => push_sgr(buf, base + 65),
        Color::LightCyan => push_sgr(buf, base + 66),
        Color::Gray => push_sgr(buf, base + 7),
        Color::Indexed(idx) => {
            let prefix: &[u8] = if is_bg { b"48;5;" } else { b"38;5;" };
            buf.extend_from_slice(b"\x1b[");
            buf.extend_from_slice(prefix);
            push_number(buf, u32::from(idx));
            buf.push(b'm');
        }
        Color::Rgb(r, g, b) => {
            let prefix: &[u8] = if is_bg { b"48;2;" } else { b"38;2;" };
            buf.extend_from_slice(b"\x1b[");
            buf.extend_from_slice(prefix);
            push_number(buf, u32::from(r));
            buf.push(b';');
            push_number(buf, u32::from(g));
            buf.push(b';');
            push_number(buf, u32::from(b));
            buf.push(b'm');
        }
    }
}

fn push_sgr(buf: &mut Vec<u8>, code: u8) {
    buf.extend_from_slice(b"\x1b[");
    push_number(buf, u32::from(code));
    buf.push(b'm');
}

fn push_number(buf: &mut Vec<u8>, n: u32) {
    // Avoid allocation: write digits directly.
    if n >= 100 {
        buf.push(b'0' + (n / 100) as u8);
    }
    if n >= 10 {
        buf.push(b'0' + ((n / 10) % 10) as u8);
    }
    buf.push(b'0' + (n % 10) as u8);
}

impl Backend for SocketBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        // Track the terminal cursor position so we can skip the `\x1b[row;colH`
        // move for cells that immediately follow the previously written cell on
        // the same row. This eliminates a large fraction of cursor-position
        // escapes when Ratatui's diff sends runs of consecutive changed cells
        // (e.g. the entire dialog backdrop on first open).
        let mut cursor_row: Option<u16> = None;
        let mut cursor_col: Option<u16> = None;

        for (x, y, cell) in content {
            let row = y + 1; // 1-based terminal row
            let col = x + 1; // 1-based terminal column

            // Emit cursor position only when we are not already there.
            // After writing a single-column cell at (col, row) the terminal
            // advances to (col + 1, row), so we can skip the next move when
            // the next cell is exactly one column to the right on the same row.
            let already_positioned = cursor_row == Some(row) && cursor_col == Some(col);
            if !already_positioned {
                self.output.extend_from_slice(b"\x1b[");
                push_number(&mut self.output, u32::from(row));
                self.output.push(b';');
                push_number(&mut self.output, u32::from(col));
                self.output.push(b'H');
                cursor_row = Some(row);
            }

            self.apply_style(CellStyle::from_cell(cell));
            let sym = cell.symbol();
            self.output.extend_from_slice(sym.as_bytes());

            // Advance the tracked column by the number of terminal columns the
            // symbol occupies.  ASCII is always 1; wide characters (CJK etc.)
            // are 2.  Combining characters (width 0) leave the cursor in place.
            use unicode_width::UnicodeWidthStr;
            let width = sym.width() as u16;
            if width == 0 {
                // Combining character: cursor didn't move.
            } else {
                cursor_col = Some(col + width);
            }
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.output.extend_from_slice(b"\x1b[?25l");
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.output.extend_from_slice(b"\x1b[?25h");
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(Position { x: 0, y: 0 })
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        let pos = position.into();
        let row = pos.y + 1;
        let col = pos.x + 1;
        self.output.extend_from_slice(b"\x1b[");
        push_number(&mut self.output, u32::from(row));
        self.output.push(b';');
        push_number(&mut self.output, u32::from(col));
        self.output.push(b'H');
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        // Do NOT emit \x1b[2J here. This method is called by Ratatui's
        // Terminal::clear() to reset the double-buffer so the next draw()
        // produces a full diff (all cells "changed"). In the SocketBackend
        // context we achieve the same effect by resetting style tracking and
        // letting the full diff send every cell — no need for a screen-erase
        // escape that would cause a momentary blank flash on the client.
        self.current_style = CellStyle::default();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let seq: &[u8] = match clear_type {
            ClearType::All => b"\x1b[2J\x1b[H",
            ClearType::AfterCursor => b"\x1b[0J",
            ClearType::BeforeCursor => b"\x1b[1J",
            ClearType::CurrentLine => b"\x1b[2K",
            ClearType::UntilNewLine => b"\x1b[0K",
        };
        self.output.extend_from_slice(seq);
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(Size {
            width: self.size.0,
            height: self.size.1,
        })
    }

    fn window_size(&mut self) -> Result<ratatui::backend::WindowSize, Self::Error> {
        Ok(ratatui::backend::WindowSize {
            columns_rows: Size {
                width: self.size.0,
                height: self.size.1,
            },
            pixels: Size {
                width: 0,
                height: 0,
            },
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // The caller retrieves output via take_output(); nothing to do here.
        Ok(())
    }
}

#[cfg(test)]
mod tests;
