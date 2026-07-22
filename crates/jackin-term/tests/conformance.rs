// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Conformance replay harness for jackin-term.
//!
//! Feeds committed byte streams to `jackin_term::DamageGrid` and asserts the
//! owned parser/grid stay panic-free, geometrically valid, and deterministic
//! across one-shot vs byte-split processing.
//!
//! ## Corpus layout
//!
//! Each fixture file in `tests/fixtures/` is a raw byte sequence. The harness feeds
//! each file through one-shot and byte-split processing and compares the resulting screens. Fixtures are organized
//! by category:
//!
//! - `basic/`: plain text, cursor movement, colors — baseline coverage
//! - `wide_chars/`: CJK, emoji, wide-char continuation cells
//! - `resize/`: sequences that stress resize + reflow
//! - `scrollback/`: sequences that fill scrollback and clear it
//! - `alt_screen/`: alternate screen enter/exit
//! - `pathological/`: high-volume stress sequences (`yes`, `seq 1 100000`, redraw storms)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jackin_term::{Cell, Color, DamageGrid};

// ---------------------------------------------------------------------------
// Neutral color type for snapshots
// ---------------------------------------------------------------------------

/// Neutral color representation for owned grid snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorSnap {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
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

/// SGR text-attribute flags captured by the conformance snapshot.
/// Bundled so `CellSnapshot` keeps the `struct_excessive_bools` clippy gate
/// quiet while the assertion side reads the whole struct via `PartialEq`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal SGR bits (bold / italic / underline / inverse) — \
              the standard CSI SGR attribute set is intrinsically a 4-bit mask and \
              named-field construction reads better than bit-position lookups in a \
              conformance harness."
)]
#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
struct CellAttributes {
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

/// A comparable snapshot of a single screen cell.
#[derive(Debug, PartialEq)]
struct CellSnapshot {
    contents: String,
    is_wide: bool,
    is_wide_continuation: bool,
    foreground: ColorSnap,
    background: ColorSnap,
    attributes: CellAttributes,
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
// DamageGrid adapter (owned model)
// ---------------------------------------------------------------------------

#[expect(
    clippy::panic,
    reason = "conformance adapter must fail tests with the exact out-of-bounds grid cell"
)]
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
                contents: cell.contents().to_owned(),
                is_wide: cell.is_wide,
                is_wide_continuation: cell.is_wide_continuation,
                foreground: cell.fgcolor().into(),
                background: cell.bgcolor().into(),
                attributes: CellAttributes {
                    bold: cell.bold(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    inverse: cell.inverse(),
                },
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
// Replay runner
// ---------------------------------------------------------------------------

/// Feed `bytes` to `DamageGrid` in one chunk and byte-by-byte. The final
/// snapshots must match, proving parser carry state is deterministic across PTY
/// read boundaries.
fn run_conformance(rows: u16, cols: u16, bytes: &[u8], label: &str) {
    let mut one_shot = DamageGrid::new(rows, cols, 10_000);
    let mut split = DamageGrid::new(rows, cols, 10_000);

    one_shot.process(bytes);
    for byte in bytes {
        split.process(std::slice::from_ref(byte));
    }

    let one_shot_snap = snapshot_damagegrid(&one_shot);
    let split_snap = snapshot_damagegrid(&split);
    assert_snapshot_invariants(&one_shot_snap, label);
    one_shot_snap.assert_eq(&split_snap, label);
}

fn assert_snapshot_invariants(snapshot: &ScreenSnapshot, label: &str) {
    assert!(
        snapshot.cursor_row < snapshot.rows,
        "{label}: cursor row out of bounds"
    );
    assert!(
        snapshot.cursor_col <= snapshot.cols,
        "{label}: cursor col out of bounds"
    );
    assert_eq!(
        snapshot.cells.len(),
        snapshot.rows as usize,
        "{label}: row count does not match cells"
    );
    for (idx, row) in snapshot.cells.iter().enumerate() {
        assert_eq!(
            row.len(),
            snapshot.cols as usize,
            "{label}: row {idx} col count mismatch"
        );
    }
}

// ---------------------------------------------------------------------------
// Inline corpus tests (basic coverage without fixture files)
// ---------------------------------------------------------------------------

#[test]
fn claude_welcome_live_conformance() {
    // Real Claude Code v2 welcome render captured from a live 159x44 session
    // (pane body 39 rows × 157 cols). Claude draws box borders that hit the
    // last column on most lines and paints the welcome on a coloured field —
    // it caught both the missing DECAWM deferred wrap (cursor drifted one row
    // per border line) and the missing back-colour-erase (cleared regions lost
    // the active background). Regression guard for both.
    // Kept under tests/data/ (not tests/fixtures/) so the 24x80 corpus walker
    // does not replay this 157-col-specific capture at the wrong geometry.
    let bytes = std::fs::read("tests/data/claude_welcome_live.bin").expect("fixture");
    run_conformance(39, 157, &bytes, "claude welcome live");
}

#[test]
fn sanity_empty_bytes() {
    run_conformance(24, 80, b"", "empty bytes");
}

#[test]
fn sanity_plain_text() {
    run_conformance(24, 80, b"Hello, World!\r\n", "plain text");
}

#[test]
fn sanity_cursor_movement() {
    // Move to (5,10), print a character, move back to origin.
    let seq = b"\x1b[6;11H*\x1b[H";
    run_conformance(24, 80, seq, "cursor movement");
}

#[test]
fn sanity_colors_sgr() {
    // SGR: bold red foreground on blue background, then reset.
    let seq = b"\x1b[1;31;44mX\x1b[0m";
    run_conformance(24, 80, seq, "SGR colors");
}

#[test]
fn sanity_alt_screen_enter_exit() {
    // Enter alternate screen, write something, exit.
    let seq = b"\x1b[?1049hAlt screen content\x1b[?1049l";
    run_conformance(24, 80, seq, "alt screen enter/exit");
}

#[test]
fn sanity_line_clear_to_end() {
    run_conformance(24, 80, b"Hello\x1b[2KWorld\r\n", "clear to end of line");
}

#[test]
fn sanity_screen_clear() {
    run_conformance(24, 80, b"Line 1\r\nLine 2\r\n\x1b[2JDone", "screen clear");
}

#[test]
fn sanity_wide_chars_cjk() {
    // Simplified Chinese ideographs (2-column wide).
    run_conformance(24, 80, "你好世界\r\n".as_bytes(), "CJK wide chars");
}

#[test]
fn sanity_emoji() {
    // Emoji (wide in most terminals).
    run_conformance(24, 80, "🦀 Rust!\r\n".as_bytes(), "emoji wide chars");
}

#[test]
fn sanity_resize_smaller_then_larger() {
    // Feed content, resize smaller (simulating Defect 44 scenario), then larger.
    let content = b"Line 1\r\nLine 2\r\nLine 3\r\n";

    let mut left = DamageGrid::new(24, 80, 10_000);

    left.process(content);

    left.set_size(10, 40);
    assert_snapshot_invariants(&snapshot_damagegrid(&left), "resize smaller");
    left.set_size(24, 80);

    let snap = snapshot_damagegrid(&left);
    assert_snapshot_invariants(&snap, "resize smaller then larger");
    assert_eq!(snap.rows, 24);
    assert_eq!(snap.cols, 80);
}

#[test]
fn sanity_scrollback() {
    // Fill and clear scrollback.
    let mut lines = String::new();
    for i in 0..200 {
        lines.push_str(&format!("Line {i}\r\n"));
    }
    run_conformance(24, 80, lines.as_bytes(), "scrollback fill");
}

#[test]
fn sanity_dec_private_modes() {
    // Mouse reporting enable/disable (modes jackin❯ uses).
    let seq =
        b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?1003l\x1b[?1006l\x1b[?1002l\x1b[?1000l";
    run_conformance(24, 80, seq, "DEC mouse mode enable/disable");
}

#[test]
fn sanity_osc_title_ignored_by_grid() {
    // OSC 0 (window title): consumed by Callbacks, must not corrupt the cell grid.
    let seq = b"\x1b]0;My Window Title\x07Some text after";
    run_conformance(24, 80, seq, "OSC title passthrough");
}

#[test]
fn sanity_bracketed_paste() {
    // Bracketed paste mode toggle + content (used by agent UIs).
    let seq = b"\x1b[?2004h\x1b[200~pasted content\x1b[201~\x1b[?2004l";
    run_conformance(24, 80, seq, "bracketed paste");
}

#[test]
fn sanity_high_volume_plain_text() {
    // Simulates `seq 1 10000` output — high volume plain text.
    let mut data = String::new();
    for i in 1..=5_000 {
        data.push_str(&format!("{i}\r\n"));
    }
    run_conformance(24, 80, data.as_bytes(), "high volume plain text");
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
    run_conformance(24, 80, &seq, "interleaved SGR and cursor movement");
}

// ---------------------------------------------------------------------------
// Fixture-based corpus tests
// ---------------------------------------------------------------------------

#[expect(
    clippy::panic,
    reason = "fixture corpus decoder must fail tests with the exact malformed fixture path"
)]
fn decode_vt_fixture(contents: &str, fixture_path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut chars = contents.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'x') {
            let _ = chars.next();
            let hi = chars
                .next()
                .unwrap_or_else(|| panic!("{} has incomplete \\x escape", fixture_path.display()));
            let lo = chars
                .next()
                .unwrap_or_else(|| panic!("{} has incomplete \\x escape", fixture_path.display()));
            let hex = format!("{hi}{lo}");
            let byte = u8::from_str_radix(&hex, 16).unwrap_or_else(|e| {
                panic!(
                    "{} has invalid \\x{hex} escape: {e}",
                    fixture_path.display()
                )
            });
            bytes.push(byte);
        } else {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    bytes
}

#[expect(
    clippy::panic,
    reason = "fixture corpus reader must fail tests with the exact unreadable fixture path"
)]
fn fixture_bytes(fixture_path: &Path) -> Vec<u8> {
    let raw = std::fs::read(fixture_path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fixture_path.display()));
    if fixture_path.extension().is_some_and(|e| e == "vt") {
        let text = std::str::from_utf8(&raw)
            .unwrap_or_else(|e| panic!("{} is not valid UTF-8: {e}", fixture_path.display()));
        decode_vt_fixture(text, fixture_path)
    } else if fixture_path.extension().is_some_and(|e| e == "cast") {
        cast_output_bytes(&raw, fixture_path)
    } else {
        raw
    }
}

#[expect(
    clippy::panic,
    reason = "asciinema corpus decoder must fail tests with the exact malformed fixture path"
)]
fn cast_output_bytes(raw: &[u8], fixture_path: &Path) -> Vec<u8> {
    let text = std::str::from_utf8(raw)
        .unwrap_or_else(|e| panic!("{} is not valid UTF-8: {e}", fixture_path.display()));
    let mut lines = text.lines();
    let header = lines
        .next()
        .unwrap_or_else(|| panic!("{} is missing asciinema header", fixture_path.display()));
    let header: serde_json::Value = serde_json::from_str(header).unwrap_or_else(|e| {
        panic!(
            "{} has invalid asciinema header: {e}",
            fixture_path.display()
        )
    });
    assert_eq!(
        header.get("version").and_then(serde_json::Value::as_u64),
        Some(2),
        "{} must be asciinema v2",
        fixture_path.display()
    );

    let mut bytes = Vec::new();
    for line in lines {
        let event: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!(
                "{} has invalid asciinema event `{line}`: {e}",
                fixture_path.display()
            )
        });
        let event = event
            .as_array()
            .unwrap_or_else(|| panic!("{} event is not an array", fixture_path.display()));
        if event.len() < 3 || event[1].as_str() != Some("o") {
            continue;
        }
        let payload = event[2].as_str().unwrap_or_else(|| {
            panic!(
                "{} output event payload is not a string",
                fixture_path.display()
            )
        });
        bytes.extend_from_slice(payload.as_bytes());
    }
    bytes
}

fn run_fixture(fixture_path: &Path) {
    let bytes = fixture_bytes(fixture_path);
    let label = fixture_path.display().to_string();
    run_conformance(24, 80, &bytes, &label);
}

#[test]
fn corpus_all_fixtures() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    if !fixture_dir.exists() {
        return;
    }
    let mut count = 0usize;
    for entry in walkdir(&fixture_dir) {
        if entry
            .extension()
            .is_some_and(|e| e == "bin" || e == "vt" || e == "cast")
        {
            run_fixture(&entry);
            count += 1;
        }
    }
    assert!(count > 0, "conformance corpus must contain fixtures");
    eprintln!("[conformance] ran {count} corpus fixtures");
}

#[test]
fn corpus_contains_required_fixture_classes() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut categories = BTreeSet::new();
    let mut filenames = BTreeSet::new();
    for entry in walkdir(&fixture_dir) {
        if !entry
            .extension()
            .is_some_and(|e| e == "bin" || e == "vt" || e == "cast")
        {
            continue;
        }
        let relative = entry.strip_prefix(&fixture_dir).unwrap_or_else(|e| {
            panic!(
                "fixture {} is outside fixture root {}: {e}",
                entry.display(),
                fixture_dir.display()
            )
        });
        if let Some(category) = relative.components().next() {
            categories.insert(category.as_os_str().to_string_lossy().into_owned());
        }
        filenames.insert(relative.display().to_string());
    }

    for required in [
        "vttest",
        "esctest",
        "real",
        "asciinema",
        "pathological",
        "wide_chars",
    ] {
        assert!(
            categories.contains(required),
            "missing required corpus category `{required}`"
        );
    }

    for required_name in [
        "real/claude",
        "real/codex",
        "real/vim",
        "real/htop",
        "real/tmux",
    ] {
        assert!(
            filenames.iter().any(|name| name.contains(required_name)),
            "missing required fixture containing `{required_name}`"
        );
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
    run_conformance(24, 80, seq, "DECSTBM scroll region");
}

#[test]
fn vt_cursor_save_restore_sc_rc() {
    // ESC 7 / ESC 8 (save/restore cursor position + attrs).
    let seq = b"\x1b[10;20H\x1b7\x1b[1;1HAnywhere\x1b8Back";
    run_conformance(24, 80, seq, "cursor save/restore ESC 7/8");
}

#[test]
fn vt_decawm_autowrap_off() {
    // Disable auto-wrap (DECAWM off). Characters past end of line stay at last col.
    let mut seq = b"\x1b[?7l".to_vec();
    seq.extend(std::iter::repeat_n(b'X', 100));
    run_conformance(24, 80, &seq, "DECAWM off beyond right margin");
}

#[test]
fn vt_erase_line_variants() {
    // CSI K: erase to end (0), from start (1), whole line (2).
    let seq = b"\x1b[10;1HHello World\x1b[10;6H\x1b[0K"; // erase from 'W' to end
    run_conformance(24, 80, seq, "erase to end of line");
    let seq = b"\x1b[10;1HHello World\x1b[10;6H\x1b[1K"; // erase from start to cursor
    run_conformance(24, 80, seq, "erase from start to cursor");
    let seq = b"\x1b[10;1HHello World\x1b[10;6H\x1b[2K"; // erase whole line
    run_conformance(24, 80, seq, "erase whole line");
}

#[test]
fn vt_insert_delete_chars() {
    // CSI @ (ICH - insert characters), CSI P (DCH - delete characters).
    let seq = b"\x1b[5;1HABCDE\x1b[5;3H\x1b[@X"; // insert X at position 3
    run_conformance(24, 80, seq, "insert character (ICH)");
    let seq = b"\x1b[5;1HABCDE\x1b[5;3H\x1b[2P"; // delete 2 chars from position 3
    run_conformance(24, 80, seq, "delete characters (DCH)");
}

#[test]
fn vt_reverse_index_ri() {
    // ESC M (RI - reverse index: move cursor up, scroll if at top).
    let seq = b"\x1b[1;1HTop\x1b[1;4H\x1bM"; // at row 1, RI scrolls down
    run_conformance(24, 80, seq, "reverse index at top of screen");
    let seq = b"\x1b[5;1HMiddle\x1b[5;7H\x1bM"; // at row 5, RI moves up to row 4
    run_conformance(24, 80, seq, "reverse index in middle");
}

#[test]
fn vt_rgb_truecolor_sgr() {
    // SGR 38;2;R;G;B and 48;2;R;G;B (RGB foreground and background).
    let seq = b"\x1b[38;2;255;128;0m\x1b[48;2;0;0;128mOrange on navy\x1b[0m";
    run_conformance(24, 80, seq, "RGB truecolor SGR");
}

#[test]
fn vt_256color_sgr() {
    // SGR 38;5;N and 48;5;N (256-color palette).
    let seq = b"\x1b[38;5;196m\x1b[48;5;21mRed on blue\x1b[0m";
    run_conformance(24, 80, seq, "256-color SGR");
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
    run_conformance(24, 80, &seq, "full-screen clear and rewrite (5 frames)");
}

#[test]
fn vt_many_sgr_on_off_cycles() {
    // Stress SGR attribute tracking with many enable/disable cycles.
    let mut seq: Vec<u8> = Vec::new();
    for _ in 0..50 {
        seq.extend_from_slice(b"\x1b[1mB\x1b[22mN\x1b[3mI\x1b[23mN\x1b[4mU\x1b[24mN\x1b[0m");
    }
    run_conformance(24, 80, &seq, "many SGR attribute cycles");
}

#[test]
fn vt_cursor_column_set_cha() {
    // CSI G (CHA - cursor horizontal absolute, 1-based col).
    let seq = b"\x1b[5;1HHello\x1b[3G*"; // move to col 3, overwrite with *
    run_conformance(24, 80, seq, "cursor horizontal absolute (CHA)");
}

#[test]
fn vt_repeat_preceding_char_rep() {
    // CSI b (REP - repeat preceding printed character N times).
    let seq = b"A\x1b[79b"; // 'A' repeated 79 times = 80 cols
    run_conformance(24, 80, seq, "repeat preceding char (REP)");
}

#[test]
fn vt_erase_display_from_cursor() {
    // CSI J variants: 0=cursor to end, 1=start to cursor, 2=all, 3=with scrollback.
    let seq = b"\x1b[10;1HFirst\r\nSecond\r\nThird\x1b[11;1H\x1b[0J";
    run_conformance(24, 80, seq, "erase display from cursor to end");
    let seq = b"\x1b[10;1HFirst\r\nSecond\r\nThird\x1b[11;4H\x1b[1J";
    run_conformance(24, 80, seq, "erase display from start to cursor");
    let seq = b"\x1b[10;1HFirst\r\nSecond\r\nThird\x1b[2J";
    run_conformance(24, 80, seq, "erase entire display");
}

// ---------------------------------------------------------------------------
// More VT conformance tests — common agent TUI patterns
// ---------------------------------------------------------------------------

#[test]
fn vt_scroll_up_csi_s_down_csi_t() {
    // CSI S (scroll up n lines).
    let seq = b"\x1b[1;1HLine1\r\nLine2\r\nLine3\r\nLine4\x1b[2S"; // scroll up 2 lines
    run_conformance(24, 80, seq, "scroll up CSI S");
}

#[test]
fn vt_line_feed_beyond_scroll_region() {
    // LF at bottom of a scroll region causes the region to scroll.
    // Set region to rows 5-10, move to row 10, write LF.
    let seq = b"\x1b[5;10r\x1b[10;1HLast\n\x1b[r";
    run_conformance(24, 80, seq, "LF at bottom of scroll region");
}

#[test]
fn vt_cursor_vertical_absolute_vpa() {
    // CSI d (VPA - cursor vertical absolute, 1-based row).
    let seq = b"\x1b[5;10H\x1b[15dHere"; // move to row 15 (cursor at col 10)
    run_conformance(24, 80, seq, "cursor vertical absolute (VPA)");
}

#[test]
fn vt_cursor_next_prev_line() {
    // CSI E (CNL - cursor next line), CSI F (CPL - cursor previous line).
    let seq = b"\x1b[10;5H\x1b[3EDown3"; // from (10,5) go down 3 rows, col reset to 0
    run_conformance(24, 80, seq, "cursor next line (CNL)");
    let seq = b"\x1b[10;5H\x1b[2FUp2"; // from (10,5) go up 2 rows, col reset to 0
    run_conformance(24, 80, seq, "cursor previous line (CPL)");
}

#[test]
fn vt_dim_and_strikethrough_sgr() {
    // SGR 2 (dim/faint), SGR 9 (strikethrough) — used by agent TUIs for metadata.
    let seq = b"\x1b[2mFaint\x1b[22m \x1b[9mStrike\x1b[29m\x1b[0m";
    run_conformance(24, 80, seq, "dim and strikethrough SGR");
}

#[test]
fn vt_blinking_hidden_sgr() {
    // SGR 5 (blink), SGR 8 (conceal/hidden) — some TUIs use these.
    let seq = b"\x1b[5mBlink\x1b[25m \x1b[8mHidden\x1b[28m\x1b[0m";
    run_conformance(24, 80, seq, "blink and hidden SGR");
}

#[test]
fn vt_cursor_up_with_scrollback() {
    // Move cursor to top, RI creates scrollback, then scroll back.
    let seq = b"\x1b[1;1HTop\r\nLine2\r\nLine3\x1b[1;1H\x1bM\x1bM"; // two RI at top
    run_conformance(24, 80, seq, "cursor RI creates scrollback");
}

#[test]
fn vt_insert_delete_lines_with_scroll_region() {
    // IL (L) and DL (M) inside a scroll region.
    let seq = b"\x1b[5;15r\x1b[5;1HLine5\x1b[5;1H\x1b[2L\x1b[r";
    run_conformance(24, 80, seq, "insert lines in scroll region");
    let seq = b"\x1b[5;15r\x1b[5;1HLine5\r\nLine6\r\nLine7\x1b[5;1H\x1b[2M\x1b[r";
    run_conformance(24, 80, seq, "delete lines in scroll region");
}

#[test]
fn vt_mixed_wide_and_narrow() {
    // Mix of CJK wide chars and ASCII narrow chars on same line.
    run_conformance(
        24,
        80,
        "AB你好CD\r\n".as_bytes(),
        "mixed wide and narrow chars",
    );
}

#[test]
fn vt_ris_reset_clears_all_state() {
    // ESC c (RIS - reset to initial state): clears grid + attrs + cursor.
    let seq = b"\x1b[1;31mRed text\x1bc"; // RIS resets everything
    run_conformance(24, 80, seq, "RIS resets all state");
}

#[test]
fn vt_sequence_split_across_process_calls() {
    // A real PTY might split a sequence across two read() calls.
    // DamageGrid must handle this correctly via vte's streaming parser.
    let mut grid = DamageGrid::new(24, 80, 1_000);
    // Process part 1: incomplete CSI sequence
    grid.process(b"\x1b[5;10");
    // Process part 2: completes the CSI H (cursor move) + text
    grid.process(b"HHello");
    let left = snapshot_damagegrid(&grid);

    let mut one_shot = DamageGrid::new(24, 80, 1_000);
    one_shot.process(b"\x1b[5;10HHello");
    let right_snap = snapshot_damagegrid(&one_shot);
    left.assert_eq(&right_snap, "split CSI sequence across process() calls");
}

/// Minimal recursive directory walker that avoids pulling in `walkdir` as a dev dep.
fn walkdir(dir: &Path) -> Vec<PathBuf> {
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
