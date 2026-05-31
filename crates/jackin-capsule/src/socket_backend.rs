//! Custom Ratatui backend that serializes frame diffs to the attach socket.
//!
//! Ratatui owns the buffer double-buffering and diff computation.
//! This backend converts the per-cell diff stream produced by
//! `Terminal::draw` into cursor-positioned SGR escape sequences and
//! accumulates them in a `Vec<u8>`. The caller flushes the buffer to
//! the attach socket via [`SocketBackend::take_output`].
//!
//! Only chrome and dialog widgets run through this backend today; pane
//! body content is still handled by the existing `PaneBodyCache` path
//! and written alongside the chrome output. That split is explicit and
//! temporary: once `tui-term` or a thin custom cell widget lands, pane
//! bodies will also go through this backend and `PaneBodyCache` can be
//! retired.

use std::io;

use ratatui::{
    backend::{Backend, ClearType},
    buffer::Cell,
    layout::{Position, Size},
    style::{Color, Modifier},
};

/// Ratatui backend that buffers output for delivery to the attach socket.
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
    }

    /// Take the accumulated output, leaving the buffer empty.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
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
            let prefix = if is_bg { b"48;5;" as &[u8] } else { b"38;5;" };
            buf.extend_from_slice(b"\x1b[");
            buf.extend_from_slice(prefix);
            push_number(buf, u32::from(idx));
            buf.push(b'm');
        }
        Color::Rgb(r, g, b) => {
            let prefix = if is_bg { b"48;2;" as &[u8] } else { b"38;2;" };
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
                cursor_col = cursor_col; // unchanged
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
        self.output.extend_from_slice(b"\x1b[2J\x1b[H");
        self.current_style = CellStyle::default();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let seq = match clear_type {
            ClearType::All => b"\x1b[2J\x1b[H" as &[u8],
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
mod tests {
    use ratatui::{
        Terminal, backend::Backend, layout::Rect, style::Style, text::Span, widgets::Paragraph,
    };

    use super::SocketBackend;

    #[test]
    fn backend_renders_text_to_output_buffer() {
        let backend = SocketBackend::new(10, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                frame.render_widget(Paragraph::new(Span::raw("hi")), area);
            })
            .unwrap();
        let output = terminal.backend_mut().take_output();
        // Each character gets its own cursor-positioning sequence; verify
        // both letters appear in the output.
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains('h') && text.contains('i'),
            "expected 'h' and 'i' in output: {text:?}"
        );
    }

    #[test]
    fn resize_updates_reported_size() {
        let mut backend = SocketBackend::new(80, 24);
        backend.resize(120, 40);
        let size = backend.size().unwrap();
        assert_eq!(size.width, 120);
        assert_eq!(size.height, 40);
    }

    #[test]
    fn take_output_drains_buffer() {
        let mut backend = SocketBackend::new(10, 1);
        let terminal = Terminal::new(backend).unwrap();
        let _ = terminal; // do not call draw
        let mut backend = SocketBackend::new(10, 1);
        // Push directly for simplicity.
        backend.output.extend_from_slice(b"hello");
        let first = backend.take_output();
        let second = backend.take_output();
        assert_eq!(first, b"hello");
        assert!(second.is_empty());
    }

    #[test]
    fn cursor_movement_uses_1_based_coords() {
        let backend = SocketBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(Span::styled("X", ratatui::style::Style::default())),
                    Rect::new(2, 3, 1, 1),
                );
            })
            .unwrap();
        let output = terminal.backend_mut().take_output();
        let text = String::from_utf8_lossy(&output);
        // Row 3 (0-based) → row 4 (1-based), col 2 → col 3
        assert!(
            text.contains("\x1b[4;3H") || text.contains("\x1b[4;1H"),
            "expected cursor at row 4: {text:?}"
        );
    }
}
