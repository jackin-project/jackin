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

/// Paint the scrollbar thumb onto the pane's right border column
/// (`outer_col + outer_cols - 1`) on top of the box's `│` characters.
/// Only thumb rows are emitted: non-thumb rows keep the box border
/// underneath so the scrollbar reads as a textured border, not as a
/// duplicate vertical line. `filled == 0` suppresses the call
/// entirely so alt-screen agents and fresh shells keep their full
/// border.
///
/// Thumb height is proportional to viewport / total; thumb position
/// represents the slice of history the operator is looking at
/// (bottom row → live tail, top row → oldest scrollback line).
/// Thumb colour is phosphor-green for focused panes, gray for the
/// rest — matches the surrounding border so focus and chrome
/// agree.
#[allow(clippy::too_many_arguments)]
pub fn draw_scrollbar(
    buf: &mut Vec<u8>,
    pane_row: u16,
    pane_col: u16,
    pane_rows: u16,
    pane_cols: u16,
    offset: usize,
    filled: usize,
    focused: bool,
) {
    // Constrain the track to the pane's interior rows so the
    // top-right `┐` and bottom-right `┘` corners stay intact. Without
    // this guard the thumb overwrote one of the corners whenever the
    // scrollback was at the live tail (or the top), producing the
    // visible "scrollbar sticks out past the pane" symptom.
    let interior_rows = pane_rows.saturating_sub(2);
    let Some(thumb) = jackin_tui::vertical_thumb(interior_rows, filled, offset) else {
        return;
    };
    if pane_cols == 0 {
        return;
    }
    let col = pane_col + pane_cols - 1;

    // Active pane uses the brand phosphor-green; inactive panes a
    // neutral gray that matches their inactive border colour.
    let thumb_color = if focused {
        "\x1b[0;38;2;0;255;65m"
    } else {
        "\x1b[0;38;2;160;160;160m"
    };

    // Thumb rows are 0-based relative to the interior; skip the top
    // border row by adding 1 to `pane_row`.
    let track_start_row = pane_row + 1;
    for r in 0..thumb.thumb_rows {
        let _ = write!(buf, "\x1b[{};{}H", track_start_row + thumb.thumb_top + r + 1, col + 1);
        buf.extend_from_slice(thumb_color.as_bytes());
        buf.extend_from_slice("█".as_bytes());
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
