/// Render a `vt100::Screen` into the host terminal at a pane rectangle.
///
/// Walks the screen cell-by-cell and emits ANSI escape sequences that
/// reproduce the pane state when written to the attached client.
/// Cursor positioning is offset by the pane's origin in the host
/// terminal, so the agent's `(0, 0)` lands at `(dest_row, dest_col)`.
use std::io::Write;

use vt100::{Color, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct Attrs {
    fg: ColorKey,
    bg: ColorKey,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ColorKey {
    #[default]
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl From<Color> for ColorKey {
    fn from(c: Color) -> Self {
        match c {
            Color::Default => Self::Default,
            Color::Idx(n) => Self::Idx(n),
            Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
        }
    }
}

/// Render the screen at `(dest_row, dest_col)` into `buf`, clipped to
/// `(rect_rows, rect_cols)`. Coordinates are 0-based. When `dim` is
/// true every emitted SGR carries the ANSI dim (`;2`) attribute, used
/// as a backdrop for the modal dialog overlay so the operator sees an
/// obvious "background is paused, focus is on the dialog" cue.
pub fn render_pane(
    screen: &Screen,
    dest_row: u16,
    dest_col: u16,
    rect_rows: u16,
    rect_cols: u16,
    dim: bool,
    buf: &mut Vec<u8>,
) {
    let (screen_rows, screen_cols) = screen.size();
    let rows_to_draw = rect_rows.min(screen_rows);
    let cols_to_draw = rect_cols.min(screen_cols);

    buf.extend_from_slice(b"\x1b[0m");
    let mut last = Attrs::default();
    let mut last_emitted = false;

    for r in 0..rows_to_draw {
        write_cursor(buf, dest_row + r, dest_col);
        for c in 0..cols_to_draw {
            let cell = screen.cell(r, c);
            let attrs = cell.map(cell_attrs).unwrap_or_default();
            if !last_emitted || attrs != last {
                emit_sgr(buf, &attrs, dim);
                last = attrs;
                last_emitted = true;
            }
            match cell {
                Some(cell) if cell.has_contents() => {
                    let contents = cell.contents();
                    buf.extend_from_slice(contents.as_bytes());
                }
                _ => {
                    buf.push(b' ');
                }
            }
        }
    }
    buf.extend_from_slice(b"\x1b[0m");
}

/// Draw a 1-column vertical scrollbar on the right edge of a pane —
/// **but only when there is actually scrollback to scroll into**.
///
/// `filled` is the number of lines currently in the primary grid's
/// scrollback buffer. When `filled == 0` (alt-screen agents like
/// Claude Code, or a fresh shell that hasn't scrolled yet), the
/// scrollbar is suppressed and the pane keeps its full width — no
/// "you can scroll" hint when there's nothing to scroll into.
///
/// Thumb height is proportional to viewport / total, and the thumb's
/// position represents which slice of the history the operator is
/// looking at: bottom row → live tail; top row → oldest line in the
/// scrollback. Track = dark phosphor-green `│`; thumb = bright
/// phosphor-green `█`.
pub fn draw_scrollbar(
    buf: &mut Vec<u8>,
    pane_row: u16,
    pane_col: u16,
    pane_rows: u16,
    pane_cols: u16,
    offset: usize,
    filled: usize,
) {
    if pane_rows == 0 || pane_cols == 0 || filled == 0 {
        return;
    }
    let col = pane_col + pane_cols - 1;
    let pane_rows_us = pane_rows as usize;
    let total = filled + pane_rows_us; // history + viewport
    // Thumb height: how big the viewport is relative to total. Floor at 1.
    let thumb_rows = ((pane_rows_us * pane_rows_us) / total)
        .max(1)
        .min(pane_rows_us);
    let unscrolled_room = pane_rows_us - thumb_rows;
    // Thumb position: offset=0 → bottom; offset=filled → top.
    // Early `filled == 0` return above guarantees the divisor is non-zero.
    let thumb_top_from_bottom = (offset * unscrolled_room).checked_div(filled).unwrap_or(0);
    let thumb_top = unscrolled_room.saturating_sub(thumb_top_from_bottom);

    for r in 0..pane_rows_us {
        let _ = write!(buf, "\x1b[{};{}H", pane_row + r as u16 + 1, col + 1);
        if r >= thumb_top && r < thumb_top + thumb_rows {
            buf.extend_from_slice(b"\x1b[0;38;2;0;255;65m");
            buf.extend_from_slice("█".as_bytes());
        } else {
            buf.extend_from_slice(b"\x1b[0;38;2;0;80;18m");
            buf.extend_from_slice("│".as_bytes());
        }
    }
    buf.extend_from_slice(b"\x1b[0m");
}

fn cell_attrs(cell: &vt100::Cell) -> Attrs {
    Attrs {
        fg: ColorKey::from(cell.fgcolor()),
        bg: ColorKey::from(cell.bgcolor()),
        bold: cell.bold(),
        dim: cell.dim(),
        italic: cell.italic(),
        underline: cell.underline(),
        inverse: cell.inverse(),
    }
}

fn write_cursor(buf: &mut Vec<u8>, row: u16, col: u16) {
    let _ = write!(buf, "\x1b[{};{}H", row + 1, col + 1);
}

fn emit_sgr(buf: &mut Vec<u8>, a: &Attrs, dialog_dim: bool) {
    buf.extend_from_slice(b"\x1b[0");
    // Cell-level dim (Amp uses this for its animated bottom-bar) and
    // dialog-backdrop dim (when a modal is open) both produce the
    // same ANSI `;2` attribute — they OR together so neither shadows
    // the other.
    if a.dim || dialog_dim {
        buf.extend_from_slice(b";2");
    }
    if a.bold {
        buf.extend_from_slice(b";1");
    }
    if a.italic {
        buf.extend_from_slice(b";3");
    }
    if a.underline {
        buf.extend_from_slice(b";4");
    }
    if a.inverse {
        buf.extend_from_slice(b";7");
    }
    match a.fg {
        ColorKey::Default => {}
        ColorKey::Idx(n) if n < 8 => {
            let _ = write!(buf, ";3{}", n);
        }
        ColorKey::Idx(n) if n < 16 => {
            let _ = write!(buf, ";9{}", n - 8);
        }
        ColorKey::Idx(n) => {
            let _ = write!(buf, ";38;5;{n}");
        }
        ColorKey::Rgb(r, g, b) => {
            let _ = write!(buf, ";38;2;{r};{g};{b}");
        }
    }
    match a.bg {
        ColorKey::Default => {}
        ColorKey::Idx(n) if n < 8 => {
            let _ = write!(buf, ";4{}", n);
        }
        ColorKey::Idx(n) if n < 16 => {
            let _ = write!(buf, ";10{}", n - 8);
        }
        ColorKey::Idx(n) => {
            let _ = write!(buf, ";48;5;{n}");
        }
        ColorKey::Rgb(r, g, b) => {
            let _ = write!(buf, ";48;2;{r};{g};{b}");
        }
    }
    buf.push(b'm');
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt100::Parser;

    #[test]
    fn alt_screen_round_trip_preserves_primary() {
        // Enter alt-screen, write content, leave alt-screen, primary should
        // be restored. Regression guard for the hand-rolled emulator that
        // ignored DEC private mode `?1049`.
        let mut parser = Parser::new(5, 20, 0);
        parser.process(b"hello\r\nworld\r\n");
        let primary_before = parser.screen().contents();

        parser.process(b"\x1b[?1049h");
        parser.process(b"\x1b[2J\x1b[Halt-screen content\r\n");
        parser.process(b"\x1b[?1049l");

        let primary_after = parser.screen().contents();
        assert_eq!(
            primary_after.trim_end(),
            primary_before.trim_end(),
            "primary screen lost across alt-screen entry/exit"
        );
    }

    #[test]
    fn render_pane_offsets_cursor_to_origin() {
        let mut parser = Parser::new(3, 10, 0);
        parser.process(b"hi");
        let mut buf = Vec::new();
        render_pane(parser.screen(), 4, 2, 3, 10, false, &mut buf);
        let s = String::from_utf8_lossy(&buf);
        // Render must start by writing to row 5 col 3 (1-based after the
        // dest_row=4, dest_col=2 offset) — not row 1 col 1 which would
        // mean the offset was dropped.
        assert!(
            s.contains("\x1b[5;3H"),
            "missing pane-origin cursor move: {s:?}"
        );
    }
}
