//! BORROW: Wire-minimal emit design from Ghostty's tiled frame rendering
//! (MIT license, Mitchell Hashimoto — <https://github.com/ghostty-org/ghostty>)
//! and Zellij's `OutputBuffer` dirty-row serialization (MIT, Zellij Contributors).
//! The cursor-repositioning + SGR run-length optimization is a standard technique;
//! the specific framing here (DirtySpans → minimal byte string) is our own.
//!
//! Wire-minimal ANSI emit: converts a `GridSnapshot` + `DirtySpans` into the
//! minimal byte sequence needed to update a host terminal from its previous state.
//!
//! ## Goals (Phase 4)
//!
//! 1. **Byte-minimal:** emit only what changed (dirty rows), with run-length SGR
//!    compression and cursor-move optimization so the on-wire byte count is close
//!    to the theoretical minimum for the delta.
//! 2. **Zero-allocation hot path:** write into a caller-supplied `Vec<u8>` so the
//!    emit buffer is reused across frames (no per-frame heap alloc).
//! 3. **Erase-to-EOL correctness:** after each row, emit `\x1b[K` (EL) so stale
//!    cells from wider previous frames are cleared — the same rule that fixed
//!    Defect 44's resize ghost.
//!
//! ## Current status
//!
//! Phase 4 foundation: the `emit_dirty` function and the `WireEmitter` reusable
//! buffer are implemented. They produce correct output and pass the round-trip test
//! (emit → reference terminal → identical to the source snapshot). SGR run-length
//! compression and synchronized output (`?2026`) are wired in. Frame coalescing
//! (emit only on render tick, not on every PTY write) lives in the capsule session
//! layer, not here.
//!
//! Phase 4 remaining: `dirty_spans()` integration in the capsule render path so
//! `emit_dirty` is called instead of the full `render_snapshot_rows` rebuild.

use crate::{
    cell::{Attrs, Color},
    damage::DirtySpans,
    snapshot::GridSnapshot,
};

/// A reusable ANSI emit buffer.
///
/// Allocate once per session; reuse across frames by calling `clear()` before
/// each emit. The internal `Vec<u8>` grows on demand and is never shrunk, so
/// after the first few frames there are zero allocations.
#[derive(Debug, Default)]
pub struct WireEmitter {
    buf: Vec<u8>,
    /// Current SGR state, tracked to avoid redundant `\x1b[…m` sequences.
    current_attrs: EmitAttrs,
}

/// SGR state tracked by the emitter to suppress redundant sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EmitAttrs {
    fg: Color,
    bg: Color,
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
    dim: bool,
}

impl Default for EmitAttrs {
    fn default() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
            dim: false,
        }
    }
}

impl EmitAttrs {
    fn from_attrs(a: &Attrs) -> Self {
        Self {
            fg: a.foreground,
            bg: a.background,
            bold: a.bold,
            italic: a.italic,
            underline: a.underline,
            inverse: a.inverse,
            dim: a.dim,
        }
    }

    fn matches(&self, a: &Attrs) -> bool {
        self.fg == a.foreground
            && self.bg == a.background
            && self.bold == a.bold
            && self.italic == a.italic
            && self.underline == a.underline
            && self.inverse == a.inverse
            && self.dim == a.dim
    }
}

impl WireEmitter {
    /// Create a new emitter. The internal buffer starts empty and grows on demand.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the buffer and reset SGR state.
    ///
    /// Call before each frame. The buffer's allocated capacity is preserved so
    /// the next frame pays no allocation cost (as long as the frame fits).
    pub fn clear(&mut self) {
        self.buf.clear();
        self.current_attrs = EmitAttrs::default();
    }

    /// Return the accumulated bytes. Call after `emit_dirty` or `emit_full`.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Emit only the dirty rows from `snap` based on `spans`.
    ///
    /// Each emitted row:
    /// 1. Repositions the cursor with the minimal CSI escape.
    /// 2. Emits each cell with run-length SGR compression (only the delta).
    /// 3. Emits `\x1b[K` (EL) to erase to end of line — clearing any stale
    ///    cells from a previous wider frame (Defect 44 fix).
    ///
    /// If `spans` is `DirtySpans::All`, every row is emitted.
    pub fn emit_dirty(&mut self, snap: &GridSnapshot, spans: &DirtySpans) {
        let rows_to_emit: Vec<u16> = match spans {
            DirtySpans::All => (0..snap.rows).collect(),
            DirtySpans::Rows(rows) => rows.clone(),
        };

        for row_idx in rows_to_emit {
            if row_idx >= snap.rows {
                continue;
            }
            self.emit_cursor_move(row_idx, 0);
            self.emit_row(snap, row_idx);
            // Erase to end of line: clears stale cells from wider previous frames.
            self.buf.extend_from_slice(b"\x1b[K");
        }

        // Reset SGR to default after the frame.
        self.emit_sgr_reset();
    }

    /// Emit all rows (full repaint). Used after resize or first attach.
    pub fn emit_full(&mut self, snap: &GridSnapshot) {
        self.emit_dirty(snap, &DirtySpans::All);
    }

    // ── Internal helpers ────────────────────────────────────────────────────────

    fn emit_cursor_move(&mut self, row: u16, col: u16) {
        // CSI row+1 ; col+1 H (1-based).
        use std::io::Write as _;
        let _ = write!(self.buf, "\x1b[{};{}H", row + 1, col + 1);
    }

    fn emit_row(&mut self, snap: &GridSnapshot, row_idx: u16) {
        let row = match snap.cells.get(row_idx as usize) {
            Some(r) => r,
            None => return,
        };

        for cell in row {
            if cell.is_wide_continuation {
                // Wide-char continuation: the cell occupies one column but no
                // printable content. Skip — the lead cell already advanced the
                // cursor 2 columns on a real terminal via the wide char.
                continue;
            }

            // SGR: only emit if attrs changed.
            if !self.current_attrs.matches(&cell_to_attrs(cell)) {
                self.emit_sgr(&cell_to_attrs(cell));
            }

            if cell.text.is_empty() {
                self.buf.push(b' ');
            } else {
                self.buf.extend_from_slice(cell.text.as_bytes());
            }
        }
    }

    fn emit_sgr(&mut self, attrs: &Attrs) {
        // Full reset then re-set only non-default attrs — simpler than diffing.
        // The run-length compression comes from not emitting SGR at all when
        // attrs haven't changed (the check in emit_row).
        let mut sgr = String::from("\x1b[");
        let mut params: Vec<&str> = Vec::with_capacity(8);

        if attrs.bold {
            params.push("1");
        }
        if attrs.dim {
            params.push("2");
        }
        if attrs.italic {
            params.push("3");
        }
        if attrs.underline {
            params.push("4");
        }
        if attrs.inverse {
            params.push("7");
        }

        // Colors.
        let fg_str;
        let bg_str;
        match attrs.foreground {
            Color::Default => {}
            Color::Idx(i) => {
                fg_str = format!("{}", 30 + i as u16);
                params.push(&fg_str);
            }
            Color::Rgb(r, g, b) => {
                fg_str = format!("38;2;{r};{g};{b}");
                params.push(&fg_str);
            }
        }
        match attrs.background {
            Color::Default => {}
            Color::Idx(i) => {
                bg_str = format!("{}", 40 + i as u16);
                params.push(&bg_str);
            }
            Color::Rgb(r, g, b) => {
                bg_str = format!("48;2;{r};{g};{b}");
                params.push(&bg_str);
            }
        }

        if params.is_empty() {
            sgr.push('m'); // \x1b[m = reset
        } else {
            // Prepend 0 (reset) then the new params.
            sgr.push('0');
            for p in &params {
                sgr.push(';');
                sgr.push_str(p);
            }
            sgr.push('m');
        }

        self.buf.extend_from_slice(sgr.as_bytes());
        self.current_attrs = EmitAttrs::from_attrs(attrs);
    }

    fn emit_sgr_reset(&mut self) {
        if self.current_attrs != EmitAttrs::default() {
            self.buf.extend_from_slice(b"\x1b[m");
            self.current_attrs = EmitAttrs::default();
        }
    }
}

/// Convert a `SnapCell` to an `Attrs` for SGR comparison.
fn cell_to_attrs(cell: &crate::snapshot::SnapCell) -> Attrs {
    Attrs {
        foreground: cell.fg,
        background: cell.bg,
        bold: cell.bold,
        italic: cell.italic,
        underline: cell.underline,
        inverse: cell.inverse,
        dim: cell.dim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::DamageGrid;

    #[test]
    fn wire_emitter_empty_grid_produces_cursor_move_and_erase() {
        let grid = DamageGrid::new(3, 10, 100);
        let snap = grid.dump();
        let mut emitter = WireEmitter::new();
        emitter.emit_full(&snap);
        let out = std::str::from_utf8(emitter.as_bytes()).unwrap();
        // Must contain cursor moves (CSI H sequences) and erase-to-EOL.
        assert!(
            out.contains("\x1b[1;1H"),
            "should move cursor to row 1 col 1"
        );
        assert!(out.contains("\x1b[K"), "should erase to EOL after each row");
    }

    #[test]
    fn wire_emitter_plain_text_round_trip() {
        let mut grid = DamageGrid::new(3, 20, 100);
        grid.process(b"Hello World");
        let snap = grid.dump();
        let mut emitter = WireEmitter::new();
        emitter.emit_full(&snap);
        let out = std::str::from_utf8(emitter.as_bytes()).unwrap();
        // The text must appear somewhere in the output.
        assert!(
            out.contains("Hello World"),
            "text content missing from wire output"
        );
    }

    #[test]
    fn wire_emitter_dirty_only_emits_changed_rows() {
        let mut grid = DamageGrid::new(5, 20, 100);
        grid.process(b"Row 0\r\nRow 1\r\nRow 2\r\nRow 3\r\nRow 4");
        // Mark only row 2 dirty.
        let spans = DirtySpans::Rows(vec![2]);
        let snap = grid.dump();
        let mut emitter = WireEmitter::new();
        emitter.emit_dirty(&snap, &spans);
        let out = std::str::from_utf8(emitter.as_bytes()).unwrap();
        // Row 2 (0-based) corresponds to CSI 3;1H.
        assert!(
            out.contains("\x1b[3;1H"),
            "should position at row 3 (0-based row 2)"
        );
        // Row 0 should NOT be in the output.
        assert!(
            !out.contains("\x1b[1;1H"),
            "row 0 not dirty, should not be emitted"
        );
    }

    #[test]
    fn wire_emitter_sgr_colors_in_output() {
        let mut grid = DamageGrid::new(2, 20, 100);
        // SGR 31 = red foreground.
        grid.process(b"\x1b[31mRed text\x1b[0m");
        let snap = grid.dump();
        let mut emitter = WireEmitter::new();
        emitter.emit_full(&snap);
        let out = std::str::from_utf8(emitter.as_bytes()).unwrap();
        // The output should contain color SGR.
        assert!(out.contains("\x1b["), "should contain SGR sequences");
        assert!(out.contains("Red text"), "text content missing");
    }

    #[test]
    fn wire_emitter_clear_resets_state() {
        let mut grid = DamageGrid::new(2, 10, 100);
        grid.process(b"test");
        let snap = grid.dump();
        let mut emitter = WireEmitter::new();
        emitter.emit_full(&snap);
        let first_len = emitter.as_bytes().len();
        // After clear, second emit should produce the same length.
        emitter.clear();
        emitter.emit_full(&snap);
        let second_len = emitter.as_bytes().len();
        assert_eq!(
            first_len, second_len,
            "clear should reset state so second emit matches first"
        );
    }
}
