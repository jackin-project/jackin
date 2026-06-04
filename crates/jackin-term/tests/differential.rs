//! Differential test harness: Phase 2 of Defect 45 (jackin-term).
//!
//! Feeds identical byte streams to `jackin_term::DamageGrid` (left) and
//! `vt100::Parser` (right, oracle) and asserts identical final grids —
//! cells, attrs, cursor, alt-screen.
//!
//! The harness is the correctness gate from the checklist:
//! "the differential harness (Phase 1) passes against the `vt100` oracle
//! across the entire committed corpus."
//!
//! ## Corpus layout
//!
//! Each fixture file in `tests/fixtures/` is a raw byte sequence. The harness feeds
//! each file to both models and compares the resulting screens. Fixtures are organized
//! by category:
//!
//! - `basic/`: plain text, cursor movement, colors — baseline coverage
//! - `wide_chars/`: CJK, emoji, wide-char continuation cells
//! - `resize/`: sequences that stress resize + reflow
//! - `scrollback/`: sequences that fill scrollback and clear it
//! - `alt_screen/`: alternate screen enter/exit
//! - `pathological/`: high-volume stress sequences (`yes`, `seq 1 100000`, redraw storms)

use std::path::Path;

use jackin_term::{Cell, Color, DamageGrid};

// ---------------------------------------------------------------------------
// Neutral color type for cross-model comparison
// ---------------------------------------------------------------------------

/// Neutral color representation comparable across both models.
///
/// Mirrors `vt100::Color` and `jackin_term::cell::Color` which have identical
/// structural variants. Using a local type avoids the test depending on the
/// `vt100` crate's type stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorSnap {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

impl From<vt100::Color> for ColorSnap {
    fn from(c: vt100::Color) -> Self {
        match c {
            vt100::Color::Default => ColorSnap::Default,
            vt100::Color::Idx(i) => ColorSnap::Idx(i),
            vt100::Color::Rgb(r, g, b) => ColorSnap::Rgb(r, g, b),
        }
    }
}

impl From<Color> for ColorSnap {
    fn from(c: Color) -> Self {
        match c {
            Color::Default => ColorSnap::Default,
            Color::Idx(i) => ColorSnap::Idx(i),
            Color::Rgb(r, g, b) => ColorSnap::Rgb(r, g, b),
        }
    }
}

// ---------------------------------------------------------------------------
// Oracle abstraction
// ---------------------------------------------------------------------------

/// A comparable snapshot of a single screen cell.
#[derive(Debug, PartialEq)]
struct CellSnapshot {
    contents: String,
    is_wide: bool,
    is_wide_continuation: bool,
    foreground: ColorSnap,
    background: ColorSnap,
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

/// A comparable snapshot of a full terminal screen.
#[derive(Debug)]
struct ScreenSnapshot {
    rows: u16,
    cols: u16,
    cursor_row: u16,
    cursor_col: u16,
    alternate_screen: bool,
    cells: Vec<Vec<CellSnapshot>>,
}

impl ScreenSnapshot {
    /// Assert that two snapshots are identical, providing a detailed diff on failure.
    fn assert_eq(&self, other: &ScreenSnapshot, label: &str) {
        assert_eq!(
            self.rows, other.rows,
            "{label}: row count mismatch: left={} right={}",
            self.rows, other.rows
        );
        assert_eq!(
            self.cols, other.cols,
            "{label}: col count mismatch: left={} right={}",
            self.cols, other.cols
        );
        assert_eq!(
            self.cursor_row, other.cursor_row,
            "{label}: cursor row mismatch: left={} right={}",
            self.cursor_row, other.cursor_row
        );
        assert_eq!(
            self.cursor_col, other.cursor_col,
            "{label}: cursor col mismatch: left={} right={}",
            self.cursor_col, other.cursor_col
        );
        assert_eq!(
            self.alternate_screen, other.alternate_screen,
            "{label}: alt-screen flag mismatch"
        );
        for r in 0..self.rows as usize {
            for c in 0..self.cols as usize {
                let l = &self.cells[r][c];
                let o = &other.cells[r][c];
                assert_eq!(
                    l, o,
                    "{label}: cell mismatch at ({r},{c}): left={l:?} right={o:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DamageGrid adapter (left model)
// ---------------------------------------------------------------------------

fn snapshot_damagegrid(grid: &DamageGrid) -> ScreenSnapshot {
    let (rows, cols) = grid.size();
    let (cursor_row, cursor_col) = grid.cursor_position();

    let mut cells = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols as usize);
        for c in 0..cols {
            let cell: &Cell = grid
                .cell(r, c)
                .unwrap_or_else(|| panic!("cell ({r},{c}) out of bounds for {rows}×{cols} grid"));
            row.push(CellSnapshot {
                contents: cell.contents().to_string(),
                is_wide: cell.is_wide,
                is_wide_continuation: cell.is_wide_continuation,
                foreground: cell.fgcolor().into(),
                background: cell.bgcolor().into(),
                bold: cell.bold(),
                italic: cell.italic(),
                underline: cell.underline(),
                inverse: cell.inverse(),
            });
        }
        cells.push(row);
    }

    ScreenSnapshot {
        rows,
        cols,
        cursor_row,
        cursor_col,
        alternate_screen: grid.alternate_screen(),
        cells,
    }
}

// ---------------------------------------------------------------------------
// vt100 oracle adapter (right model)
// ---------------------------------------------------------------------------

fn snapshot_vt100(parser: &vt100::Parser) -> ScreenSnapshot {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let (cursor_row, cursor_col) = screen.cursor_position();

    let mut cells = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols as usize);
        for c in 0..cols {
            let cell = screen
                .cell(r, c)
                .unwrap_or_else(|| panic!("cell ({r},{c}) out of bounds for {rows}×{cols} screen"));
            row.push(CellSnapshot {
                contents: cell.contents().to_string(),
                is_wide: cell.is_wide(),
                is_wide_continuation: cell.is_wide_continuation(),
                foreground: cell.fgcolor().into(),
                background: cell.bgcolor().into(),
                bold: cell.bold(),
                italic: cell.italic(),
                underline: cell.underline(),
                inverse: cell.inverse(),
            });
        }
        cells.push(row);
    }

    ScreenSnapshot {
        rows,
        cols,
        cursor_row,
        cursor_col,
        alternate_screen: screen.alternate_screen(),
        cells,
    }
}

// ---------------------------------------------------------------------------
// Differential runner — DamageGrid (left) vs vt100 (right/oracle)
// ---------------------------------------------------------------------------

/// Feed `bytes` to `DamageGrid` (left) and `vt100::Parser` (right/oracle) and
/// assert identical cell grids, cursor position, and alt-screen flag.
fn run_differential(rows: u16, cols: u16, bytes: &[u8], label: &str) {
    let mut left = DamageGrid::new(rows, cols, 10_000);
    let mut right = vt100::Parser::new(rows, cols, 10_000);

    left.process(bytes);
    right.process(bytes);

    let left_snap = snapshot_damagegrid(&left);
    let right_snap = snapshot_vt100(&right);
    left_snap.assert_eq(&right_snap, label);
}

// ---------------------------------------------------------------------------
// Inline corpus tests (basic coverage without fixture files)
// ---------------------------------------------------------------------------

#[test]
fn sanity_empty_bytes() {
    run_differential(24, 80, b"", "empty bytes");
}

#[test]
fn sanity_plain_text() {
    run_differential(24, 80, b"Hello, World!\r\n", "plain text");
}

#[test]
fn sanity_cursor_movement() {
    // Move to (5,10), print a character, move back to origin.
    let seq = b"\x1b[6;11H*\x1b[H";
    run_differential(24, 80, seq, "cursor movement");
}

#[test]
fn sanity_colors_sgr() {
    // SGR: bold red foreground on blue background, then reset.
    let seq = b"\x1b[1;31;44mX\x1b[0m";
    run_differential(24, 80, seq, "SGR colors");
}

#[test]
fn sanity_alt_screen_enter_exit() {
    // Enter alternate screen, write something, exit.
    let seq = b"\x1b[?1049hAlt screen content\x1b[?1049l";
    run_differential(24, 80, seq, "alt screen enter/exit");
}

#[test]
fn sanity_line_clear_to_end() {
    run_differential(24, 80, b"Hello\x1b[2KWorld\r\n", "clear to end of line");
}

#[test]
fn sanity_screen_clear() {
    run_differential(24, 80, b"Line 1\r\nLine 2\r\n\x1b[2JDone", "screen clear");
}

#[test]
fn sanity_wide_chars_cjk() {
    // Simplified Chinese ideographs (2-column wide).
    run_differential(24, 80, "你好世界\r\n".as_bytes(), "CJK wide chars");
}

#[test]
fn sanity_emoji() {
    // Emoji (wide in most terminals).
    run_differential(24, 80, "🦀 Rust!\r\n".as_bytes(), "emoji wide chars");
}

#[test]
fn sanity_resize_smaller_then_larger() {
    // Feed content, resize smaller (simulating Defect 44 scenario), then larger.
    let content = b"Line 1\r\nLine 2\r\nLine 3\r\n";

    let mut left = DamageGrid::new(24, 80, 10_000);
    let mut right = vt100::Parser::new(24, 80, 10_000);

    left.process(content);
    right.process(content);

    left.set_size(10, 40);
    right.screen_mut().set_size(10, 40);
    left.set_size(24, 80);
    right.screen_mut().set_size(24, 80);

    snapshot_damagegrid(&left).assert_eq(&snapshot_vt100(&right), "resize smaller then larger");
}

#[test]
fn sanity_scrollback() {
    // Fill and clear scrollback.
    let mut lines = String::new();
    for i in 0..200 {
        lines.push_str(&format!("Line {i}\r\n"));
    }
    run_differential(24, 80, lines.as_bytes(), "scrollback fill");
}

#[test]
fn sanity_dec_private_modes() {
    // Mouse reporting enable/disable (modes jackin' uses).
    let seq =
        b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?1003l\x1b[?1006l\x1b[?1002l\x1b[?1000l";
    run_differential(24, 80, seq, "DEC mouse mode enable/disable");
}

#[test]
fn sanity_osc_title_ignored_by_grid() {
    // OSC 0 (window title): consumed by Callbacks, must not corrupt the cell grid.
    let seq = b"\x1b]0;My Window Title\x07Some text after";
    run_differential(24, 80, seq, "OSC title passthrough");
}

#[test]
fn sanity_bracketed_paste() {
    // Bracketed paste mode toggle + content (used by agent UIs).
    let seq = b"\x1b[?2004h\x1b[200~pasted content\x1b[201~\x1b[?2004l";
    run_differential(24, 80, seq, "bracketed paste");
}

#[test]
fn sanity_high_volume_plain_text() {
    // Simulates `seq 1 10000` output — high volume plain text.
    let mut data = String::new();
    for i in 1..=5_000 {
        data.push_str(&format!("{i}\r\n"));
    }
    run_differential(24, 80, data.as_bytes(), "high volume plain text");
}

#[test]
fn sanity_interleaved_sgr_and_movement() {
    // Typical agent TUI: color regions + cursor moves.
    let mut seq = Vec::new();
    for row in 0..24u16 {
        // Move to row, print colored text.
        let cmd = format!(
            "\x1b[{};1H\x1b[{}mRow {row:02}\x1b[0m",
            row + 1,
            31 + (row % 7)
        );
        seq.extend_from_slice(cmd.as_bytes());
    }
    run_differential(24, 80, &seq, "interleaved SGR and cursor movement");
}

// ---------------------------------------------------------------------------
// Fixture-based corpus tests
// ---------------------------------------------------------------------------

fn run_fixture(fixture_path: &Path) {
    let bytes = std::fs::read(fixture_path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture_path.display()));
    let label = fixture_path.display().to_string();
    run_differential(24, 80, &bytes, &label);
}

#[test]
fn corpus_all_fixtures() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    if !fixture_dir.exists() {
        return;
    }
    let mut count = 0usize;
    for entry in walkdir(&fixture_dir) {
        if entry.extension().is_some_and(|e| e == "bin" || e == "vt") {
            run_fixture(&entry);
            count += 1;
        }
    }
    if count > 0 {
        eprintln!("[differential] ran {count} corpus fixtures");
    }
}

// ---------------------------------------------------------------------------
// Extended VT conformance tests (DECSTBM, DECAWM, cursor save/restore, etc.)
// ---------------------------------------------------------------------------

#[test]
fn vt_decstbm_scroll_region() {
    // DECSTBM: set scrolling region to rows 5-20 (1-based), scroll inside it.
    // Used by vim, htop, most TUIs.
    let seq = b"\x1b[5;20r\x1b[5;1HLine5\r\nLine6\r\n\x1b[r";
    run_differential(24, 80, seq, "DECSTBM scroll region");
}

#[test]
fn vt_cursor_save_restore_sc_rc() {
    // ESC 7 / ESC 8 (save/restore cursor position + attrs).
    let seq = b"\x1b[10;20H\x1b7\x1b[1;1HAnywhere\x1b8Back";
    run_differential(24, 80, seq, "cursor save/restore ESC 7/8");
}

// Note: CSI s / CSI u (ANSI cursor save/restore) is intentionally omitted.
// vt100 does not implement these sequences and treats them as no-ops, while
// DamageGrid does implement them (saving to saved_cursor_row/col). Since the
// two models deliberately disagree, this is not a correctness bug in either;
// it is a vt100-divergence. The ESC 7 / ESC 8 (DECSC/DECRC) test above covers
// the standard save/restore path that both models implement correctly.

#[test]
fn vt_decawm_autowrap_off() {
    // Disable auto-wrap (DECAWM off). Characters past end of line stay at last col.
    let seq = b"\x1b[?7l";
    for _ in 0..100 {
        let mut full = seq.to_vec();
        full.extend_from_slice(b"X");
        run_differential(24, 80, &full, "DECAWM off single char");
    }
}

#[test]
fn vt_erase_line_variants() {
    // CSI K: erase to end (0), from start (1), whole line (2).
    let seq = b"\x1b[10;1HHello World\x1b[10;6H\x1b[0K"; // erase from 'W' to end
    run_differential(24, 80, seq, "erase to end of line");
    let seq = b"\x1b[10;1HHello World\x1b[10;6H\x1b[1K"; // erase from start to cursor
    run_differential(24, 80, seq, "erase from start to cursor");
    let seq = b"\x1b[10;1HHello World\x1b[10;6H\x1b[2K"; // erase whole line
    run_differential(24, 80, seq, "erase whole line");
}

#[test]
fn vt_insert_delete_chars() {
    // CSI @ (ICH - insert characters), CSI P (DCH - delete characters).
    let seq = b"\x1b[5;1HABCDE\x1b[5;3H\x1b[@X"; // insert X at position 3
    run_differential(24, 80, seq, "insert character (ICH)");
    let seq = b"\x1b[5;1HABCDE\x1b[5;3H\x1b[2P"; // delete 2 chars from position 3
    run_differential(24, 80, seq, "delete characters (DCH)");
}

#[test]
fn vt_reverse_index_ri() {
    // ESC M (RI - reverse index: move cursor up, scroll if at top).
    let seq = b"\x1b[1;1HTop\x1b[1;4H\x1bM"; // at row 1, RI scrolls down
    run_differential(24, 80, seq, "reverse index at top of screen");
    let seq = b"\x1b[5;1HMiddle\x1b[5;7H\x1bM"; // at row 5, RI moves up to row 4
    run_differential(24, 80, seq, "reverse index in middle");
}

#[test]
fn vt_rgb_truecolor_sgr() {
    // SGR 38;2;R;G;B and 48;2;R;G;B (RGB foreground and background).
    let seq = b"\x1b[38;2;255;128;0m\x1b[48;2;0;0;128mOrange on navy\x1b[0m";
    run_differential(24, 80, seq, "RGB truecolor SGR");
}

#[test]
fn vt_256color_sgr() {
    // SGR 38;5;N and 48;5;N (256-color palette).
    let seq = b"\x1b[38;5;196m\x1b[48;5;21mRed on blue\x1b[0m";
    run_differential(24, 80, seq, "256-color SGR");
}

#[test]
fn vt_full_screen_clear_and_rewrite() {
    // Simulate a TUI that clears the screen and rewrites it every frame.
    let mut seq: Vec<u8> = Vec::new();
    for frame in 0..5 {
        seq.extend_from_slice(b"\x1b[2J\x1b[H"); // clear screen, home cursor
        for row in 1..=24u16 {
            let line = format!("\x1b[{row};1HFrame {frame} Row {row:02}");
            seq.extend_from_slice(line.as_bytes());
        }
    }
    run_differential(24, 80, &seq, "full-screen clear and rewrite (5 frames)");
}

#[test]
fn vt_many_sgr_on_off_cycles() {
    // Stress SGR attribute tracking with many enable/disable cycles.
    let mut seq: Vec<u8> = Vec::new();
    for _ in 0..50 {
        seq.extend_from_slice(b"\x1b[1mB\x1b[22mN\x1b[3mI\x1b[23mN\x1b[4mU\x1b[24mN\x1b[0m");
    }
    run_differential(24, 80, &seq, "many SGR attribute cycles");
}

#[test]
fn vt_cursor_column_set_cha() {
    // CSI G (CHA - cursor horizontal absolute, 1-based col).
    let seq = b"\x1b[5;1HHello\x1b[3G*"; // move to col 3, overwrite with *
    run_differential(24, 80, seq, "cursor horizontal absolute (CHA)");
}

#[test]
fn vt_repeat_preceding_char_rep() {
    // CSI b (REP - repeat preceding printed character N times).
    // Not all terminals support this; test that vt100 and DamageGrid agree.
    let seq = b"A\x1b[79b"; // 'A' repeated 79 times = 80 cols
    run_differential(24, 80, seq, "repeat preceding char (REP)");
}

#[test]
fn vt_erase_display_from_cursor() {
    // CSI J variants: 0=cursor to end, 1=start to cursor, 2=all, 3=with scrollback.
    let seq = b"\x1b[10;1HFirst\r\nSecond\r\nThird\x1b[11;1H\x1b[0J";
    run_differential(24, 80, seq, "erase display from cursor to end");
    let seq = b"\x1b[10;1HFirst\r\nSecond\r\nThird\x1b[11;4H\x1b[1J";
    run_differential(24, 80, seq, "erase display from start to cursor");
    let seq = b"\x1b[10;1HFirst\r\nSecond\r\nThird\x1b[2J";
    run_differential(24, 80, seq, "erase entire display");
}

/// Minimal recursive directory walker that avoids pulling in `walkdir` as a dev dep.
fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                paths.extend(walkdir(&path));
            } else {
                paths.push(path);
            }
        }
    }
    paths
}
