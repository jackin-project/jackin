//! `DamageGrid` — the Phase 2 v0 terminal model implementation.
//!
//! Uses a straightforward `Vec<Vec<Cell>>` grid (correctness before memory
//! model — Phase 4 replaces this with the Ghostty-inspired PageList arena).
//!
//! The key new capability vs `vt100`: `dirty_spans()` reports which rows were
//! mutated since the last call, recorded *as* `Perform` mutates the grid —
//! not recomputed by a full-grid diff.
//!
//! Implements `vte::Perform` directly so the capsule can swap in `DamageGrid`
//! wherever it currently calls `vt100::Parser::process()`.
//!
//! # Attribution
//! Grid structure inspired by Alacritty `alacritty_terminal::grid::Grid`
//! (Apache-2.0/MIT) and Zellij `zellij-server::panes::grid::Grid` (MIT).
//! Neither crate is a dependency; only the design pattern is borrowed.

use unicode_width::UnicodeWidthChar;

use crate::cell::{Attrs, Cell, Color};
use crate::damage::{DirtySpans, DirtyTracker};
use crate::passthrough::{PassthroughBuffer, PassthroughEvent};

/// Mouse protocol modes (matching the vt100 coupling surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseProtocolMode {
    #[default]
    None,
    Press,
    PressRelease,
    AnyEvent,
}

/// Mouse protocol encodings (matching the vt100 coupling surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseProtocolEncoding {
    #[default]
    Default,
    Utf8,
    Sgr,
    Urxvt,
}

/// The Phase 2 v0 terminal model.
///
/// Call `process(bytes)` to feed raw PTY output.  The grid records which
/// rows changed via `dirty.mark_row()`.  Call `dirty_spans()` to retrieve
/// and clear the dirty set before rendering.
pub struct DamageGrid {
    // ── Grid state ────────────────────────────────────────────────────────────
    rows: u16,
    cols: u16,
    /// Primary screen cells.
    primary: Vec<Vec<Cell>>,
    /// Alternate screen cells (activated by `?1049h`).
    alternate: Vec<Vec<Cell>>,
    /// True when the alternate screen is active.
    alt_screen: bool,
    /// Scrollback buffer (primary screen only). Newest entry = last item.
    scrollback: Vec<Vec<Cell>>,
    /// Max scrollback rows kept.
    scrollback_limit: usize,
    /// Current scrollback view offset (0 = live tail).
    scrollback_offset: usize,

    // ── Cursor ────────────────────────────────────────────────────────────────
    cursor_row: u16,
    cursor_col: u16,
    saved_cursor_row: u16,
    saved_cursor_col: u16,

    // ── Modes ─────────────────────────────────────────────────────────────────
    mouse_mode: MouseProtocolMode,
    mouse_encoding: MouseProtocolEncoding,
    hide_cursor: bool,
    bracketed_paste: bool,
    application_cursor: bool,

    // ── Current SGR attributes (applied to newly written cells) ───────────────
    current_attrs: Attrs,

    // ── Scroll region ─────────────────────────────────────────────────────────
    scroll_top: u16,    // 0-based, inclusive
    scroll_bottom: u16, // 0-based, inclusive

    // ── Damage + passthrough ──────────────────────────────────────────────────
    pub dirty: DirtyTracker,
    pub passthrough: PassthroughBuffer,
}

impl DamageGrid {
    /// Create a new grid with the given dimensions and scrollback limit.
    pub fn new(rows: u16, cols: u16, scrollback_limit: usize) -> Self {
        let blank = make_blank_grid(rows, cols);
        Self {
            rows,
            cols,
            primary: blank.clone(),
            alternate: blank,
            alt_screen: false,
            scrollback: Vec::new(),
            scrollback_limit,
            scrollback_offset: 0,
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            mouse_mode: MouseProtocolMode::None,
            mouse_encoding: MouseProtocolEncoding::Default,
            hide_cursor: false,
            bracketed_paste: false,
            application_cursor: false,
            current_attrs: Attrs::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            dirty: DirtyTracker::default(),
            passthrough: PassthroughBuffer::default(),
        }
    }

    /// Feed raw PTY bytes through the vte parser, mutating the grid.
    /// Dirty rows are recorded via `self.dirty`.
    pub fn process(&mut self, bytes: &[u8]) {
        let mut parser = vte::Parser::new();
        parser.advance(self, bytes);
    }

    /// Drain and return the dirty-row set, clearing it for the next frame.
    pub fn dirty_spans(&mut self) -> DirtySpans {
        self.dirty.take()
    }

    /// Drain and return all passthrough events produced during `process()`.
    pub fn drain_passthrough(&mut self) -> Vec<PassthroughEvent> {
        self.passthrough.drain()
    }

    /// Dump the current screen state as a `GridSnapshot`.
    ///
    /// The snapshot is a complete, owned copy of the active grid — primary or
    /// alternate — plus cursor position and screen-mode flags. Use it for:
    /// - Acceptance tests that assert exact screen state.
    /// - Terminal observation (feeds the [terminal observation roadmap item]).
    /// - Debugging: `snap.to_text()` gives the visual contents as a string.
    ///
    /// Concept borrowed from `avt` (MIT, Marcin Kulik / asciinema). Implementation
    /// is our own — `avt` is not a dependency. Attribution in `snapshot.rs`.
    pub fn dump(&self) -> crate::snapshot::GridSnapshot {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        let cells = screen
            .iter()
            .map(|row| row.iter().map(crate::snapshot::SnapCell::from).collect())
            .collect();
        crate::snapshot::GridSnapshot {
            rows: self.rows,
            cols: self.cols,
            cursor: (self.cursor_row, self.cursor_col),
            alternate_screen: self.alt_screen,
            cells,
        }
    }

    // ── Coupling-surface accessors (matching vt100 API) ───────────────────────

    /// Grid dimensions `(rows, cols)`.
    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Resize the grid. Marks all rows dirty.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
        self.primary = resize_grid(&self.primary, rows, cols);
        self.alternate = resize_grid(&self.alternate, rows, cols);
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.dirty.mark_all();
    }

    /// Get a cell reference. Returns `None` if out of bounds.
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        let screen = if self.alt_screen {
            &self.alternate
        } else {
            &self.primary
        };
        screen.get(row as usize).and_then(|r| r.get(col as usize))
    }

    /// Cursor position `(row, col)`.
    pub fn cursor_position(&self) -> (u16, u16) {
        (self.cursor_row, self.cursor_col)
    }

    /// Whether the alternate screen is active.
    pub fn alternate_screen(&self) -> bool {
        self.alt_screen
    }

    /// Set the scrollback view offset. 0 = live tail; scrollback_limit = oldest.
    pub fn set_scrollback(&mut self, offset: usize) {
        self.scrollback_offset = offset.min(self.scrollback.len());
    }

    /// Current scrollback view offset.
    pub fn scrollback(&self) -> usize {
        self.scrollback_offset
    }

    /// Number of scrollback rows filled.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Clear the scrollback buffer. Used by capsule's `clear_scrollback`.
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.scrollback_offset = 0;
    }

    /// Mouse protocol mode.
    pub fn mouse_protocol_mode(&self) -> MouseProtocolMode {
        self.mouse_mode
    }

    /// Mouse protocol encoding.
    pub fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding {
        self.mouse_encoding
    }

    pub fn hide_cursor(&self) -> bool {
        self.hide_cursor
    }

    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    pub fn application_cursor(&self) -> bool {
        self.application_cursor
    }

    // ── Internal grid helpers ─────────────────────────────────────────────────

    fn active_grid(&mut self) -> &mut Vec<Vec<Cell>> {
        if self.alt_screen {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    /// Write a character at the current cursor position, advance cursor.
    fn write_char_at_cursor(&mut self, ch: char) {
        if self.cursor_row >= self.rows || self.cursor_col >= self.cols {
            return;
        }
        let width = UnicodeWidthChar::width(ch).unwrap_or(1) as u16;
        let row = self.cursor_row as usize;
        let col = self.cursor_col as usize;

        // Erase any prior wide char that would be partially overwritten.
        {
            let grid = self.active_grid();
            if col < grid[row].len() && grid[row][col].is_wide_continuation && col > 0 {
                grid[row][col - 1] = Cell::default();
            }
        }

        let attrs = self.current_attrs.clone();
        let cols = self.cols;
        let cell = Cell {
            // Phase 4: CompactString stores ch inline (no heap alloc for ASCII + most Unicode).
            contents: compact_str::format_compact!("{ch}"),
            is_wide: width == 2,
            is_wide_continuation: false,
            attrs: attrs.clone(),
        };
        {
            let grid = self.active_grid();
            grid[row][col] = cell;
            if width == 2 && col + 1 < cols as usize && col + 1 < grid[row].len() {
                grid[row][col + 1] = Cell {
                    contents: compact_str::CompactString::new(""),
                    is_wide: false,
                    is_wide_continuation: true,
                    attrs: attrs.clone(),
                };
            }
        }
        self.dirty.mark_row(self.cursor_row);

        self.cursor_col += width;
        if self.cursor_col >= self.cols {
            // Wrap to next line.
            self.cursor_col = 0;
            self.newline_action();
        }
    }

    /// Scroll the active scroll region up by `n` rows, pushing content to scrollback.
    fn scroll_up(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        for _ in 0..n {
            if !self.alt_screen && top == 0 {
                // Push top row to scrollback.
                let row = self.primary[0].clone();
                if self.scrollback.len() >= self.scrollback_limit {
                    self.scrollback.remove(0);
                }
                self.scrollback.push(row);
            }
            let cols = self.cols;
            let grid = self.active_grid();
            if bottom < grid.len() && top < bottom {
                for r in top..bottom {
                    if r + 1 < grid.len() {
                        grid[r] = grid[r + 1].clone();
                    }
                }
                grid[bottom] = blank_row(cols);
            }
        }
        for r in top as u16..=bottom as u16 {
            self.dirty.mark_row(r);
        }
    }

    /// Newline action: move down or scroll.
    fn newline_action(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor_row = (self.cursor_row + 1).min(self.rows.saturating_sub(1));
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor_row = self.cursor_row.min(self.rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.cols.saturating_sub(1));
    }

    fn erase_line(&mut self, mode: u16) {
        let row = self.cursor_row as usize;
        let col = self.cursor_col as usize;
        let cols_u16 = self.cols;
        let cols = cols_u16 as usize;
        let cursor_row = self.cursor_row;
        {
            let grid = self.active_grid();
            match mode {
                0 => {
                    grid[row][col..cols].fill(Cell::default());
                }
                1 => {
                    grid[row][0..=col.min(cols - 1)].fill(Cell::default());
                }
                2 => {
                    grid[row] = blank_row(cols_u16);
                }
                _ => {}
            }
        }
        self.dirty.mark_row(cursor_row);
    }

    fn erase_display(&mut self, mode: u16) {
        let cursor_row = self.cursor_row as usize;
        let cursor_col = self.cursor_col as usize;
        let rows = self.rows as usize;
        let cols_usize = self.cols as usize;
        let cols_u16 = self.cols;
        match mode {
            0 => {
                let grid = self.active_grid();
                grid[cursor_row][cursor_col..cols_usize].fill(Cell::default());
                for row in grid.iter_mut().take(rows).skip(cursor_row + 1) {
                    *row = blank_row(cols_u16);
                }
            }
            1 => {
                let grid = self.active_grid();
                for row in grid.iter_mut().take(cursor_row) {
                    *row = blank_row(cols_u16);
                }
                grid[cursor_row][0..=cursor_col.min(cols_usize - 1)].fill(Cell::default());
            }
            2 => {
                let grid = self.active_grid();
                for row in grid.iter_mut().take(rows) {
                    *row = blank_row(cols_u16);
                }
            }
            3 => {
                self.scrollback.clear();
                self.scrollback_offset = 0;
                let grid = self.active_grid();
                for row in grid.iter_mut().take(rows) {
                    *row = blank_row(cols_u16);
                }
            }
            _ => {}
        }
        self.dirty.mark_all();
    }

    /// Parse an OSC sequence payload and emit a passthrough event.
    fn handle_osc(&mut self, params: &[&[u8]]) {
        let Some(&code_bytes) = params.first() else {
            return;
        };
        let code = std::str::from_utf8(code_bytes)
            .ok()
            .and_then(|s| s.parse::<u8>().ok());
        let value = params.get(1).and_then(|b| std::str::from_utf8(b).ok());
        match (code, value) {
            (Some(0 | 2), Some(title)) => {
                self.passthrough
                    .push(PassthroughEvent::TitleChanged(title.to_string()));
            }
            (Some(1), Some(name)) => {
                self.passthrough
                    .push(PassthroughEvent::IconNameChanged(name.to_string()));
            }
            (Some(52), Some(payload)) => {
                self.passthrough
                    .push(PassthroughEvent::ClipboardWrite(payload.to_string()));
            }
            (Some(7), Some(uri)) => {
                self.passthrough
                    .push(PassthroughEvent::CwdChanged(uri.to_string()));
            }
            _ => {}
        }
    }
}

// ── vte::Perform implementation ────────────────────────────────────────────

impl vte::Perform for DamageGrid {
    fn print(&mut self, ch: char) {
        self.write_char_at_cursor(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // LF / VT / FF — newline.
            0x0a..=0x0c => {
                self.newline_action();
                self.dirty.mark_row(self.cursor_row);
            }
            // CR — carriage return.
            0x0d => {
                self.cursor_col = 0;
            }
            // BS — backspace.
            0x08 => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            // HT — horizontal tab (move to next tab stop, 8-col aligned).
            0x09 => {
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next_tab.min(self.cols.saturating_sub(1));
            }
            // BEL — ignore.
            0x07 => {}
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Collect param values (0 if absent/empty, as per VT semantics).
        let p: Vec<u16> = params
            .iter()
            .map(|sub| sub.first().copied().unwrap_or(0))
            .collect();
        let p0 = p.first().copied().unwrap_or(0);
        let p1 = p.get(1).copied().unwrap_or(0);

        match action {
            // Insert Characters (ICH) — insert n blank chars at cursor, shift right.
            '@' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                let grid = self.active_grid();
                let row_cells = &mut grid[row];
                // Shift existing chars right, dropping any that fall off the end.
                let end = cols.min(row_cells.len());
                for c in (col..end.saturating_sub(n)).rev() {
                    row_cells[c + n] = row_cells[c].clone();
                }
                // Fill inserted cells with blanks.
                for cell in row_cells.iter_mut().take((col + n).min(end)).skip(col) {
                    *cell = Cell::default();
                }
                self.dirty.mark_row(self.cursor_row);
            }
            // Cursor Up.
            'A' => {
                let n = p0.max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.clamp_cursor();
            }
            // Cursor Down.
            'B' => {
                let n = p0.max(1);
                self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
                self.clamp_cursor();
            }
            // Cursor Forward.
            'C' => {
                let n = p0.max(1);
                self.cursor_col = (self.cursor_col + n).min(self.cols.saturating_sub(1));
                self.clamp_cursor();
            }
            // Cursor Back.
            'D' => {
                let n = p0.max(1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
                self.clamp_cursor();
            }
            // Cursor Next Line.
            'E' => {
                let n = p0.max(1);
                self.cursor_row = (self.cursor_row + n).min(self.rows.saturating_sub(1));
                self.cursor_col = 0;
            }
            // Cursor Previous Line.
            'F' => {
                let n = p0.max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            // Cursor Horizontal Absolute.
            'G' => {
                let col = p0.saturating_sub(1);
                self.cursor_col = col.min(self.cols.saturating_sub(1));
            }
            // Cursor Position (row, col, 1-based).
            'H' | 'f' => {
                let row = p0.saturating_sub(1);
                let col = p1.saturating_sub(1);
                self.cursor_row = row.min(self.rows.saturating_sub(1));
                self.cursor_col = col.min(self.cols.saturating_sub(1));
            }
            // Erase in Display.
            'J' => {
                self.erase_display(p0);
            }
            // Erase in Line.
            'K' => {
                self.erase_line(p0);
            }
            // Insert Lines.
            'L' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let bottom = self.scroll_bottom as usize;
                let cols = self.cols;
                let grid = self.active_grid();
                for _ in 0..n {
                    if bottom < grid.len() {
                        grid.remove(bottom);
                    }
                    grid.insert(row, blank_row(cols));
                }
                self.dirty.mark_all();
            }
            // Delete Lines.
            'M' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let bottom = self.scroll_bottom as usize;
                let cols = self.cols;
                let grid = self.active_grid();
                for _ in 0..n {
                    if row < grid.len() {
                        grid.remove(row);
                    }
                    if bottom < grid.len() + 1 {
                        grid.insert(bottom, blank_row(cols));
                    }
                }
                self.dirty.mark_all();
            }
            // Delete Characters.
            'P' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                let grid = self.active_grid();
                let row_cells = &mut grid[row];
                for c in col..cols.saturating_sub(n) {
                    row_cells[c] = row_cells.get(c + n).cloned().unwrap_or_default();
                }
                let tail_start = cols.saturating_sub(n);
                row_cells[tail_start..cols].fill(Cell::default());
                self.dirty.mark_row(self.cursor_row);
            }
            // Scroll Up.
            'S' => {
                let n = p0.max(1);
                self.scroll_up(n);
            }
            // Scroll Down.
            'T' => {
                let n = p0.max(1) as usize;
                let top = self.scroll_top as usize;
                let bottom = self.scroll_bottom as usize;
                let cols = self.cols;
                let grid = self.active_grid();
                for _ in 0..n {
                    if bottom < grid.len() {
                        grid.remove(bottom);
                    }
                    grid.insert(top, blank_row(cols));
                }
                self.dirty.mark_all();
            }
            // Erase Characters.
            'X' => {
                let n = p0.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let grid = self.active_grid();
                let end = (col + n).min(grid[row].len());
                grid[row][col..end].fill(Cell::default());
                self.dirty.mark_row(self.cursor_row);
            }
            // Cursor Vertical Absolute.
            'd' => {
                let row = p0.saturating_sub(1);
                self.cursor_row = row.min(self.rows.saturating_sub(1));
            }
            // SGR — Select Graphic Rendition.
            'm' => {
                self.apply_sgr(&p);
            }
            // DEC Private Mode Set.
            'h' if intermediates == b"?" => {
                for &mode in &p {
                    self.set_dec_mode(mode, true);
                }
            }
            // DEC Private Mode Reset.
            'l' if intermediates == b"?" => {
                for &mode in &p {
                    self.set_dec_mode(mode, false);
                }
            }
            // Set Scrolling Region.
            // DECSTBM: Set Top and Bottom Margins (scroll region).
            // After setting the scroll region, cursor is homed to (0, 0).
            'r' => {
                let top = p0.saturating_sub(1);
                let bottom = if p1 == 0 {
                    self.rows.saturating_sub(1)
                } else {
                    p1.saturating_sub(1).min(self.rows.saturating_sub(1))
                };
                if top < bottom {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                } else {
                    // Invalid region: reset to full screen.
                    self.scroll_top = 0;
                    self.scroll_bottom = self.rows.saturating_sub(1);
                }
                // VT100 spec: cursor is positioned at the upper-left after DECSTBM.
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            // Save Cursor.
            's' => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // Restore Cursor.
            'u' => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
                self.clamp_cursor();
            }
            // DECSC (ESC 7) save cursor — emitted as CSI with intermediate '7' sometimes.
            _ => {
                // Unhandled CSI — emit as passthrough.
                // (Don't collect the full bytes here — just note it for passthrough.)
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            // ESC M — Reverse Index (RI): move cursor up one row.
            // If cursor is at the top margin, scroll content DOWN one row instead.
            b'M' => {
                if self.cursor_row == self.scroll_top {
                    // Scroll down: insert blank row at scroll_top, remove from scroll_bottom.
                    let top = self.scroll_top as usize;
                    let bottom = self.scroll_bottom as usize;
                    let cols = self.cols;
                    let grid = self.active_grid();
                    if bottom < grid.len() {
                        grid.remove(bottom);
                    }
                    grid.insert(top, blank_row(cols));
                    self.dirty.mark_all();
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
            }
            // DECSC — save cursor.
            b'7' => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // DECRC — restore cursor.
            b'8' => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
                self.clamp_cursor();
            }
            // RIS — full reset.
            b'c' => {
                let blank = make_blank_grid(self.rows, self.cols);
                self.primary = blank.clone();
                self.alternate = blank;
                self.alt_screen = false;
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.current_attrs = Attrs::default();
                self.scroll_top = 0;
                self.scroll_bottom = self.rows.saturating_sub(1);
                self.dirty.mark_all();
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        self.handle_osc(params);
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

// ── SGR / DEC helpers ─────────────────────────────────────────────────────

impl DamageGrid {
    fn apply_sgr(&mut self, params: &[u16]) {
        let mut i = 0;
        if params.is_empty() {
            self.current_attrs = Attrs::default();
            return;
        }
        while i < params.len() {
            match params[i] {
                0 => {
                    self.current_attrs = Attrs::default();
                }
                1 => self.current_attrs.bold = true,
                2 => self.current_attrs.dim = true,
                3 => self.current_attrs.italic = true,
                4 => self.current_attrs.underline = true,
                7 => self.current_attrs.inverse = true,
                22 => {
                    self.current_attrs.bold = false;
                    self.current_attrs.dim = false;
                }
                23 => self.current_attrs.italic = false,
                24 => self.current_attrs.underline = false,
                27 => self.current_attrs.inverse = false,
                // Standard 16 colors — foreground.
                30..=37 => {
                    self.current_attrs.foreground = Color::Idx(params[i] as u8 - 30);
                }
                38 => {
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.current_attrs.foreground = color;
                    }
                }
                39 => self.current_attrs.foreground = Color::Default,
                // Standard 16 colors — background.
                40..=47 => {
                    self.current_attrs.background = Color::Idx(params[i] as u8 - 40);
                }
                48 => {
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.current_attrs.background = color;
                    }
                }
                49 => self.current_attrs.background = Color::Default,
                // Bright foreground (90-97).
                90..=97 => {
                    self.current_attrs.foreground = Color::Idx(params[i] as u8 - 90 + 8);
                }
                // Bright background (100-107).
                100..=107 => {
                    self.current_attrs.background = Color::Idx(params[i] as u8 - 100 + 8);
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn set_dec_mode(&mut self, mode: u16, enabled: bool) {
        match mode {
            // Show/hide cursor.
            25 => self.hide_cursor = !enabled,
            // Application/normal cursor keys.
            1 => self.application_cursor = enabled,
            // Alternate screen (simple form, no cursor save).
            47 => self.set_alt_screen(enabled),
            // Bracketed paste.
            2004 => self.bracketed_paste = enabled,
            // Alternate screen (save/restore cursor).
            // Mode 1047: switch only (no cursor save/restore).
            // Mode 1049: save cursor before entering alt screen, restore after leaving.
            1047 => self.set_alt_screen(enabled),
            1049 => {
                if enabled {
                    self.saved_cursor_row = self.cursor_row;
                    self.saved_cursor_col = self.cursor_col;
                    self.set_alt_screen(true);
                    self.cursor_row = 0;
                    self.cursor_col = 0;
                } else {
                    self.set_alt_screen(false);
                    self.cursor_row = self.saved_cursor_row;
                    self.cursor_col = self.saved_cursor_col;
                    self.clamp_cursor();
                }
            }
            // Mouse modes.
            1000 => {
                self.mouse_mode = if enabled {
                    MouseProtocolMode::Press
                } else {
                    MouseProtocolMode::None
                };
            }
            1002 => {
                self.mouse_mode = if enabled {
                    MouseProtocolMode::PressRelease
                } else {
                    MouseProtocolMode::None
                };
            }
            1003 => {
                self.mouse_mode = if enabled {
                    MouseProtocolMode::AnyEvent
                } else {
                    MouseProtocolMode::None
                };
            }
            1005 => {
                self.mouse_encoding = if enabled {
                    MouseProtocolEncoding::Utf8
                } else {
                    MouseProtocolEncoding::Default
                };
            }
            1006 => {
                self.mouse_encoding = if enabled {
                    MouseProtocolEncoding::Sgr
                } else {
                    MouseProtocolEncoding::Default
                };
            }
            1015 => {
                self.mouse_encoding = if enabled {
                    MouseProtocolEncoding::Urxvt
                } else {
                    MouseProtocolEncoding::Default
                };
            }
            // Synchronized output (?2026).
            2026 => {
                self.passthrough
                    .push(PassthroughEvent::SynchronizedOutput(enabled));
            }
            _ => {}
        }
    }

    fn set_alt_screen(&mut self, active: bool) {
        self.alt_screen = active;
        self.dirty.mark_all();
    }
}

// ── Parse helpers ─────────────────────────────────────────────────────────

/// Parse extended color from SGR params starting at `i`.
/// Advances `i` past the color params consumed. Returns `None` for unknown.
fn parse_extended_color(params: &[u16], i: &mut usize) -> Option<Color> {
    match params.get(*i + 1).copied() {
        Some(2) => {
            // 38;2;r;g;b
            let r = params.get(*i + 2).copied().unwrap_or(0) as u8;
            let g = params.get(*i + 3).copied().unwrap_or(0) as u8;
            let b = params.get(*i + 4).copied().unwrap_or(0) as u8;
            *i += 4;
            Some(Color::Rgb(r, g, b))
        }
        Some(5) => {
            // 38;5;n
            let n = params.get(*i + 2).copied().unwrap_or(0) as u8;
            *i += 2;
            Some(Color::Idx(n))
        }
        _ => None,
    }
}

// ── Grid construction helpers ─────────────────────────────────────────────

fn blank_row(cols: u16) -> Vec<Cell> {
    vec![Cell::default(); cols as usize]
}

fn make_blank_grid(rows: u16, cols: u16) -> Vec<Vec<Cell>> {
    (0..rows).map(|_| blank_row(cols)).collect()
}

fn resize_grid(grid: &[Vec<Cell>], rows: u16, cols: u16) -> Vec<Vec<Cell>> {
    let mut new = make_blank_grid(rows, cols);
    for (r, row) in grid.iter().enumerate() {
        if r >= rows as usize {
            break;
        }
        for (c, cell) in row.iter().enumerate() {
            if c < cols as usize {
                new[r][c] = cell.clone();
            }
        }
    }
    new
}
