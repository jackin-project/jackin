//! Render a `vt100::Screen` into the host terminal at a pane rectangle.
//! Cursor positioning is offset by the pane's origin so the agent's
//! `(0, 0)` lands at `(dest_row, dest_col)`.

use std::io::Write;

use vt100::{Color, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
struct Attrs {
    fg: ColorKey,
    bg: ColorKey,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct CellSnapshot {
    contents: String,
    attrs: Attrs,
    width: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RowSnapshot {
    cells: Vec<CellSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneBodyRenderMode {
    Full,
    Partial,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneBodyRenderStats {
    pub mode: PaneBodyRenderMode,
    pub rows_emitted: usize,
    pub changed_rows: Vec<u16>,
}

/// Cached visible pane body used to emit Zellij-style changed rows
/// instead of repainting every pane body on every PTY burst.
#[derive(Debug, Default)]
pub struct PaneBodyCache {
    rows: u16,
    cols: u16,
    dim: bool,
    valid: bool,
    snapshot: Vec<RowSnapshot>,
}

impl PaneBodyCache {
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.snapshot.clear();
    }

    pub fn is_valid_for(&self, rect_rows: u16, rect_cols: u16, dim: bool) -> bool {
        self.valid && self.rows == rect_rows && self.cols == rect_cols && self.dim == dim
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_full(
        &mut self,
        screen: &Screen,
        dest_row: u16,
        dest_col: u16,
        rect_rows: u16,
        rect_cols: u16,
        dim: bool,
        buf: &mut Vec<u8>,
    ) -> PaneBodyRenderStats {
        let snapshot = pane_snapshot(screen, rect_rows, rect_cols);
        let changed_rows: Vec<u16> = (0..snapshot.len() as u16).collect();
        render_snapshot_rows(&snapshot, &changed_rows, dest_row, dest_col, dim, buf);
        self.rows = rect_rows;
        self.cols = rect_cols;
        self.dim = dim;
        self.valid = true;
        self.snapshot = snapshot;
        PaneBodyRenderStats {
            mode: PaneBodyRenderMode::Full,
            rows_emitted: changed_rows.len(),
            changed_rows,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_partial(
        &mut self,
        screen: &Screen,
        dest_row: u16,
        dest_col: u16,
        rect_rows: u16,
        rect_cols: u16,
        dim: bool,
        buf: &mut Vec<u8>,
    ) -> PaneBodyRenderStats {
        if !self.valid || self.rows != rect_rows || self.cols != rect_cols || self.dim != dim {
            return self.render_full(screen, dest_row, dest_col, rect_rows, rect_cols, dim, buf);
        }

        let next = pane_snapshot(screen, rect_rows, rect_cols);
        if next.len() != self.snapshot.len() {
            return self.render_full(screen, dest_row, dest_col, rect_rows, rect_cols, dim, buf);
        }

        let changed_rows: Vec<u16> = next
            .iter()
            .zip(&self.snapshot)
            .enumerate()
            .filter_map(|(idx, (new_row, old_row))| (new_row != old_row).then_some(idx as u16))
            .collect();
        render_snapshot_rows(&next, &changed_rows, dest_row, dest_col, dim, buf);
        self.snapshot = next;

        PaneBodyRenderStats {
            mode: PaneBodyRenderMode::Partial,
            rows_emitted: changed_rows.len(),
            changed_rows,
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
    let snapshot = pane_snapshot(screen, rect_rows, rect_cols);
    let rows: Vec<u16> = (0..snapshot.len() as u16).collect();
    render_snapshot_rows(&snapshot, &rows, dest_row, dest_col, dim, buf);
}

/// Paint the scrollbar thumb onto the pane's right border column
/// (`outer_col + outer_cols - 1`) on top of the box's `│` characters.
/// Only thumb rows are emitted: non-thumb rows keep the box border
/// underneath so the scrollbar reads as a textured border, not as a
/// duplicate vertical line. `filled == 0` suppresses the call
/// entirely so alternate-screen TUIs and fresh primary-screen panes
/// keep their full border.
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
    // Bail on zero-width / zero-height panes before doing any
    // arithmetic. `pane_col + pane_cols - 1` would underflow u16 when
    // pane_cols == 0; saturating_add+saturating_sub keep arithmetic
    // total even for runaway resize ticks where pane geometry briefly
    // hits zero.
    if pane_cols == 0 || pane_rows < 2 {
        return;
    }
    // Constrain the track to the pane's interior rows so the
    // top-right `┐` and bottom-right `┘` corners stay intact. Without
    // this guard the thumb overwrote one of the corners whenever the
    // scrollback was at the live tail (or the top), producing the
    // visible "scrollbar sticks out past the pane" symptom.
    let interior_rows = pane_rows.saturating_sub(2);
    let Some(thumb) = jackin_tui::vertical_thumb(interior_rows, filled, offset) else {
        return;
    };
    let col = pane_col.saturating_add(pane_cols).saturating_sub(1);

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
        let _ = write!(
            buf,
            "\x1b[{};{}H",
            track_start_row + thumb.thumb_top + r + 1,
            col + 1
        );
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

fn pane_snapshot(screen: &Screen, rect_rows: u16, rect_cols: u16) -> Vec<RowSnapshot> {
    let (screen_rows, screen_cols) = screen.size();
    let rows_to_draw = rect_rows.min(screen_rows);
    let cols_to_draw = rect_cols.min(screen_cols);
    (0..rows_to_draw)
        .map(|row| snapshot_row(screen, row, cols_to_draw))
        .collect()
}

fn snapshot_row(screen: &Screen, row: u16, cols_to_draw: u16) -> RowSnapshot {
    let mut cells = Vec::with_capacity(cols_to_draw as usize);
    let mut col = 0;
    while col < cols_to_draw {
        let cell = screen.cell(row, col);
        if cell.is_some_and(|cell| cell.is_wide_continuation()) {
            col += 1;
            continue;
        }

        let width = cell
            .filter(|cell| cell.is_wide())
            .map_or(1, |_| 2)
            .min(cols_to_draw - col);
        let attrs = cell.map(cell_attrs).unwrap_or_default();
        let contents = match cell {
            Some(cell) if cell.has_contents() => cell.contents().to_string(),
            _ => " ".repeat(width as usize),
        };
        cells.push(CellSnapshot {
            contents,
            attrs,
            width,
        });
        col += width;
    }
    RowSnapshot { cells }
}

fn render_snapshot_rows(
    snapshot: &[RowSnapshot],
    rows: &[u16],
    dest_row: u16,
    dest_col: u16,
    dim: bool,
    buf: &mut Vec<u8>,
) {
    if rows.is_empty() {
        return;
    }
    for &row_idx in rows {
        let Some(row) = snapshot.get(row_idx as usize) else {
            continue;
        };
        write_cursor(buf, dest_row + row_idx, dest_col);
        buf.extend_from_slice(b"\x1b[0m");
        let mut last = Attrs::default();
        let mut last_emitted = false;
        for cell in &row.cells {
            if !last_emitted || cell.attrs != last {
                emit_sgr(buf, &cell.attrs, dim);
                last = cell.attrs;
                last_emitted = true;
            }
            buf.extend_from_slice(cell.contents.as_bytes());
        }
    }
    buf.extend_from_slice(b"\x1b[0m");
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

    #[test]
    fn pane_cache_first_render_is_full_and_tracks_every_visible_row() {
        let mut parser = Parser::new(3, 8, 0);
        parser.process(b"one\r\ntwo");
        let mut cache = PaneBodyCache::default();
        let mut buf = Vec::new();

        let stats = cache.render_partial(parser.screen(), 10, 20, 3, 8, false, &mut buf);

        assert_eq!(stats.mode, PaneBodyRenderMode::Full);
        assert_eq!(stats.changed_rows, vec![0, 1, 2]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("\x1b[11;21H"));
        assert!(s.contains("\x1b[12;21H"));
        assert!(s.contains("\x1b[13;21H"));
    }

    #[test]
    fn pane_cache_emits_only_changed_rows_after_warmup() {
        let mut parser = Parser::new(3, 12, 0);
        parser.process(b"alpha\r\nbeta\r\ngamma");
        let mut cache = PaneBodyCache::default();
        let mut buf = Vec::new();
        cache.render_full(parser.screen(), 0, 0, 3, 12, false, &mut buf);
        buf.clear();

        parser.process(b"\x1b[2;1Hbravo");
        let stats = cache.render_partial(parser.screen(), 0, 0, 3, 12, false, &mut buf);

        assert_eq!(stats.mode, PaneBodyRenderMode::Partial);
        assert_eq!(stats.changed_rows, vec![1]);
        let s = String::from_utf8_lossy(&buf);
        assert!(!s.contains("\x1b[1;1H"));
        assert!(s.contains("\x1b[2;1H"));
        assert!(!s.contains("\x1b[3;1H"));
        assert!(s.contains("bravo"));
    }

    #[test]
    fn pane_cache_partial_rows_reset_styles_independently() {
        let mut parser = Parser::new(2, 16, 0);
        parser.process(b"\x1b[31mred\x1b[0m\r\nplain");
        let mut cache = PaneBodyCache::default();
        let mut buf = Vec::new();
        cache.render_full(parser.screen(), 0, 0, 2, 16, false, &mut buf);
        buf.clear();

        parser.process(b"\x1b[1;1H\x1b[32mgreen\x1b[0m");
        let stats = cache.render_partial(parser.screen(), 0, 0, 2, 16, false, &mut buf);

        assert_eq!(stats.changed_rows, vec![0]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("\x1b[1;1H\x1b[0m"));
        assert!(s.contains("\x1b[0;32mgreen"));
        assert!(s.ends_with("\x1b[0m"));
    }

    #[test]
    fn pane_cache_handles_wide_characters_without_dirtying_continuations() {
        let mut parser = Parser::new(2, 10, 0);
        parser.process("表x\r\nsame".as_bytes());
        let mut cache = PaneBodyCache::default();
        let mut buf = Vec::new();
        cache.render_full(parser.screen(), 0, 0, 2, 10, false, &mut buf);
        buf.clear();

        parser.process("\x1b[1;3Hy".as_bytes());
        let stats = cache.render_partial(parser.screen(), 0, 0, 2, 10, false, &mut buf);

        assert_eq!(stats.changed_rows, vec![0]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("表y"));
        assert!(!s.contains("表 y"));
    }

    #[test]
    fn pane_cache_partial_ansi_serialization_covers_rgb_and_background() {
        let mut parser = Parser::new(1, 8, 0);
        parser.process(b"plain");
        let mut cache = PaneBodyCache::default();
        let mut buf = Vec::new();
        cache.render_full(parser.screen(), 0, 0, 1, 8, false, &mut buf);
        buf.clear();

        parser.process(b"\x1b[1;1H\x1b[38;2;1;2;3;48;5;4;1mX");
        let stats = cache.render_partial(parser.screen(), 0, 0, 1, 8, false, &mut buf);

        assert_eq!(stats.changed_rows, vec![0]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("\x1b[0;1;38;2;1;2;3;44mX"));
    }
}
