//! Model-correctness tests for the capsule rendering plan's PR 4 semantics:
//! CSI default-deny, DECSTR/DECSCUSR in-grid, `?2026` absorption,
//! grapheme-cluster cells, wide-lead overwrite, DSR clamp, exact-dedupe
//! preserve-on-clear, and the spurious-LF damage removal.

use super::{DamageGrid, PassthroughEvent};

fn cell_text(grid: &DamageGrid, row: u16, col: u16) -> String {
    let (rows, _) = grid.size();
    let view = grid.scrollback_view(0, rows);
    view.cell(row, col)
        .map(|c| c.contents().to_owned())
        .unwrap_or_default()
}

#[test]
fn unknown_csi_is_default_denied_and_carried_as_dropped() {
    let mut grid = DamageGrid::new(5, 20, 10);
    // `CSI 18 t` (xterm window report) is not handled and not allowlisted.
    grid.process(b"\x1b[18t");
    let events = grid.drain_passthrough();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PassthroughEvent::DroppedCsi(bytes) if bytes.ends_with(b"t"))),
        "unknown CSI must surface as DroppedCsi: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, PassthroughEvent::UnhandledCsi(_))),
        "unknown CSI must not be forwarded: {events:?}"
    );
}

#[test]
fn kitty_and_modify_other_keys_stay_on_the_forward_allowlist() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process(b"\x1b[>1u\x1b[<1u\x1b[>4;2m");
    let forwarded: Vec<Vec<u8>> = grid
        .drain_passthrough()
        .into_iter()
        .filter_map(|e| match e {
            PassthroughEvent::UnhandledCsi(bytes) => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(
        forwarded,
        vec![
            b"\x1b[>1u".to_vec(),
            b"\x1b[<1u".to_vec(),
            b"\x1b[>4;2m".to_vec()
        ],
        "exactly the allowlist is forwarded"
    );
}

#[test]
fn decstr_resets_modes_attrs_and_margins_in_grid() {
    let mut grid = DamageGrid::new(10, 20, 10);
    grid.process(b"\x1b[1m\x1b[?25l\x1b[?1h\x1b[?2004h\x1b[2;5r");
    grid.process(b"\x1b[!p");
    assert!(!grid.hide_cursor(), "DECSTR must re-show the cursor");
    assert!(
        !grid.application_cursor(),
        "DECSTR must reset application cursor keys"
    );
    assert!(!grid.bracketed_paste(), "DECSTR must reset bracketed paste");
    let events = grid.drain_passthrough();
    assert!(
        !events.iter().any(|e| matches!(
            e,
            PassthroughEvent::UnhandledCsi(bytes) if bytes.ends_with(b"p")
        )),
        "DECSTR must never be forwarded: {events:?}"
    );
    // Attrs reset: the next glyph is plain.
    grid.process(b"x");
    let (rows, _) = grid.size();
    let view = grid.scrollback_view(0, rows);
    let cell = view.cell(0, 0).expect("written cell");
    assert!(!cell.attrs.bold, "DECSTR must reset SGR attributes");
    // Margins reset: a newline at the old margin bottom (row 4) must not
    // scroll a 2..5 region any more.
    grid.process(b"\x1b[10;1Hbottom");
    assert_eq!(cell_text(&grid, 9, 0), "b");
}

#[test]
fn decscusr_is_tracked_per_grid_and_not_forwarded() {
    let mut grid = DamageGrid::new(5, 20, 10);
    assert_eq!(grid.cursor_style(), 0);
    grid.process(b"\x1b[5 q");
    assert_eq!(grid.cursor_style(), 5);
    let events = grid.drain_passthrough();
    assert!(
        events.is_empty(),
        "DECSCUSR is reconciled by the encoder, never forwarded: {events:?}"
    );
}

#[test]
fn synchronized_output_toggles_are_absorbed() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process(b"\x1b[?2026h\x1b[?2026l");
    assert!(
        grid.drain_passthrough().is_empty(),
        "?2026 must be absorbed by the grid"
    );
}

#[test]
fn combining_mark_joins_base_cell() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process("e\u{301}!".as_bytes());
    assert_eq!(cell_text(&grid, 0, 0), "e\u{301}");
    assert_eq!(cell_text(&grid, 0, 1), "!");
}

#[test]
fn vs16_and_zwj_sequences_stay_one_cluster() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process("\u{2601}\u{fe0f}X".as_bytes());
    assert_eq!(cell_text(&grid, 0, 0), "\u{2601}\u{fe0f}");
    assert_eq!(cell_text(&grid, 0, 1), "X");

    let mut grid = DamageGrid::new(5, 20, 10);
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    grid.process(family.as_bytes());
    assert_eq!(cell_text(&grid, 0, 0), family);
}

#[test]
fn overwriting_a_wide_lead_blanks_the_continuation() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process("\u{4f60}".as_bytes());
    grid.process(b"\x1b[1;1HA");
    let (rows, _) = grid.size();
    let view = grid.scrollback_view(0, rows);
    let continuation = view.cell(0, 1).expect("continuation cell");
    assert!(
        !continuation.is_wide_continuation && continuation.contents().is_empty(),
        "stale continuation after lead overwrite: {continuation:?}"
    );
}

#[test]
fn dsr_clamps_the_deferred_wrap_phantom_column() {
    let mut grid = DamageGrid::new(5, 10, 10);
    grid.process(b"0123456789"); // fills row 0, arms pending wrap
    grid.process(b"\x1b[6n");
    let reply = grid
        .drain_passthrough()
        .into_iter()
        .find_map(|e| match e {
            PassthroughEvent::Reply(bytes) => Some(bytes),
            _ => None,
        })
        .expect("CPR reply");
    assert_eq!(
        reply, b"\x1b[1;10R",
        "CPR must clamp the phantom column to the last real column"
    );
}

#[test]
fn repeated_clear_without_mutation_preserves_exactly_once() {
    let mut grid = DamageGrid::new(5, 20, 100);
    grid.process(b"alpha\r\nbeta");
    grid.process(b"\x1b[2J\x1b[H");
    let after_first = grid.scrollback_len();
    assert!(after_first >= 2, "first clear must preserve the screen");

    // Clear again with no content mutation in between: nothing new retained.
    grid.process(b"\x1b[2J\x1b[H");
    assert_eq!(
        grid.scrollback_len(),
        after_first,
        "a clear without mutation must not duplicate the transcript (D11)"
    );

    // Identical repaint then clear: byte-equal block is deduped too.
    grid.process(b"alpha\r\nbeta");
    grid.process(b"\x1b[2J\x1b[H");
    assert_eq!(
        grid.scrollback_len(),
        after_first,
        "a byte-identical repaint must not duplicate the transcript (D11)"
    );

    // Genuinely new content is preserved.
    grid.process(b"gamma");
    grid.process(b"\x1b[2J\x1b[H");
    assert!(
        grid.scrollback_len() > after_first,
        "new content must still be preserved on clear"
    );
}

#[test]
fn plain_line_feed_marks_no_damage() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process(b"x");
    drop(grid.dirty_spans());
    // LF mid-screen moves the cursor; no cell changed.
    grid.process(b"\n");
    let spans = grid.dirty_spans();
    assert!(
        spans.is_empty(),
        "a plain LF must not mark damage (D16): {spans:?}"
    );
}

fn first_reply(grid: &mut DamageGrid) -> Vec<u8> {
    grid.drain_passthrough()
        .into_iter()
        .find_map(|e| match e {
            PassthroughEvent::Reply(bytes) => Some(bytes),
            _ => None,
        })
        .expect("query reply")
}

#[test]
fn osc_color_queries_answer_from_stored_colors() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.set_reported_colors(Some((0x10, 0x20, 0x30)), Some((0xab, 0xcd, 0xef)));

    grid.process(b"\x1b]10;?\x07");
    assert_eq!(
        first_reply(&mut grid),
        b"\x1b]10;rgb:1010/2020/3030\x07",
        "OSC 10 must report the stored foreground with a BEL terminator"
    );

    grid.process(b"\x1b]11;?\x1b\\");
    assert_eq!(
        first_reply(&mut grid),
        b"\x1b]11;rgb:abab/cdcd/efef\x1b\\",
        "OSC 11 must report the stored background with an ST terminator"
    );
}

#[test]
fn osc_color_queries_answer_without_explicit_colors() {
    // Codex paints no backgrounds at all until OSC 11 is answered, so the
    // defaults must produce a reply even when the capsule never stored the
    // attach client's palette.
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process(b"\x1b]11;?\x07");
    assert_eq!(first_reply(&mut grid), b"\x1b]11;rgb:0000/0000/0000\x07");
}

#[test]
fn osc_color_set_forms_are_dropped() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process(b"\x1b]11;#336699\x07");
    assert!(
        grid.drain_passthrough()
            .into_iter()
            .all(|e| !matches!(e, PassthroughEvent::Reply(_))),
        "OSC 11 set form must not produce a reply"
    );
}
