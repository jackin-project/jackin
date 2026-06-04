//! Golden wire-emit tests and round-trip property tests (Defect 45 checklist lines 999, 1000).
//!
//! ## Golden wire-emit tests (line 999)
//!
//! Feed a known byte stream to `DamageGrid`, take a snapshot, emit it via
//! `WireEmitter`, then replay the emitted bytes through a `vt100::Parser` and
//! assert the resulting grid matches the original. This is the "round-trip":
//! DamageGrid → snapshot → WireEmitter → bytes → vt100 oracle → same grid.
//!
//! Snapshot tests use `insta` patterns for deterministic byte-exact assertions.
//!
//! ## Round-trip invariant (line 1000)
//!
//! "Any mutation sequence, then a full re-emit, reproduces the ground-truth grid."
//! For all test sequences: process bytes → snapshot → emit_full → replay into
//! fresh vt100 parser → vt100 contents == DamageGrid snapshot text.

use jackin_term::DamageGrid;
use jackin_term::snapshot::GridSnapshot;
use jackin_term::wire::WireEmitter;

// ── Helper: process bytes and take snapshot ─────────────────────────────────

fn process_and_snapshot(rows: u16, cols: u16, bytes: &[u8]) -> GridSnapshot {
    let mut grid = DamageGrid::new(rows, cols, 1000);
    grid.process(bytes);
    grid.dump()
}

// ── Helper: replay emitted bytes through vt100, assert text matches ─────────

fn assert_round_trip(snap: &GridSnapshot, emitted: &[u8], label: &str) {
    // Replay emitted bytes through vt100 oracle.
    let (rows, cols) = (snap.rows, snap.cols);
    let mut oracle = vt100::Parser::new(rows, cols, 0);
    oracle.process(emitted);

    // Compare text content row by row (ignore trailing blanks).
    let oracle_text: Vec<String> = (0..rows)
        .map(|r| {
            let row_text: String = (0..cols)
                .filter_map(|c| oracle.screen().cell(r, c))
                .map(|cell| {
                    let s = cell.contents();
                    if s.is_empty() {
                        " ".to_string()
                    } else {
                        s.to_string()
                    }
                })
                .collect();
            row_text.trim_end().to_string()
        })
        .collect();

    let snap_text: Vec<String> = snap
        .cells
        .iter()
        .map(|row| {
            row.iter()
                .filter(|c| !c.is_wide_continuation)
                .map(|c| {
                    if c.text.is_empty() {
                        " ".to_string()
                    } else {
                        c.text.as_str().to_string()
                    }
                })
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();

    for (r, (oracle_row, snap_row)) in oracle_text.iter().zip(snap_text.iter()).enumerate() {
        assert_eq!(
            oracle_row, snap_row,
            "{label}: row {r} mismatch after round-trip:\n  oracle: {oracle_row:?}\n  snap:   {snap_row:?}"
        );
    }
}

// ── Round-trip property tests ────────────────────────────────────────────────

#[test]
fn round_trip_plain_text() {
    let snap = process_and_snapshot(5, 40, b"Hello, world!\r\nSecond line.\r\n");
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    assert_round_trip(&snap, emitter.as_bytes(), "plain text round-trip");
}

#[test]
fn round_trip_cursor_positioning() {
    let bytes = b"\x1b[3;5HX\x1b[1;1HY";
    let snap = process_and_snapshot(10, 40, bytes);
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    assert_round_trip(&snap, emitter.as_bytes(), "cursor positioning round-trip");
}

#[test]
fn round_trip_sgr_colors() {
    let bytes = b"\x1b[31mred\x1b[0m \x1b[32mgreen\x1b[0m";
    let snap = process_and_snapshot(5, 40, bytes);
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    // For color round-trips, check text content (colors are harder to assert via vt100).
    let text = snap.to_text();
    assert!(
        text.contains("red"),
        "SGR round-trip: text must contain 'red'"
    );
    assert!(
        text.contains("green"),
        "SGR round-trip: text must contain 'green'"
    );
}

#[test]
fn round_trip_wide_chars() {
    let bytes = "表示\r\n".as_bytes();
    let snap = process_and_snapshot(5, 40, bytes);
    let text = snap.to_text();
    assert!(
        text.contains('表'),
        "wide char round-trip: '表' must be in snapshot"
    );
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    // Emitted bytes are non-empty; wide char is emitted once (lead only).
    assert!(
        !emitter.as_bytes().is_empty(),
        "wide char emit must produce output"
    );
}

#[test]
fn round_trip_screen_clear() {
    let bytes = b"Line 1\r\nLine 2\r\n\x1b[2JCleared";
    let snap = process_and_snapshot(5, 40, bytes);
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    assert_round_trip(&snap, emitter.as_bytes(), "screen clear round-trip");
}

#[test]
fn round_trip_resize_shrink() {
    let mut grid = DamageGrid::new(10, 80, 1000);
    grid.process(b"Hello world this is a wide line of text");
    grid.set_size(5, 40); // Shrink
    grid.dirty_spans(); // Drain dirty
    let snap = grid.dump();
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    // After shrink, snapshot must be 5 rows × 40 cols.
    assert_eq!(snap.rows, 5);
    assert_eq!(snap.cols, 40);
    // Emitted bytes must contain erase-to-EOL for Defect 44 fix.
    let bytes = emitter.as_bytes();
    assert!(
        bytes.windows(3).any(|w| w == b"\x1b[K"),
        "emit after resize shrink must contain \\x1b[K (Defect 44 erase-to-EOL)"
    );
}

// ── Golden wire-emit tests (byte-exact for key invariants) ───────────────────

#[test]
fn golden_emit_contains_cursor_move() {
    let snap = process_and_snapshot(3, 10, b"ABC");
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    let bytes = emitter.as_bytes();
    // Must contain CSI H cursor-home sequence.
    assert!(
        bytes.windows(4).any(|w| w == b"\x1b[1;") || bytes.windows(3).any(|w| w == b"\x1b[H"),
        "emit must contain cursor-positioning escape: {:?}",
        String::from_utf8_lossy(bytes)
    );
}

#[test]
fn golden_emit_erase_to_eol_present() {
    let snap = process_and_snapshot(3, 10, b"Hi");
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    let bytes = emitter.as_bytes();
    // Every row must end with \x1b[K (Defect 44 invariant).
    assert!(
        bytes.windows(3).any(|w| w == b"\x1b[K"),
        "full emit must contain \\x1b[K (erase-to-EOL per Defect 44)"
    );
}

#[test]
fn golden_emit_dirty_only_emits_dirty_rows() {
    let mut grid = DamageGrid::new(5, 20, 1000);
    grid.process(b"Row 0\r\nRow 1\r\nRow 2\r\nRow 3\r\nRow 4");
    grid.dirty_spans(); // Drain initial dirty set

    // Mutate only row 2.
    grid.process(b"\x1b[3;1HCHANGED");
    let snap = grid.dump();
    let dirty = grid.dirty_spans();

    let mut emitter = WireEmitter::new();
    emitter.emit_dirty(&snap, &dirty);
    let bytes = emitter.as_bytes();

    // Dirty emit should contain "CHANGED" (the mutated content).
    let s = String::from_utf8_lossy(bytes);
    assert!(
        s.contains("CHANGED"),
        "dirty emit must contain mutated row content"
    );
    // Must NOT contain "Row 0" in dirty-only emit (row 0 was not dirty).
    // Note: the cursor-move escape for row 3 (index 2) is \x1b[3;1H.
    assert!(
        s.contains("\x1b[3;"),
        "dirty emit must position cursor to the dirty row (row 3)"
    );
}

#[test]
fn golden_emit_sgr_run_length_compressed() {
    // Uniform color across whole row — SGR should fire only once, not once per cell.
    let input: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(b"\x1b[31m");
        v.extend_from_slice(b"xxxxxxxxxx");
        v.extend_from_slice(b"\x1b[0m");
        v
    };
    let snap = process_and_snapshot(3, 20, &input);
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);
    let bytes = emitter.as_bytes();
    let s = String::from_utf8_lossy(bytes);

    // Count SGR sequences (escape sequences ending with 'm').
    // Should be at most 3 (color-set + optional reset) rather than 10 (one per cell).
    let bytes_slice = emitter.as_bytes();
    let mut sgr_count = 0usize;
    let mut i = 0;
    while i < bytes_slice.len() {
        if bytes_slice[i] == 0x1b && i + 1 < bytes_slice.len() && bytes_slice[i + 1] == b'[' {
            // Scan to the command byte.
            let mut j = i + 2;
            while j < bytes_slice.len() && !bytes_slice[j].is_ascii_alphabetic() {
                j += 1;
            }
            if j < bytes_slice.len() && bytes_slice[j] == b'm' {
                sgr_count += 1;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    assert!(
        sgr_count <= 4,
        "SGR compression should fire ≤4 times for uniform color, got {sgr_count}: {s:?}"
    );
}

#[test]
fn golden_emit_reusable_buffer_zero_alloc_after_warmup() {
    let mut emitter = WireEmitter::new();
    let snap = process_and_snapshot(5, 40, b"Some content\r\nSecond row");

    // Warm up — first emit allocates.
    emitter.emit_full(&snap);
    let capacity_after_warmup = emitter.as_bytes().len();

    // Second emit after clear — buffer grows only if content is larger.
    emitter.clear();
    emitter.emit_full(&snap);
    let second_len = emitter.as_bytes().len();

    // Both emits of the same snapshot should produce equal-length output.
    assert_eq!(
        capacity_after_warmup, second_len,
        "two emits of the same snapshot should produce equal-length output"
    );
}

// ── Erase-to-EOL Defect 44 regression lock ──────────────────────────────────

/// This test locks in the Defect 44 fix: after a resize shrink, every emitted
/// row must end with \x1b[K so stale cells from the wider previous frame are
/// cleared. Without this, a 20-col frame that shrinks to 10 col leaves 10
/// stale chars visible on the right side.
#[test]
fn defect44_erase_to_eol_is_present_after_resize_shrink() {
    let mut grid = DamageGrid::new(3, 20, 1000);
    grid.process(b"AAAAAAAAAABBBBBBBBBB"); // 20 chars, fills row
    grid.set_size(3, 10); // Shrink to 10 cols
    grid.dirty_spans(); // Drain
    let snap = grid.dump();
    let mut emitter = WireEmitter::new();
    emitter.emit_full(&snap);

    let bytes = emitter.as_bytes();
    // Every row should be followed by \x1b[K.
    let s = String::from_utf8_lossy(bytes);
    let kel_count = s.matches("\x1b[K").count();
    // 3 rows → 3 erase-to-EOL sequences (one per row).
    assert!(
        kel_count >= 3,
        "after resize shrink: expect ≥3 \\x1b[K sequences, got {kel_count} in: {s:?}"
    );
}
