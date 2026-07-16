// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

use jackin_term::{Color as TermColor, UnderlineStyle};
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
    current_metadata: SgrMetadata,
    /// Hyperlinked cell rects for the current frame: the encoder emits
    /// `OSC 8` open/close brackets around exactly these cells during cell
    /// emission (§3.4 — no raw overlay writes). Consumed by the next `draw`.
    hyperlink_regions: Vec<(ratatui::layout::Rect, String)>,
    sgr_regions: Vec<(ratatui::layout::Rect, SgrMetadata)>,
    /// While armed, swallow every screen-erase escape from
    /// `clear_region(ClearType::All)`. `Terminal::resize()` resets Ratatui's
    /// diff baseline AND moves the buffers/`last_known_area` (whereas
    /// `Terminal::clear()` only resets the baseline), routing through
    /// `clear_region(All)` once — or twice on a width shrink. The resize
    /// bookkeeping needs that baseline/geometry reset without blanking the
    /// client screen or leaking a stray erase that would ride a later frame.
    /// The single visible wipe is the compositor's `Resize` redraw, emitted
    /// after suppression is lifted.
    suppress_clear_escapes: bool,
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
            current_metadata: SgrMetadata::default(),
            hyperlink_regions: Vec::new(),
            sgr_regions: Vec::new(),
            suppress_clear_escapes: false,
        }
    }

    /// Set the frame's hyperlinked cell rects; consumed by the next `draw`.
    pub fn set_hyperlink_regions(&mut self, regions: Vec<(ratatui::layout::Rect, String)>) {
        self.hyperlink_regions = regions;
    }

    pub(crate) fn set_sgr_regions(&mut self, regions: Vec<(ratatui::layout::Rect, SgrMetadata)>) {
        self.sgr_regions = regions;
    }

    /// Arm sustained erase suppression: every `clear_region(ClearType::All)`
    /// resets style/baseline but emits no bytes until `end_clear_suppression`.
    /// Wrap a `Terminal::resize()` (which clears once, or twice on a width
    /// shrink) so the geometry/baseline reset stays byte-silent. See the field
    /// doc for why the visible wipe must come from the compositor instead.
    pub fn begin_clear_suppression(&mut self) {
        self.suppress_clear_escapes = true;
    }

    /// Lift sustained erase suppression. The next `clear_region(All)` — the
    /// compositor's `Resize` wipe — erases the client screen normally.
    pub fn end_clear_suppression(&mut self) {
        self.suppress_clear_escapes = false;
    }

    /// Update the terminal size. Called when the daemon receives a resize event.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
        self.current_style = CellStyle::default();
        self.current_metadata = SgrMetadata::default();
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

    /// Write the SGR sequence for `style` if it differs from the last one emitted.
    fn apply_style(&mut self, style: CellStyle, metadata: SgrMetadata) {
        if style == self.current_style && metadata == self.current_metadata {
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
        if style.modifiers.contains(Modifier::SLOW_BLINK) {
            self.output.extend_from_slice(b"\x1b[5m");
        }
        if style.modifiers.contains(Modifier::RAPID_BLINK) {
            self.output.extend_from_slice(b"\x1b[6m");
        }
        if style.modifiers.contains(Modifier::REVERSED) {
            self.output.extend_from_slice(b"\x1b[7m");
        }
        if style.modifiers.contains(Modifier::HIDDEN) {
            self.output.extend_from_slice(b"\x1b[8m");
        }
        if style.modifiers.contains(Modifier::CROSSED_OUT) {
            self.output.extend_from_slice(b"\x1b[9m");
        }

        // Foreground
        write_color_sgr(&mut self.output, style.fg, false);

        // Background
        write_color_sgr(&mut self.output, style.bg, true);
        write_sgr_metadata(&mut self.output, metadata);

        self.current_style = style;
        self.current_metadata = metadata;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SgrMetadata {
    pub(crate) underline_style: UnderlineStyle,
    pub(crate) underline_color: TermColor,
    pub(crate) overline: bool,
}

pub(crate) const fn term_color(color: TermColor) -> Color {
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
            buf.extend_from_slice(if is_bg { b"\x1b[48;" } else { b"\x1b[38;" });
            push_indexed_color_tail(buf, idx);
        }
        Color::Rgb(r, g, b) => {
            buf.extend_from_slice(if is_bg { b"\x1b[48;" } else { b"\x1b[38;" });
            push_rgb_color_tail(buf, r, g, b);
        }
    }
}

/// `5;<idx>m` tail of an indexed-color SGR, after the `38;`/`48;`/`58;` opener.
fn push_indexed_color_tail(buf: &mut Vec<u8>, idx: u8) {
    buf.extend_from_slice(b"5;");
    push_number(buf, u32::from(idx));
    buf.push(b'm');
}

/// `2;<r>;<g>;<b>m` tail of a truecolor SGR, after the `38;`/`48;`/`58;` opener.
fn push_rgb_color_tail(buf: &mut Vec<u8>, r: u8, g: u8, b: u8) {
    buf.extend_from_slice(b"2;");
    push_number(buf, u32::from(r));
    buf.push(b';');
    push_number(buf, u32::from(g));
    buf.push(b';');
    push_number(buf, u32::from(b));
    buf.push(b'm');
}

fn write_sgr_metadata(buf: &mut Vec<u8>, metadata: SgrMetadata) {
    match metadata.underline_style {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => buf.extend_from_slice(b"\x1b[4m"),
        UnderlineStyle::Double => buf.extend_from_slice(b"\x1b[4:2m"),
        UnderlineStyle::Curly => buf.extend_from_slice(b"\x1b[4:3m"),
        UnderlineStyle::Dotted => buf.extend_from_slice(b"\x1b[4:4m"),
        UnderlineStyle::Dashed => buf.extend_from_slice(b"\x1b[4:5m"),
    }
    if metadata.underline_color != TermColor::Default {
        buf.extend_from_slice(b"\x1b[58;");
        match metadata.underline_color {
            TermColor::Default => {}
            TermColor::Idx(idx) => push_indexed_color_tail(buf, idx),
            TermColor::Rgb(r, g, b) => push_rgb_color_tail(buf, r, g, b),
        }
    }
    if metadata.overline {
        buf.extend_from_slice(b"\x1b[53m");
    }
}

fn push_sgr(buf: &mut Vec<u8>, code: u8) {
    buf.extend_from_slice(b"\x1b[");
    push_number(buf, u32::from(code));
    buf.push(b'm');
}

fn push_number(buf: &mut Vec<u8>, n: u32) {
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut remaining = n;
    loop {
        digits[len] = b'0' + (remaining % 10) as u8;
        len += 1;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    for digit in digits[..len].iter().rev() {
        buf.push(*digit);
    }
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
        let regions = std::mem::take(&mut self.hyperlink_regions);
        let sgr_regions = std::mem::take(&mut self.sgr_regions);
        let mut open_link: Option<usize> = None;

        for (x, y, cell) in content {
            let row = y + 1; // 1-based terminal row
            let col = x + 1; // 1-based terminal column

            // Frame hyperlink layer: bracket linked cells with OSC 8 during
            // emission. Transitions only — adjacent cells in the same region
            // share one open/close pair.
            let desired_link = regions
                .iter()
                .position(|(rect, _)| rect.contains(Position { x, y }));
            if desired_link != open_link {
                if open_link.is_some() {
                    crate::tui::ansi::emit_osc8_close(&mut self.output);
                }
                if let Some(idx) = desired_link {
                    crate::tui::ansi::emit_osc8_open(&mut self.output, &regions[idx].1);
                }
                open_link = desired_link;
            }
            let metadata = sgr_regions
                .iter()
                .find_map(|(rect, metadata)| rect.contains(Position { x, y }).then_some(*metadata))
                .unwrap_or_default();

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

            self.apply_style(CellStyle::from_cell(cell), metadata);
            let sym = cell.symbol();
            self.output.extend_from_slice(sym.as_bytes());

            // The skip-the-CUP optimization applies only across runs of
            // single printable ASCII cells (0x20–0x7E): their width-1 advance is
            // unambiguous on every terminal. After any other glyph the next
            // cell gets an explicit move — the outer terminal's
            // ambiguous-width configuration may disagree with unicode-width
            // about how far the cursor moved (D8).
            let is_single_ascii_printable =
                sym.len() == 1 && matches!(sym.as_bytes()[0], 0x20..=0x7e);
            if is_single_ascii_printable {
                cursor_col = Some(col + 1);
            } else {
                cursor_col = None;
            }
        }
        if open_link.is_some() {
            crate::tui::ansi::emit_osc8_close(&mut self.output);
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
        // Ratatui's Terminal::clear() does NOT call this method — with a
        // Fullscreen viewport it calls clear_region(ClearType::All) and resets
        // its back buffer itself (verified against the pinned ratatui-core
        // 0.1.0 terminal.rs). Backend::clear is unreachable from the
        // compositor today; reset style tracking and emit nothing so any
        // future caller still cannot blank the client screen.
        self.current_style = CellStyle::default();
        self.current_metadata = SgrMetadata::default();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let seq: &[u8] = match clear_type {
            ClearType::All => {
                self.current_style = CellStyle::default();
                self.current_metadata = SgrMetadata::default();
                if self.suppress_clear_escapes {
                    // Sustained baseline/geometry-reset mode: Terminal::resize()
                    // wants the diff baseline reset (possibly twice on a width
                    // shrink) without any visible wipe.
                    return Ok(());
                }
                // ED uses the terminal's active SGR background on BCE-capable
                // terminals. Reset before the clear so a previous green status
                // cell cannot turn the full screen into green blank space.
                b"\x1b[0m\x1b[2J\x1b[H"
            }
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
