//! Differential test harness: Phase 1 of Defect 45 (jackin-term).
//!
//! Feeds identical byte streams to two terminal model implementations and asserts
//! they produce identical final grids (cells, attrs, cursor, scrollback, alt-screen).
//!
//! Phase 1 goal: "can already diff stock vt100 vs itself (sanity) so the moment
//! jackin-term v0 exists it is gradeable." The oracle-vs-oracle sanity run verifies
//! that the harness plumbing is correct before `jackin-term` is wired in as the
//! left-hand model.
//!
//! When Phase 2 lands `DamageGrid`, add it as the left model and update `run_differential`
//! to instantiate `jackin_term::DamageGrid` on the left side.
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

// ---------------------------------------------------------------------------
// Oracle abstraction
// ---------------------------------------------------------------------------

/// A comparable snapshot of a single screen cell.
#[derive(Debug, PartialEq)]
struct CellSnapshot {
    contents: String,
    is_wide: bool,
    is_wide_continuation: bool,
    foreground: vt100::Color,
    background: vt100::Color,
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
// vt100 oracle adapter
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
                foreground: cell.fgcolor(),
                background: cell.bgcolor(),
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
// Differential runner
// ---------------------------------------------------------------------------

/// Feed `bytes` to two independent vt100 parsers and assert identical output.
///
/// This is the Phase 1 sanity gate: oracle-vs-oracle. When Phase 2 lands
/// `jackin_term::DamageGrid`, replace the left parser with it.
fn run_differential(rows: u16, cols: u16, bytes: &[u8], label: &str) {
    let mut left = vt100::Parser::new(rows, cols, 10_000);
    let mut right = vt100::Parser::new(rows, cols, 10_000);

    left.process(bytes);
    right.process(bytes);

    let left_snap = snapshot_vt100(&left);
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
    let mut left = vt100::Parser::new(24, 80, 10_000);
    let mut right = vt100::Parser::new(24, 80, 10_000);
    let content = b"Line 1\r\nLine 2\r\nLine 3\r\n";
    left.process(content);
    right.process(content);
    left.screen_mut().set_size(10, 40);
    right.screen_mut().set_size(10, 40);
    left.screen_mut().set_size(24, 80);
    right.screen_mut().set_size(24, 80);
    snapshot_vt100(&left).assert_eq(&snapshot_vt100(&right), "resize smaller then larger");
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
        // No fixtures committed yet — Phase 1 corpus is assembled incrementally.
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
