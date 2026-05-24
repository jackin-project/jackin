//! Render a `vt100::Screen` into the host terminal at a pane rectangle.
//! Cursor positioning is offset by the pane's origin so the agent's
//! `(0, 0)` lands at `(dest_row, dest_col)`.

use std::io::Write;

use unicode_width::UnicodeWidthChar;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneBodyDim {
    #[default]
    Normal,
    Inactive,
    Backdrop,
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
    dim: PaneBodyDim,
    valid: bool,
    snapshot: Vec<RowSnapshot>,
}

impl PaneBodyCache {
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.snapshot.clear();
    }

    pub fn is_valid_for(&self, rect_rows: u16, rect_cols: u16, dim: PaneBodyDim) -> bool {
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
        dim: PaneBodyDim,
        buf: &mut Vec<u8>,
    ) -> PaneBodyRenderStats {
        let snapshot = pane_snapshot(screen, rect_rows, rect_cols);
        self.render_full_from_snapshot(snapshot, dest_row, dest_col, rect_rows, rect_cols, dim, buf)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_full_with_scrollback_prefix(
        &mut self,
        screen: &Screen,
        scrollback_prefix: &[String],
        dest_row: u16,
        dest_col: u16,
        rect_rows: u16,
        rect_cols: u16,
        dim: PaneBodyDim,
        buf: &mut Vec<u8>,
    ) -> PaneBodyRenderStats {
        let snapshot =
            pane_snapshot_with_scrollback_prefix(screen, scrollback_prefix, rect_rows, rect_cols);
        self.render_full_from_snapshot(snapshot, dest_row, dest_col, rect_rows, rect_cols, dim, buf)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_full_from_snapshot(
        &mut self,
        snapshot: Vec<RowSnapshot>,
        dest_row: u16,
        dest_col: u16,
        rect_rows: u16,
        rect_cols: u16,
        dim: PaneBodyDim,
        buf: &mut Vec<u8>,
    ) -> PaneBodyRenderStats {
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
        dim: PaneBodyDim,
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
/// `(rect_rows, rect_cols)`. Coordinates are 0-based. Inactive panes
/// get a light ANSI-dim cue; modal backdrops use the stronger darkened
/// color treatment so dialogs clearly own the whole terminal.
pub fn render_pane(
    screen: &Screen,
    dest_row: u16,
    dest_col: u16,
    rect_rows: u16,
    rect_cols: u16,
    dim: PaneBodyDim,
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
    pane_snapshot_with_scrollback_prefix(screen, &[], rect_rows, rect_cols)
}

fn pane_snapshot_with_scrollback_prefix(
    screen: &Screen,
    scrollback_prefix: &[String],
    rect_rows: u16,
    rect_cols: u16,
) -> Vec<RowSnapshot> {
    let (screen_rows, screen_cols) = screen.size();
    let rows_to_draw = rect_rows.min(screen_rows);
    let cols_to_draw = rect_cols.min(screen_cols);
    let prefix_rows = scrollback_prefix.len().min(usize::from(rows_to_draw));
    let mut snapshot = Vec::with_capacity(usize::from(rows_to_draw));
    snapshot.extend(
        scrollback_prefix
            .iter()
            .take(prefix_rows)
            .map(|row| snapshot_plain_row(row, cols_to_draw)),
    );
    snapshot.extend(
        (0..rows_to_draw.saturating_sub(prefix_rows as u16))
            .map(|row| snapshot_row(screen, row, cols_to_draw)),
    );
    snapshot
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

fn snapshot_plain_row(text: &str, cols_to_draw: u16) -> RowSnapshot {
    let mut contents = String::new();
    let mut width = 0u16;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
        if ch_width == 0 {
            contents.push(ch);
            continue;
        }
        if width.saturating_add(ch_width) > cols_to_draw {
            break;
        }
        contents.push(ch);
        width += ch_width;
    }
    contents.extend(std::iter::repeat_n(
        ' ',
        usize::from(cols_to_draw.saturating_sub(width)),
    ));
    RowSnapshot {
        cells: vec![CellSnapshot {
            contents,
            attrs: Attrs::default(),
            width: cols_to_draw,
        }],
    }
}

fn render_snapshot_rows(
    snapshot: &[RowSnapshot],
    rows: &[u16],
    dest_row: u16,
    dest_col: u16,
    dim: PaneBodyDim,
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

fn emit_sgr(buf: &mut Vec<u8>, a: &Attrs, dim: PaneBodyDim) {
    buf.extend_from_slice(b"\x1b[0");
    // Cell-level dim (Amp uses this for its animated bottom-bar) uses
    // ANSI dim. Dialog backdrop dim is intentionally stronger: ANSI dim
    // is subtle in many terminals, so modal background cells also get
    // darkened foreground/background colors below.
    if a.dim || dim != PaneBodyDim::Normal {
        buf.extend_from_slice(b";2");
    }
    if a.bold && dim != PaneBodyDim::Backdrop {
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
    if dim == PaneBodyDim::Backdrop {
        emit_backdrop_fg(buf, a.fg);
        emit_backdrop_bg(buf, a.bg);
    } else {
        emit_fg(buf, a.fg);
        emit_bg(buf, a.bg);
    }
    buf.push(b'm');
}

/// Which SGR plane an emit targets. `Fg` uses the `3x`/`9x`/`38;…`
/// family; `Bg` uses `4x`/`10x`/`48;…`. The two emit functions are
/// parameterised over this so a future palette change moves once.
#[derive(Clone, Copy)]
enum SgrLayer {
    Fg,
    Bg,
}

impl SgrLayer {
    const fn low(self) -> u8 {
        match self {
            Self::Fg => 3,
            Self::Bg => 4,
        }
    }
    const fn bright(self) -> u8 {
        match self {
            Self::Fg => 9,
            Self::Bg => 10,
        }
    }
    const fn truecolor(self) -> u8 {
        match self {
            Self::Fg => 38,
            Self::Bg => 48,
        }
    }
    const fn backdrop_default_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Fg => (58, 58, 58),
            Self::Bg => (0, 0, 0),
        }
    }
}

fn emit_color(buf: &mut Vec<u8>, layer: SgrLayer, color: ColorKey) {
    match color {
        ColorKey::Default => {}
        ColorKey::Idx(n) if n < 8 => {
            let _ = write!(buf, ";{}{n}", layer.low());
        }
        ColorKey::Idx(n) if n < 16 => {
            let _ = write!(buf, ";{}{}", layer.bright(), n - 8);
        }
        ColorKey::Idx(n) => {
            let _ = write!(buf, ";{};5;{n}", layer.truecolor());
        }
        ColorKey::Rgb(r, g, b) => {
            let _ = write!(buf, ";{};2;{r};{g};{b}", layer.truecolor());
        }
    }
}

fn emit_fg(buf: &mut Vec<u8>, color: ColorKey) {
    emit_color(buf, SgrLayer::Fg, color);
}

fn emit_bg(buf: &mut Vec<u8>, color: ColorKey) {
    emit_color(buf, SgrLayer::Bg, color);
}

fn emit_backdrop(buf: &mut Vec<u8>, layer: SgrLayer, color: ColorKey) {
    let (r, g, b) = match color {
        ColorKey::Default => layer.backdrop_default_rgb(),
        ColorKey::Idx(n) => dim_indexed_color(n),
        ColorKey::Rgb(r, g, b) => (strong_dim(r), strong_dim(g), strong_dim(b)),
    };
    let _ = write!(buf, ";{};2;{r};{g};{b}", layer.truecolor());
}

fn emit_backdrop_fg(buf: &mut Vec<u8>, color: ColorKey) {
    emit_backdrop(buf, SgrLayer::Fg, color);
}

fn emit_backdrop_bg(buf: &mut Vec<u8>, color: ColorKey) {
    emit_backdrop(buf, SgrLayer::Bg, color);
}

const fn strong_dim(value: u8) -> u8 {
    value / 5
}

const fn dim_indexed_color(idx: u8) -> (u8, u8, u8) {
    let (r, g, b) = match idx & 0x0f {
        0 => (0, 0, 0),
        1 => (170, 0, 0),
        2 => (0, 170, 0),
        3 => (170, 85, 0),
        4 => (0, 0, 170),
        5 => (170, 0, 170),
        6 => (0, 170, 170),
        7 => (170, 170, 170),
        8 => (85, 85, 85),
        9 => (255, 85, 85),
        10 => (85, 255, 85),
        11 => (255, 255, 85),
        12 => (85, 85, 255),
        13 => (255, 85, 255),
        14 => (85, 255, 255),
        _ => (255, 255, 255),
    };
    (strong_dim(r), strong_dim(g), strong_dim(b))
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
        render_pane(parser.screen(), 4, 2, 3, 10, PaneBodyDim::Normal, &mut buf);
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
    fn dialog_backdrop_dim_uses_strong_darkened_colors() {
        let mut parser = Parser::new(1, 10, 0);
        parser.process(b"\x1b[31mred");
        let mut buf = Vec::new();
        render_pane(
            parser.screen(),
            0,
            0,
            1,
            10,
            PaneBodyDim::Backdrop,
            &mut buf,
        );
        let out = String::from_utf8_lossy(&buf);

        assert!(
            out.contains(";2;38;2;34;0;0;48;2;0;0;0m"),
            "dialog backdrop should darken colors, not rely on ANSI dim alone: {out:?}"
        );
    }

    #[test]
    fn inactive_pane_dim_uses_light_ansi_dim_only() {
        let mut parser = Parser::new(1, 10, 0);
        parser.process(b"\x1b[31mred");
        let mut buf = Vec::new();
        render_pane(
            parser.screen(),
            0,
            0,
            1,
            10,
            PaneBodyDim::Inactive,
            &mut buf,
        );
        let out = String::from_utf8_lossy(&buf);

        assert!(
            out.contains("\x1b[0;2;31mred"),
            "inactive pane should keep normal color codes with ANSI dim: {out:?}"
        );
        assert!(
            !out.contains(";38;2;34;0;0"),
            "inactive pane should not use the strong dialog-backdrop darkening: {out:?}"
        );
    }

    #[test]
    fn pane_cache_first_render_is_full_and_tracks_every_visible_row() {
        let mut parser = Parser::new(3, 8, 0);
        parser.process(b"one\r\ntwo");
        let mut cache = PaneBodyCache::default();
        let mut buf = Vec::new();

        let stats =
            cache.render_partial(parser.screen(), 10, 20, 3, 8, PaneBodyDim::Normal, &mut buf);

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
        cache.render_full(parser.screen(), 0, 0, 3, 12, PaneBodyDim::Normal, &mut buf);
        buf.clear();

        parser.process(b"\x1b[2;1Hbravo");
        let stats =
            cache.render_partial(parser.screen(), 0, 0, 3, 12, PaneBodyDim::Normal, &mut buf);

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
        cache.render_full(parser.screen(), 0, 0, 2, 16, PaneBodyDim::Normal, &mut buf);
        buf.clear();

        parser.process(b"\x1b[1;1H\x1b[32mgreen\x1b[0m");
        let stats =
            cache.render_partial(parser.screen(), 0, 0, 2, 16, PaneBodyDim::Normal, &mut buf);

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
        cache.render_full(parser.screen(), 0, 0, 2, 10, PaneBodyDim::Normal, &mut buf);
        buf.clear();

        parser.process("\x1b[1;3Hy".as_bytes());
        let stats =
            cache.render_partial(parser.screen(), 0, 0, 2, 10, PaneBodyDim::Normal, &mut buf);

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
        cache.render_full(parser.screen(), 0, 0, 1, 8, PaneBodyDim::Normal, &mut buf);
        buf.clear();

        parser.process(b"\x1b[1;1H\x1b[38;2;1;2;3;48;5;4;1mX");
        let stats =
            cache.render_partial(parser.screen(), 0, 0, 1, 8, PaneBodyDim::Normal, &mut buf);

        assert_eq!(stats.changed_rows, vec![0]);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("\x1b[0;1;38;2;1;2;3;44mX"));
    }
}
