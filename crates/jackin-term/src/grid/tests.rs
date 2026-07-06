//! Tests for the terminal damage grid.
use super::*;

// device_query

fn replies(g: &mut DamageGrid) -> Vec<Vec<u8>> {
    g.drain_passthrough()
        .into_iter()
        .filter_map(|e| match e {
            PassthroughEvent::Reply(b) => Some(b),
            _ => None,
        })
        .collect()
}

#[test]
fn da1_answers_conservative_vt220() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[c");
    assert_eq!(replies(&mut g), vec![b"\x1b[?62c".to_vec()]);
}

#[test]
fn dsr_cursor_position_uses_grid_cursor_not_host() {
    let mut g = DamageGrid::new(10, 40, 100);
    g.process(b"\x1b[4;6H\x1b[6n"); // home to row4/col6 (1-based), then query
    assert_eq!(replies(&mut g), vec![b"\x1b[4;6R".to_vec()]);
}

#[test]
fn decrqm_declines_grapheme_width_mode_2027() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[?2027$p");
    // 0 = "mode not recognized" -> agent renders with legacy column widths.
    assert_eq!(replies(&mut g), vec![b"\x1b[?2027;0$y".to_vec()]);
}

#[test]
fn kitty_keyboard_query_answers_no_enhancement() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[?u");
    assert_eq!(replies(&mut g), vec![b"\x1b[?0u".to_vec()]);
}

#[test]
fn device_queries_are_not_forwarded_to_host() {
    let mut g = DamageGrid::new(4, 20, 100);
    g.process(b"\x1b[c\x1b[6n\x1b[?2026$p\x1b[?u");
    // Every event is an agent-bound Reply; none is a host-bound UnhandledCsi.
    for e in g.drain_passthrough() {
        assert!(
            matches!(e, PassthroughEvent::Reply(_)),
            "device query leaked to host as {e:?}"
        );
    }
}

// fuzz_regression

#[test]
fn fuzz_csi_cursor_down_count_does_not_overflow() {
    let mut grid = DamageGrid::new(24, 80, 1_000);
    let bytes = [
        0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 56, 66, 56, 0, 0, 26,
        27, 255, 253, 91, 253, 52, 56, 56, 56, 253, 52, 56, 56, 56, 66, 56, 0, 0, 26, 27, 152, 152,
        10, 3, 3,
    ];

    grid.process(&bytes);

    let (row, col) = grid.cursor_position();
    assert_eq!(row, 23);
    assert!(col < 80);
}

#[test]
fn fuzz_split_utf8_c1_control_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[0xc2, 0x8a, 0x1b, 0x1f]);
}

#[test]
fn fuzz_split_adjacent_utf8_leads_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[b'k', 0xd6, 0xd6]);
}

#[test]
fn fuzz_valid_utf8_prefix_before_incomplete_suffix_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[0xd6, 0x8c, 0xf0, 0xb9]);
}

#[test]
fn split_utf8_printable_matches_one_shot() {
    assert_one_shot_matches_byte_split("a¢b".as_bytes());
}

#[test]
fn split_incomplete_utf8_then_escape_matches_one_shot() {
    assert_one_shot_matches_byte_split(&[0xc2, 0x1b, b'[', b'2', b'C']);
}

fn assert_one_shot_matches_byte_split(bytes: &[u8]) {
    let mut one_shot = DamageGrid::new(24, 80, 1_000);
    let mut split = DamageGrid::new(24, 80, 1_000);

    one_shot.process(bytes);
    for byte in bytes {
        split.process(std::slice::from_ref(byte));
    }

    assert_eq!(one_shot.cursor_position(), split.cursor_position());
    assert_eq!(one_shot.alternate_screen(), split.alternate_screen());

    let (rows, cols) = one_shot.size();
    for row in 0..rows {
        for col in 0..cols {
            assert_eq!(one_shot.cell(row, col), split.cell(row, col));
        }
    }
}

// model_correctness
//
// Model-correctness tests for the capsule rendering plan's PR 4 semantics:
// CSI default-deny, DECSTR/DECSCUSR in-grid, `?2026` absorption,
// grapheme-cluster cells, wide-lead overwrite, DSR clamp, exact-dedupe
// preserve-on-clear, and the spurious-LF damage removal.

fn cell_text(grid: &DamageGrid, row: u16, col: u16) -> String {
    let (rows, _) = grid.size();
    let view = grid.scrollback_view(0, rows);
    view.cell(row, col)
        .map(|c| c.contents().to_owned())
        .unwrap_or_default()
}

fn first_col_text(grid: &DamageGrid, rows: u16) -> Vec<String> {
    (0..rows).map(|row| cell_text(grid, row, 0)).collect()
}

fn seed_first_col(grid: &mut DamageGrid) {
    grid.process(b"\x1b[1;1HA\x1b[2;1HB\x1b[3;1HC\x1b[4;1HD\x1b[5;1HE");
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
    assert!(
        grid.cell(0, 0).expect("cloud lead").is_wide,
        "VS16 emoji presentation must grow the lead to two columns"
    );
    assert!(
        grid.cell(0, 1)
            .expect("cloud continuation")
            .is_wide_continuation,
        "VS16 growth must create a continuation cell"
    );
    assert_eq!(cell_text(&grid, 0, 2), "X");

    let mut grid = DamageGrid::new(5, 20, 10);
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    grid.process(family.as_bytes());
    assert_eq!(cell_text(&grid, 0, 0), family);
    assert!(
        grid.cell(0, 0).expect("family lead").is_wide,
        "ZWJ emoji cluster stays two columns"
    );
    assert!(
        grid.cell(0, 1)
            .expect("family continuation")
            .is_wide_continuation,
        "ZWJ emoji cluster keeps a continuation cell"
    );
}

#[test]
fn halfwidth_katakana_voicing_mark_grows_cluster_width() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process("\u{ff76}\u{ff9e}X".as_bytes());

    assert_eq!(cell_text(&grid, 0, 0), "\u{ff76}\u{ff9e}");
    assert!(
        grid.cell(0, 0).expect("dakuten lead").is_wide,
        "dakuten cluster must grow to two columns"
    );
    assert!(
        grid.cell(0, 1)
            .expect("dakuten continuation")
            .is_wide_continuation,
        "dakuten growth must create a continuation cell"
    );
    assert_eq!(
        cell_text(&grid, 0, 2),
        "X",
        "next printable must use the grown cursor column"
    );
}

#[test]
fn dsr_reports_cursor_after_cluster_width_growth() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process("\u{ff76}\u{ff9e}".as_bytes());
    grid.process(b"\x1b[6n");
    let reply = first_reply(&mut grid);
    assert_eq!(
        reply, b"\x1b[1;3R",
        "DSR must report the cursor after the grown two-column cluster"
    );
}

#[test]
fn cluster_width_growth_marks_the_grown_span_dirty() {
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.process("\u{ff76}".as_bytes());
    drop(grid.dirty_spans());

    grid.process("\u{ff9e}".as_bytes());
    let spans = grid.dirty_spans();
    let DirtySpans::Rows(rows) = spans else {
        panic!("expected row-span dirty tracking, got {spans:?}");
    };
    assert_eq!(
        rows.as_slice(),
        [crate::damage::DirtySpan {
            row: 0,
            start_col: 0,
            end_col: 2,
        }],
        "dakuten growth must dirty both the lead and new continuation"
    );
}

#[test]
fn final_column_cluster_growth_keeps_deferred_wrap_contract() {
    let mut grid = DamageGrid::new(5, 5, 10);
    grid.process(b"\x1b[1;5H");
    grid.process("\u{ff76}\u{ff9e}".as_bytes());
    grid.process(b"\x1b[6n");
    assert_eq!(
        first_reply(&mut grid),
        b"\x1b[1;5R",
        "DSR must clamp final-column cluster growth to the last real column"
    );

    grid.process(b"Z");
    assert_eq!(
        cell_text(&grid, 1, 0),
        "Z",
        "next printable after final-column growth must consume deferred wrap"
    );
}

#[test]
fn deferred_wrap_records_soft_row_provenance() {
    let mut grid = DamageGrid::new(3, 5, 10);
    grid.process(b"abcdeZ");
    let snap = grid.dump();

    assert_eq!(snap.row_wraps[0], RowWrap::Hard);
    assert_eq!(snap.row_wraps[1], RowWrap::Soft);
    assert_eq!(cell_text(&grid, 1, 0), "Z");
}

#[test]
fn explicit_line_feed_records_hard_row_provenance() {
    let mut grid = DamageGrid::new(3, 5, 10);
    grid.process(b"abc\nZ");
    let snap = grid.dump();

    assert_eq!(snap.row_wraps[1], RowWrap::Hard);
    assert_eq!(cell_text(&grid, 1, 3), "Z");
}

#[test]
fn wrap_provenance_survives_scrollback_view_and_resize() {
    let mut grid = DamageGrid::new(2, 5, 10);
    grid.process(b"abcdeZ\nY");

    let view = grid.scrollback_view(1, 2);
    assert_eq!(view.row_wrap(0), Some(RowWrap::Hard));
    assert_eq!(view.row_wrap(1), Some(RowWrap::Soft));

    let snap = grid.dump_scrollback_view(1, 2);
    assert_eq!(snap.row_wraps, [RowWrap::Hard, RowWrap::Soft]);

    grid.set_size(3, 5);
    let resized = grid.dump();
    assert_eq!(
        resized.row_wraps[0],
        RowWrap::Soft,
        "visible soft-wrap provenance must survive resize"
    );
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

#[test]
fn reported_colors_survive_a_none_none_reapply() {
    // Reattach from a terminal that could not read its palette passes
    // (None, None); the last reporting client's colors must hold.
    let mut grid = DamageGrid::new(5, 20, 10);
    grid.set_reported_colors(Some((0x10, 0x20, 0x30)), Some((0x40, 0x50, 0x60)));
    grid.set_reported_colors(None, None);
    grid.process(b"\x1b]11;?\x07");
    assert_eq!(first_reply(&mut grid), b"\x1b]11;rgb:4040/5050/6060\x07");
    grid.process(b"\x1b]10;?\x07");
    assert_eq!(first_reply(&mut grid), b"\x1b]10;rgb:1010/2020/3030\x07");
}

// row_arena

#[test]
fn shared_row_arena_recycles_rows_between_grids() {
    let arena = RowArena::default();
    {
        let mut grid = DamageGrid::with_row_arena(3, 8, 8, arena.clone());
        grid.process(b"one\ntwo\nthree\nfour\nfive");
    }
    let recycled_after_drop = arena.recycled_rows();
    assert!(
        recycled_after_drop >= 6,
        "primary + alternate rows should return to shared arena on drop"
    );

    let _next_grid = DamageGrid::with_row_arena(3, 8, 8, arena.clone());
    assert!(
        arena.recycled_rows() < recycled_after_drop,
        "new grids should draw rows from the shared arena before allocating"
    );
}

// scrollback_view

#[test]
fn dump_scrollback_view_offset_zero_matches_live() {
    let mut g = DamageGrid::new(2, 8, 100);
    g.process(b"abc\r\ndef\r\nghi");
    let view = g.dump_scrollback_view(0, 2);
    assert_eq!(view.cells, g.dump().cells);
}

#[test]
fn dump_scrollback_view_shows_scrolled_history() {
    let mut g = DamageGrid::new(2, 8, 100);
    for i in 0..6 {
        g.process(format!("line{i}\r\n").as_bytes());
    }
    assert!(g.scrollback_len() > 0, "scrollback should have filled");
    let view = g.dump_scrollback_view(2, 2);
    assert_eq!(view.cells.len(), 2);
    // The scrolled-up view differs from the live tail; it shows history.
    assert_ne!(view.cells, g.dump().cells);
}

#[test]
fn borrowed_scrollback_view_matches_owned_dump() {
    let mut g = DamageGrid::new(3, 10, 100);
    for i in 0..8 {
        g.process(format!("line{i}\r\n").as_bytes());
    }

    let owned = g.dump_scrollback_view(2, 3);
    let borrowed = g.scrollback_view(2, 3);

    assert_eq!(borrowed.rows, owned.rows);
    assert_eq!(borrowed.cols, owned.cols);
    for row in 0..owned.rows {
        for col in 0..owned.cols {
            let owned_cell = owned.cell(row, col).expect("owned cell");
            let borrowed_cell = borrowed.cell(row, col).expect("borrowed cell");
            assert_eq!(borrowed_cell.contents(), owned_cell.text);
            assert_eq!(borrowed_cell.is_wide, owned_cell.is_wide);
            assert_eq!(
                borrowed_cell.is_wide_continuation,
                owned_cell.is_wide_continuation
            );
            assert_eq!(borrowed_cell.fgcolor(), owned_cell.fg);
            assert_eq!(borrowed_cell.bgcolor(), owned_cell.bg);
            assert_eq!(borrowed_cell.bold(), owned_cell.attributes.bold);
            assert_eq!(borrowed_cell.italic(), owned_cell.attributes.italic);
            assert_eq!(borrowed_cell.underline(), owned_cell.attributes.underline);
            assert_eq!(
                borrowed_cell.attrs.underline_style,
                owned_cell.underline_style
            );
            assert_eq!(
                borrowed_cell.attrs.underline_color,
                owned_cell.underline_color
            );
            assert_eq!(borrowed_cell.inverse(), owned_cell.attributes.inverse);
            assert_eq!(borrowed_cell.dim(), owned_cell.attributes.dim);
            assert_eq!(
                borrowed_cell.strikethrough(),
                owned_cell.attributes.strikethrough
            );
            assert_eq!(borrowed_cell.slow_blink(), owned_cell.attributes.slow_blink);
            assert_eq!(
                borrowed_cell.rapid_blink(),
                owned_cell.attributes.rapid_blink
            );
            assert_eq!(borrowed_cell.conceal(), owned_cell.attributes.conceal);
            assert_eq!(borrowed_cell.overline(), owned_cell.attributes.overline);
            assert_eq!(
                borrowed_cell
                    .hyperlink
                    .as_ref()
                    .map(|link| link.id.as_str()),
                owned_cell.hyperlink_id.as_deref()
            );
            assert_eq!(
                borrowed_cell
                    .hyperlink
                    .as_ref()
                    .map(|link| link.uri.as_str()),
                owned_cell.hyperlink_uri.as_deref()
            );
        }
    }
}

#[test]
fn osc8_hyperlink_is_cell_metadata_not_passthrough() {
    let mut g = DamageGrid::new(2, 20, 100);
    g.process(b"\x1b]8;id=docs;https://example.test\x07AB\x1b]8;;\x07C");

    let snap = g.dump();
    let first = snap.cell(0, 0).expect("linked A");
    let second = snap.cell(0, 1).expect("linked B");
    let third = snap.cell(0, 2).expect("plain C");

    assert_eq!(first.hyperlink_id.as_deref(), Some("docs"));
    assert_eq!(first.hyperlink_uri.as_deref(), Some("https://example.test"));
    assert_eq!(
        second.hyperlink_uri.as_deref(),
        Some("https://example.test")
    );
    assert_eq!(third.hyperlink_uri, None);
    assert!(
        g.drain_passthrough().is_empty(),
        "OSC 8 should not be raw passthrough"
    );
}

#[test]
fn sgr_records_extended_visible_attributes() {
    let mut g = DamageGrid::new(2, 20, 100);
    g.process(b"\x1b[4:3;58:2:12:34:56;9;5;6;8;53mA");

    let snap = g.dump();
    let cell = snap.cell(0, 0).expect("styled cell");
    assert_eq!(cell.underline_style, UnderlineStyle::Curly);
    assert_eq!(cell.underline_color, Color::Rgb(12, 34, 56));
    assert!(cell.attributes.strikethrough);
    assert!(cell.attributes.slow_blink);
    assert!(cell.attributes.rapid_blink);
    assert!(cell.attributes.conceal);
    assert!(cell.attributes.overline);
}

#[test]
fn sgr_resets_extended_visible_attributes() {
    let mut g = DamageGrid::new(2, 20, 100);
    g.process(b"\x1b[4:5;58;5;123;9;5;6;8;53mA");
    g.process(b"\x1b[24;25;28;29;55;59mB");

    let snap = g.dump();
    let first = snap.cell(0, 0).expect("first styled cell");
    assert_eq!(first.underline_style, UnderlineStyle::Dashed);
    assert_eq!(first.underline_color, Color::Idx(123));
    assert!(first.attributes.strikethrough);
    assert!(first.attributes.slow_blink);
    assert!(first.attributes.rapid_blink);
    assert!(first.attributes.conceal);
    assert!(first.attributes.overline);

    let second = snap.cell(0, 1).expect("reset cell");
    assert_eq!(second.underline_style, UnderlineStyle::None);
    assert_eq!(second.underline_color, Color::Default);
    assert!(!second.attributes.strikethrough);
    assert!(!second.attributes.slow_blink);
    assert!(!second.attributes.rapid_blink);
    assert!(!second.attributes.conceal);
    assert!(!second.attributes.overline);
}

#[test]
fn sgr_colon_rgb_preserves_foreground_and_background() {
    let mut g = DamageGrid::new(2, 20, 100);
    g.process(b"\x1b[38:2:1:2:3;48:2::4:5:6mA");

    let snap = g.dump();
    let cell = snap.cell(0, 0).expect("rgb cell");
    assert_eq!(cell.fg, Color::Rgb(1, 2, 3));
    assert_eq!(cell.bg, Color::Rgb(4, 5, 6));
}

#[test]
fn sgr_colon_rgb_preserves_zero_red_channel() {
    // The no-colorspace colon form `38:2:r:g:b` (4 subparams after `38`) must
    // read a leading R=0 as the red channel, not as an empty colorspace-id to
    // skip. The colorspace skip only applies to the 5-subparam `38:2:Pi:r:g:b`.
    let mut g = DamageGrid::new(2, 20, 100);
    g.process(b"\x1b[38:2:0:5:6mA");

    let snap = g.dump();
    let cell = snap.cell(0, 0).expect("rgb cell");
    assert_eq!(cell.fg, Color::Rgb(0, 5, 6));
}

#[test]
fn scroll_ops_record_linefeed_scrolls() {
    let mut g = DamageGrid::new(3, 10, 100);
    g.process(b"one\r\ntwo\r\nthree\r\nfour");

    assert_eq!(
        g.drain_scroll_ops(),
        vec![ScrollOp::Up {
            top: 0,
            bottom: 2,
            rows: 1
        }]
    );
    assert!(
        g.drain_scroll_ops().is_empty(),
        "drain_scroll_ops must clear recorded ops"
    );
}

#[test]
fn scroll_ops_record_decstbm_region_scrolls() {
    let mut g = DamageGrid::new(5, 10, 100);
    g.process(b"\x1b[2;4r\x1b[4;1H\n");

    assert_eq!(
        g.drain_scroll_ops(),
        vec![ScrollOp::Up {
            top: 1,
            bottom: 3,
            rows: 1
        }]
    );
}

#[test]
fn scroll_ops_record_insert_delete_line_and_reverse_index() {
    let mut g = DamageGrid::new(5, 10, 100);
    g.process(b"\x1b[2;4r\x1b[3;1H\x1b[2L\x1b[1M\x1b[2;1H\x1bM");

    assert_eq!(
        g.drain_scroll_ops(),
        vec![
            ScrollOp::Down {
                top: 2,
                bottom: 3,
                rows: 2
            },
            ScrollOp::Up {
                top: 2,
                bottom: 3,
                rows: 1
            },
            ScrollOp::Down {
                top: 1,
                bottom: 3,
                rows: 1
            },
        ]
    );
}

#[test]
fn insert_delete_line_noop_when_cursor_above_scroll_region() {
    let mut g = DamageGrid::new(5, 10, 100);
    seed_first_col(&mut g);
    g.process(b"\x1b[3;5r");
    let before = first_col_text(&g, 5);

    g.process(b"\x1b[L\x1b[M");

    assert_eq!(first_col_text(&g, 5), before);
    assert!(g.drain_scroll_ops().is_empty());
}

#[test]
fn insert_delete_line_noop_when_cursor_below_scroll_region() {
    let mut g = DamageGrid::new(5, 10, 100);
    seed_first_col(&mut g);
    g.process(b"\x1b[1;3r\x1b[5;1H");
    let before = first_col_text(&g, 5);

    g.process(b"\x1b[L\x1b[M");

    assert_eq!(first_col_text(&g, 5), before);
    assert!(g.drain_scroll_ops().is_empty());
}

#[test]
fn insert_line_inside_scroll_region_inserts_blank_and_drops_region_bottom() {
    let mut g = DamageGrid::new(5, 10, 100);
    seed_first_col(&mut g);
    g.process(b"\x1b[2;4r\x1b[3;1H\x1b[L");

    assert_eq!(first_col_text(&g, 5), vec!["A", "B", "", "C", "E"]);
    assert_eq!(
        g.drain_scroll_ops(),
        vec![ScrollOp::Down {
            top: 2,
            bottom: 3,
            rows: 1
        }]
    );
}

#[test]
fn scroll_ops_record_csi_scroll_up_and_down() {
    // CSI S (scroll up) and CSI T (scroll down) both shift the scroll region
    // and must each record a ScrollOp so the deferred scroll-region optimizer
    // sees both directions. Regression: CSI T previously recorded nothing.
    let mut g = DamageGrid::new(5, 10, 100);
    g.process(b"\x1b[2S\x1b[3T");

    assert_eq!(
        g.drain_scroll_ops(),
        vec![
            ScrollOp::Up {
                top: 0,
                bottom: 4,
                rows: 2
            },
            ScrollOp::Down {
                top: 0,
                bottom: 4,
                rows: 3
            },
        ]
    );
}

#[test]
fn zero_scrollback_limit_evicts_without_panic() {
    // scrollback_limit == 0 means "no scrollback"; rows that would be
    // preserved must be dropped, not pushed onto an empty buffer with a
    // remove(0) that would panic (len 0).
    let mut g = DamageGrid::new(2, 8, 0);
    for i in 0..6 {
        g.process(format!("line{i}\r\n").as_bytes());
    }
    assert_eq!(g.scrollback_len(), 0);
}

#[test]
fn dump_dirty_patch_drains_changed_rows_only() {
    let mut g = DamageGrid::new(3, 12, 100);
    g.process(b"\x1b[1;1Halpha\x1b[2;1Hbeta");
    drop(g.dump_dirty_patch());

    g.process(b"\x1b[2;1Hgamma");
    let patch = g.dump_dirty_patch();
    let changed_rows = patch.changed_rows().collect::<Vec<_>>();

    assert_eq!(changed_rows.len(), 1);
    assert_eq!(changed_rows[0].0, 1);
    let text: String = changed_rows[0]
        .1
        .iter()
        .map(|cell| {
            if cell.contents().is_empty() {
                " ".to_owned()
            } else {
                cell.contents().to_owned()
            }
        })
        .collect();
    assert!(text.starts_with("gamma"));
    assert!(g.dump_dirty_patch().is_empty());
}

#[test]
fn resize_to_zero_rows_keeps_grid_addressable() {
    // Regression: a capsule pane squeezed below its border height resized its
    // shadow grid to 0 rows. The next PTY burst — `ESC[1;1H` (home) then
    // `ESC[J` (erase-to-end) — indexed `active_grid()[0]` on an empty VecDeque
    // and panicked the whole capsule daemon ("Out of bounds access"). The grid
    // must clamp to at least 1×1 and absorb the bytes without panicking.
    let mut g = DamageGrid::new(10, 132, 100);
    g.process(b"\x1b[3;3HX");

    g.set_size(0, 0);
    let (rows, cols) = g.size();
    assert!(
        rows >= 1 && cols >= 1,
        "grid clamped to 1x1, got {rows}x{cols}"
    );

    // The exact alt-screen repaint burst from the soak crash log.
    g.process(
        b"\x1b[?2026h\x1b[1;1H\x1b[J\x1b[39m\x1b[49m\x1b[0m\x1b[0 q\x1b[?25h\x1b[3;3H\x1b[?2026l",
    );
    let (row, col) = g.cursor_position();
    assert!(
        row < rows && col < cols,
        "cursor stays in range: {row}x{col} vs {rows}x{cols}"
    );
}

#[test]
fn dump_dirty_patch_tracks_changed_cell_span() {
    let mut g = DamageGrid::new(3, 12, 100);
    g.process(b"\x1b[1;1Halpha\x1b[2;1Hbeta");
    drop(g.dump_dirty_patch());

    g.process(b"\x1b[2;3HZ");
    let patch = g.dump_dirty_patch();

    assert_eq!(patch.changed_row_count(), 1);
    assert_eq!(patch.changed_cell_count(), 1);
    assert_eq!(
        patch
            .changed_spans()
            .map(|(row, start, cells)| (row, start, cells.len()))
            .collect::<Vec<_>>(),
        [(1, 2, 1)]
    );
}
