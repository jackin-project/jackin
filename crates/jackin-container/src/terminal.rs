/// Virtual terminal emulator backed by the `vte` crate.
///
/// Tracks the visible cell grid for each pane so we can re-render when the
/// operator switches sessions. Only the subset of VTE needed for multiplexer
/// correctness is implemented — cursor movement, SGR attributes, erasure, and
/// character printing.
use vte::{Params, Parser, Perform};

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub underline: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self { ch: ' ', fg: Color::Default, bg: Color::Default, bold: false, underline: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Default,
    Ansi(u8),
    Rgb(u8, u8, u8),
}

pub struct VirtualTerminal {
    pub rows: u16,
    pub cols: u16,
    cells: Vec<Vec<Cell>>,
    cursor_row: u16,
    cursor_col: u16,
    scroll_top: u16,
    scroll_bot: u16,
    current_fg: Color,
    current_bg: Color,
    current_bold: bool,
    current_underline: bool,
    in_alt_screen: bool,
    saved_cursor: (u16, u16),
    parser: Parser,
}

impl VirtualTerminal {
    pub fn new(rows: u16, cols: u16) -> Self {
        let cells = vec![vec![Cell::default(); cols as usize]; rows as usize];
        Self {
            rows,
            cols,
            cells,
            cursor_row: 0,
            cursor_col: 0,
            scroll_top: 0,
            scroll_bot: rows.saturating_sub(1),
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_bold: false,
            current_underline: false,
            in_alt_screen: false,
            saved_cursor: (0, 0),
            parser: Parser::new(),
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
        self.cells.resize(rows as usize, vec![Cell::default(); cols as usize]);
        for row in &mut self.cells {
            row.resize(cols as usize, Cell::default());
        }
        self.scroll_bot = rows.saturating_sub(1);
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
    }

    pub fn process(&mut self, bytes: &[u8]) {
        // vte::Perform requires &mut self but Parser::advance takes (&mut Perform, byte).
        // We need to temporarily own the parser, process, then restore it.
        // SAFETY: we don't access self through the parser during processing.
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        for &b in bytes {
            parser.advance(self, b);
        }
        self.parser = parser;
    }

    /// Render the grid as ANSI escape sequences suitable for writing to a
    /// real terminal, starting at (dest_row, dest_col) in the host terminal.
    pub fn render_to(&self, dest_row: u16, dest_col: u16, buf: &mut Vec<u8>) {
        let mut last_fg = Color::Default;
        let mut last_bg = Color::Default;
        let mut last_bold = false;

        // Reset attributes once at the start.
        buf.extend_from_slice(b"\x1b[0m");

        for (r, row) in self.cells.iter().enumerate() {
            // Move cursor to destination row + r, destination col.
            let target_row = dest_row + r as u16;
            write_cursor_pos(buf, target_row, dest_col);

            for cell in row {
                // Emit SGR only when attributes change.
                if cell.fg != last_fg || cell.bg != last_bg || cell.bold != last_bold {
                    buf.extend_from_slice(b"\x1b[");
                    let mut first = true;

                    if last_bold && !cell.bold {
                        if !first { buf.push(b';'); }
                        buf.extend_from_slice(b"0");
                        first = false;
                        // After reset, also re-emit fg/bg.
                        last_fg = Color::Default;
                        last_bg = Color::Default;
                    }
                    if cell.bold && !last_bold {
                        if !first { buf.push(b';'); }
                        buf.extend_from_slice(b"1");
                        first = false;
                    }
                    if cell.fg != last_fg {
                        if !first { buf.push(b';'); }
                        write_fg(buf, cell.fg);
                        first = false;
                        last_fg = cell.fg;
                    }
                    if cell.bg != last_bg {
                        if !first { buf.push(b';'); }
                        write_bg(buf, cell.bg);
                        #[allow(unused_assignments)]
                        { first = false; }
                        last_bg = cell.bg;
                    }
                    buf.push(b'm');
                    last_bold = cell.bold;
                }

                // Encode the character as UTF-8.
                let mut tmp = [0u8; 4];
                buf.extend_from_slice(cell.ch.encode_utf8(&mut tmp).as_bytes());
            }
        }
        buf.extend_from_slice(b"\x1b[0m");
    }

    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.cells
            .get(row as usize)
            .and_then(|r| r.get(col as usize))
    }

    fn put_char(&mut self, ch: char) {
        let r = self.cursor_row as usize;
        let c = self.cursor_col as usize;
        if r < self.cells.len() && c < self.cols as usize {
            self.cells[r][c] = Cell {
                ch,
                fg: self.current_fg,
                bg: self.current_bg,
                bold: self.current_bold,
                underline: self.current_underline,
            };
        }
        self.cursor_col += 1;
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.advance_line();
        }
    }

    fn advance_line(&mut self) {
        if self.cursor_row < self.scroll_bot {
            self.cursor_row += 1;
        } else {
            // Scroll up: drop top row of scroll region, add blank at bottom.
            let st = self.scroll_top as usize;
            let sb = self.scroll_bot as usize;
            if sb > st && sb < self.cells.len() {
                self.cells[st..=sb].rotate_left(1);
                let cols = self.cols as usize;
                self.cells[sb] = vec![Cell::default(); cols];
            }
        }
    }

    fn erase_in_line(&mut self, mode: u16) {
        let r = self.cursor_row as usize;
        if r >= self.cells.len() { return; }
        let c = self.cursor_col as usize;
        let cols = self.cols as usize;
        match mode {
            0 => { for col in c..cols { self.cells[r][col] = Cell::default(); } }
            1 => { for col in 0..=c.min(cols.saturating_sub(1)) { self.cells[r][col] = Cell::default(); } }
            2 => { self.cells[r] = vec![Cell::default(); cols]; }
            _ => {}
        }
    }

    fn erase_in_display(&mut self, mode: u16) {
        let cols = self.cols as usize;
        let rows = self.rows as usize;
        let r = self.cursor_row as usize;
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in (r + 1)..rows {
                    self.cells[row] = vec![Cell::default(); cols];
                }
            }
            1 => {
                for row in 0..r {
                    self.cells[row] = vec![Cell::default(); cols];
                }
                self.erase_in_line(1);
            }
            2 | 3 => {
                for row in &mut self.cells {
                    *row = vec![Cell::default(); cols];
                }
            }
            _ => {}
        }
    }
}

impl Perform for VirtualTerminal {
    fn print(&mut self, ch: char) {
        self.put_char(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => { self.cursor_col = 0; }
            b'\n' | b'\x0b' | b'\x0c' => { self.advance_line(); }
            b'\x08' => { self.cursor_col = self.cursor_col.saturating_sub(1); }
            b'\t' => {
                let next = (self.cursor_col + 8) & !7;
                self.cursor_col = next.min(self.cols.saturating_sub(1));
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let ps: Vec<u16> = params.iter()
            .map(|subp| subp.first().copied().unwrap_or(0))
            .collect();
        let p0 = ps.first().copied().unwrap_or(0);
        let p1 = ps.get(1).copied().unwrap_or(0);

        match action {
            // Cursor movement
            'A' => { self.cursor_row = self.cursor_row.saturating_sub(p0.max(1)); }
            'B' => { self.cursor_row = (self.cursor_row + p0.max(1)).min(self.rows.saturating_sub(1)); }
            'C' => { self.cursor_col = (self.cursor_col + p0.max(1)).min(self.cols.saturating_sub(1)); }
            'D' => { self.cursor_col = self.cursor_col.saturating_sub(p0.max(1)); }
            'E' => { self.cursor_row = (self.cursor_row + p0.max(1)).min(self.rows.saturating_sub(1)); self.cursor_col = 0; }
            'F' => { self.cursor_row = self.cursor_row.saturating_sub(p0.max(1)); self.cursor_col = 0; }
            'G' => { self.cursor_col = p0.saturating_sub(1).min(self.cols.saturating_sub(1)); }
            'H' | 'f' => {
                self.cursor_row = p0.saturating_sub(1).min(self.rows.saturating_sub(1));
                self.cursor_col = p1.saturating_sub(1).min(self.cols.saturating_sub(1));
            }
            'd' => { self.cursor_row = p0.saturating_sub(1).min(self.rows.saturating_sub(1)); }
            // Erase
            'J' => { self.erase_in_display(p0); }
            'K' => { self.erase_in_line(p0); }
            // Scroll
            'S' => {
                let n = p0.max(1) as usize;
                let st = self.scroll_top as usize;
                let sb = self.scroll_bot as usize;
                let cols = self.cols as usize;
                if sb > st {
                    for _ in 0..n {
                        self.cells[st..=sb].rotate_left(1);
                        self.cells[sb] = vec![Cell::default(); cols];
                    }
                }
            }
            'T' => {
                let n = p0.max(1) as usize;
                let st = self.scroll_top as usize;
                let sb = self.scroll_bot as usize;
                let cols = self.cols as usize;
                if sb > st {
                    for _ in 0..n {
                        self.cells[st..=sb].rotate_right(1);
                        self.cells[st] = vec![Cell::default(); cols];
                    }
                }
            }
            // Scrolling region
            'r' => {
                let top = p0.saturating_sub(1).min(self.rows.saturating_sub(1));
                let bot = p1.saturating_sub(1).min(self.rows.saturating_sub(1));
                if top < bot { self.scroll_top = top; self.scroll_bot = bot; }
                else { self.scroll_top = 0; self.scroll_bot = self.rows.saturating_sub(1); }
            }
            // Save/restore cursor
            's' => { self.saved_cursor = (self.cursor_row, self.cursor_col); }
            'u' => { let (r, c) = self.saved_cursor; self.cursor_row = r; self.cursor_col = c; }
            // SGR
            'm' => {
                if ps.is_empty() || (ps.len() == 1 && p0 == 0) {
                    self.current_fg = Color::Default;
                    self.current_bg = Color::Default;
                    self.current_bold = false;
                    self.current_underline = false;
                    return;
                }
                let mut i = 0;
                while i < ps.len() {
                    match ps[i] {
                        0 => {
                            self.current_fg = Color::Default;
                            self.current_bg = Color::Default;
                            self.current_bold = false;
                            self.current_underline = false;
                        }
                        1 => { self.current_bold = true; }
                        4 => { self.current_underline = true; }
                        22 => { self.current_bold = false; }
                        24 => { self.current_underline = false; }
                        30..=37 => { self.current_fg = Color::Ansi(ps[i] as u8 - 30); }
                        38 => {
                            if ps.get(i + 1).copied() == Some(5) {
                                if let Some(&n) = ps.get(i + 2) {
                                    self.current_fg = Color::Ansi(n as u8);
                                    i += 2;
                                }
                            } else if ps.get(i + 1).copied() == Some(2) {
                                if let (Some(&r), Some(&g), Some(&b)) = (ps.get(i+2), ps.get(i+3), ps.get(i+4)) {
                                    self.current_fg = Color::Rgb(r as u8, g as u8, b as u8);
                                    i += 4;
                                }
                            }
                        }
                        39 => { self.current_fg = Color::Default; }
                        40..=47 => { self.current_bg = Color::Ansi(ps[i] as u8 - 40); }
                        48 => {
                            if ps.get(i + 1).copied() == Some(5) {
                                if let Some(&n) = ps.get(i + 2) {
                                    self.current_bg = Color::Ansi(n as u8);
                                    i += 2;
                                }
                            } else if ps.get(i + 1).copied() == Some(2) {
                                if let (Some(&r), Some(&g), Some(&b)) = (ps.get(i+2), ps.get(i+3), ps.get(i+4)) {
                                    self.current_bg = Color::Rgb(r as u8, g as u8, b as u8);
                                    i += 4;
                                }
                            }
                        }
                        49 => { self.current_bg = Color::Default; }
                        90..=97 => { self.current_fg = Color::Ansi(ps[i] as u8 - 90 + 8); }
                        100..=107 => { self.current_bg = Color::Ansi(ps[i] as u8 - 100 + 8); }
                        _ => {}
                    }
                    i += 1;
                }
            }
            // Line erase / insert
            'P' => {
                let r = self.cursor_row as usize;
                let c = self.cursor_col as usize;
                let n = p0.max(1) as usize;
                if r < self.cells.len() {
                    let cols = self.cols as usize;
                    let row = &mut self.cells[r];
                    let end = c.saturating_add(n).min(cols);
                    row.drain(c..end);
                    row.resize(cols, Cell::default());
                }
            }
            '@' => {
                let r = self.cursor_row as usize;
                let c = self.cursor_col as usize;
                let n = p0.max(1) as usize;
                if r < self.cells.len() {
                    let cols = self.cols as usize;
                    let row = &mut self.cells[r];
                    for _ in 0..n { row.insert(c, Cell::default()); }
                    row.truncate(cols);
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'M' => {
                // Reverse index: move cursor up, scroll if at top of scroll region.
                if self.cursor_row == self.scroll_top {
                    let st = self.scroll_top as usize;
                    let sb = self.scroll_bot as usize;
                    let cols = self.cols as usize;
                    if sb > st {
                        self.cells[st..=sb].rotate_right(1);
                        self.cells[st] = vec![Cell::default(); cols];
                    }
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
            }
            b'7' => { self.saved_cursor = (self.cursor_row, self.cursor_col); }
            b'8' => { let (r, c) = self.saved_cursor; self.cursor_row = r; self.cursor_col = c; }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

fn write_cursor_pos(buf: &mut Vec<u8>, row: u16, col: u16) {
    // CSI <row+1> ; <col+1> H
    buf.extend_from_slice(b"\x1b[");
    write_u16(buf, row + 1);
    buf.push(b';');
    write_u16(buf, col + 1);
    buf.push(b'H');
}

fn write_u16(buf: &mut Vec<u8>, n: u16) {
    if n == 0 { buf.push(b'0'); return; }
    let mut tmp = [0u8; 5];
    let mut i = 5;
    let mut n = n;
    while n > 0 {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    buf.extend_from_slice(&tmp[i..]);
}

fn write_fg(buf: &mut Vec<u8>, color: Color) {
    match color {
        Color::Default => buf.extend_from_slice(b"39"),
        Color::Ansi(n) if n < 8 => { buf.extend_from_slice(b"3"); write_u16(buf, n as u16); }
        Color::Ansi(n) if n < 16 => { buf.extend_from_slice(b"9"); write_u16(buf, (n - 8) as u16); }
        Color::Ansi(n) => { buf.extend_from_slice(b"38;5;"); write_u16(buf, n as u16); }
        Color::Rgb(r, g, b) => {
            buf.extend_from_slice(b"38;2;");
            write_u16(buf, r as u16); buf.push(b';');
            write_u16(buf, g as u16); buf.push(b';');
            write_u16(buf, b as u16);
        }
    }
}

fn write_bg(buf: &mut Vec<u8>, color: Color) {
    match color {
        Color::Default => buf.extend_from_slice(b"49"),
        Color::Ansi(n) if n < 8 => { buf.extend_from_slice(b"4"); write_u16(buf, n as u16); }
        Color::Ansi(n) if n < 16 => { buf.extend_from_slice(b"10"); write_u16(buf, (n - 8) as u16); }
        Color::Ansi(n) => { buf.extend_from_slice(b"48;5;"); write_u16(buf, n as u16); }
        Color::Rgb(r, g, b) => {
            buf.extend_from_slice(b"48;2;");
            write_u16(buf, r as u16); buf.push(b';');
            write_u16(buf, g as u16); buf.push(b';');
            write_u16(buf, b as u16);
        }
    }
}
