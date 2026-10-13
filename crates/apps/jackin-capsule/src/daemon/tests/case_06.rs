// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn zoom_state_is_independent_per_tab() {
    let mut mux = split_tab_mux();
    mux.toggle_zoom();
    assert_eq!(mux.active_zoomed_id(), Some(1));

    let mut tab_b = Tab::new_single("Shell", 3, "test-b");
    assert!(tab_b.tree.split_h(3, 4, SplitPosition::After));
    tab_b.focused_id = 4;
    mux.session_supervisor.tabs.push(tab_b);
    mux.session_supervisor.active_tab = 1;
    mux.toggle_zoom();

    assert_eq!(mux.active_zoomed_id(), Some(4));
    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    assert_eq!(mux.session_supervisor.tabs[1].zoomed, Some(4));

    mux.session_supervisor.active_tab = 0;
    assert_eq!(mux.active_zoomed_id(), Some(1));
}

#[test]
fn unzooming_active_tab_does_not_clear_other_tab_zoom() {
    let mut mux = split_tab_mux();
    mux.toggle_zoom();
    let mut tab_b = Tab::new_single("Shell", 3, "test-b");
    assert!(tab_b.tree.split_h(3, 4, SplitPosition::After));
    mux.session_supervisor.tabs.push(tab_b);
    mux.session_supervisor.active_tab = 1;
    mux.toggle_zoom();

    mux.toggle_zoom();

    assert_eq!(mux.session_supervisor.tabs[1].zoomed, None);
    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    mux.session_supervisor.active_tab = 0;
    assert_eq!(mux.active_zoomed_id(), Some(1));
}

#[test]
fn killing_zoomed_pane_clears_only_owning_tab_zoom() {
    let mut mux = split_tab_mux();
    mux.toggle_zoom();
    let mut tab_b = Tab::new_single("Shell", 3, "test-b");
    assert!(tab_b.tree.split_h(3, 4, SplitPosition::After));
    mux.session_supervisor.tabs.push(tab_b);
    mux.session_supervisor.active_tab = 1;
    mux.toggle_zoom();

    mux.close_focused_pane();

    assert_eq!(mux.session_supervisor.tabs[0].zoomed, Some(1));
    assert_eq!(mux.session_supervisor.tabs[1].zoomed, None);
    assert_eq!(mux.session_supervisor.tabs[1].focused_id, 4);
    mux.session_supervisor.active_tab = 0;
    assert_eq!(mux.active_zoomed_id(), Some(1));
}

#[test]
fn resize_zero_zero_normalizes_to_default_dimensions() {
    // A client sending Resize { rows: 0, cols: 0 } is asking for
    // "use the defaults"; the daemon must floor through
    // `normalize_size` and never store 0 in `term_rows`/`term_cols`,
    // because zero-row PTYs collapse grid rendering.
    let mut mux = test_mux(48, 160);
    mux.resize(0, 0);
    assert_eq!(
        (mux.render.term_rows, mux.render.term_cols),
        (DEFAULT_ROWS, DEFAULT_COLS)
    );
}

#[test]
fn resize_then_full_frame_repaints_with_new_geometry() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    assert!(!compose_after(&mut mux, FullRedrawReason::FirstAttach).is_empty());

    mux.resize(30, 100);
    let frame = compose_after(&mut mux, FullRedrawReason::Resize);

    assert_eq!((mux.render.term_rows, mux.render.term_cols), (30, 100));
    assert!(
        !frame.is_empty(),
        "resize must produce a repaint for the attach client"
    );
}

#[test]
fn resize_shrink_terminal_edge_frame_stays_inside_new_geometry() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[1;1HLEFT-EDGE\x1b[1;70HOLD-RIGHT-EDGE\x1b[20;70HOLD-BOTTOM");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.resize(10, 30);
    let frame = compose_after(&mut mux, FullRedrawReason::Resize);

    assert_frame_stays_within_geometry(&frame, 10, 30, "terminal-edge shrink");
}

#[test]
fn resize_shrink_split_frame_stays_inside_new_geometry() {
    let mut mux = split_tab_mux();
    for id in [1, 2] {
        let (mut session, _rx) = test_session(20, 38);
        session.feed_pty(format!("\x1b[1;1HPANE-{id}\x1b[20;30HOLD-SPLIT-{id}").as_bytes());
        mux.session_supervisor.sessions.insert(id, session);
    }
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    mux.resize(10, 40);
    let frame = mux.compose_pending_frame();

    assert_frame_stays_within_geometry(&frame, 10, 40, "interior split shrink");
}

#[test]
fn dialog_dismiss_frame_repaints_covered_pane_body() {
    // Dialog-dismiss ghost regression (PR #495): closing a dialog must repaint
    // the pane cells the backdrop covered, with no 2J clear. The frame
    // apply_action(Dismiss) returns is that repaint — the SocketBackend cell
    // diff turns the backdrop spaces back into pane content, so HELLO-PANE-BODY
    // (one same-style run) reappears and no backdrop ghost survives.
    let needle = b"HELLO-PANE-BODY";
    let contains = |frame: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[1;1HHELLO-PANE-BODY");
    mux.session_supervisor.sessions.insert(1, session);

    let first = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert!(contains(&first), "first frame must paint the pane body");

    // Open a dialog: the full-screen backdrop covers the pane body.
    mux.open_container_info_dialog();
    let opened = compose_after(&mut mux, FullRedrawReason::DialogChange);
    assert!(!contains(&opened), "backdrop must cover the pane body");

    // Dismiss returns the repaint frame directly; it must restore the body.
    let dismissed = apply_action_frame(&mut mux, Action::Dialog(DialogAction::Dismiss))
        .expect("dismiss must emit a repaint frame");
    assert!(!mux.dialog_open(), "Dismiss must pop the dialog");
    assert!(
        contains(&dismissed),
        "dialog dismiss must repaint the covered pane body (no backdrop ghost)"
    );
}

#[test]
fn partial_ratatui_frame_repaints_non_dirty_split_pane_body() {
    // Ratatui draw closures build a fresh current buffer before diffing against
    // the previous one. A partial frame that paints only the dirty pane body can
    // therefore turn every non-dirty split pane into blank cells in the emitted
    // diff. Keep dirty-row patches in the direct backend path only; Ratatui
    // fallback frames must repaint every visible pane body.
    let left_needle = b"LEFT-PANE-STABLE";
    let right_needle = b"RIGHT-PANE-UPDATE";
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let mut mux = split_tab_mux();
    for (id, label) in [(1, "LEFT-PANE-STABLE"), (2, "RIGHT-PANE-STABLE")] {
        let (mut session, _rx) = test_session(20, 38);
        session.feed_pty(format!("\x1b[1;1H{label}").as_bytes());
        mux.session_supervisor.sessions.insert(id, session);
    }
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    // Simulate an invalid/stale Ratatui backing buffer, matching what happens
    // after direct dirty-patch frames or attach-side terminal disruption. The
    // next fallback Ratatui frame must still be self-contained for pane bodies.
    drop(mux.render.ratatui_terminal.clear());
    drop(mux.render.ratatui_terminal.backend_mut().take_output());

    mux.session_supervisor
        .sessions
        .get_mut(2)
        .expect("right pane session")
        .feed_pty(b"\x1b]2;right pane title\x07\x1b[2;1HRIGHT-PANE-UPDATE");
    let frame = compose_after(&mut mux, FullRedrawReason::PtyOutput);

    assert!(
        contains(&frame, left_needle),
        "partial fallback must repaint non-dirty split pane body: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(
        contains(&frame, right_needle),
        "partial fallback must repaint dirty split pane body: {:?}",
        String::from_utf8_lossy(&frame)
    );
}

#[test]
fn bottom_chrome_rides_the_cell_buffer_on_every_frame() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hstable pane");
    // Retain scrollback so the scrolled-chrome step below can park the view
    // in history (the grid clamps the offset to the filled scrollback).
    for i in 0..30 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.session_supervisor.sessions.insert(1, session);

    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let first = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert!(
        contains(&first, b"resize pane"),
        "first full frame must assert raw bottom chrome: {:?}",
        String::from_utf8_lossy(&first)
    );

    // Chrome is widget cells now: Ratatui emits it when it changes, then the
    // previous buffer suppresses unchanged chrome on later frames.
    let unchanged = compose_after(&mut mux, status_change_redraw_reason());
    assert!(
        !contains(&unchanged, b"resize pane"),
        "unchanged chrome cells must not be re-emitted: {:?}",
        String::from_utf8_lossy(&unchanged)
    );
    assert!(
        !contains(&unchanged, b"exit scrollback"),
        "live view must not paint the scrollback hint: {:?}",
        String::from_utf8_lossy(&unchanged)
    );

    assert!(
        mux.session_supervisor
            .sessions
            .get_mut(1)
            .expect("test session")
            .set_scrollback_offset(1)
    );
    let changed = compose_after(&mut mux, FullRedrawReason::ScrollbackMovement);
    assert!(
        contains(&changed, b"scroll") && contains(&changed, b"exit"),
        "changed scrollback chrome must re-emit the hint row: {:?}",
        String::from_utf8_lossy(&changed)
    );
}

#[test]
fn scan_emitted_frame_reports_geometry_fingerprint() {
    // \x1b[2J (erase) + move to (5,10) + move to (40,160).
    let frame = b"\x1b[2J\x1b[5;10Hx\x1b[40;160Hy".to_vec();
    let metrics = scan_emitted_frame(&frame);
    assert_eq!(metrics.cursor_moves, 2);
    assert_eq!(metrics.max_row_addressed, 40);
    assert_eq!(metrics.max_col_addressed, 160);
    assert_eq!(metrics.full_screen_erases, 1);
    assert_eq!(metrics.painted_cells, 2);

    // A move with no col defaults col to 1; `f` is an alias for `H`.
    let frame = b"\x1b[12Hz".to_vec();
    let metrics = scan_emitted_frame(&frame);
    assert_eq!(metrics.cursor_moves, 1);
    assert_eq!(metrics.max_row_addressed, 12);
    assert_eq!(metrics.max_col_addressed, 1);
    assert_eq!(metrics.full_screen_erases, 0);
}

#[test]
fn scan_emitted_frame_counts_modern_render_metrics() {
    let frame = b"\x1b[0m\x1b]8;;https://example.test\x07x\x1b]8;;\x07";
    let metrics = scan_emitted_frame(frame);
    assert_eq!(metrics.bytes, frame.len());
    assert_eq!(metrics.sgr_resets, 1);
    assert_eq!(metrics.osc8_opens, 1);
    assert_eq!(metrics.osc8_closes, 1);
    assert_eq!(metrics.full_screen_erases, 0);
    assert_eq!(metrics.painted_cells, 1);
    assert!(
        !metrics.full_frame_repaint,
        "geometry-free scan should not claim full-frame repaint"
    );
    let full = crate::client_writer::scan_emitted_frame_with_geometry(b"12345678", Some((2, 5)));
    assert!(
        full.full_frame_repaint,
        "painted-cell threshold should flag full-frame repaint"
    );
}

#[test]
fn pty_osc8_hyperlink_emits_from_frame_metadata() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b]8;;https://example.test/docs\x07link\x1b]8;;\x07");
    mux.session_supervisor.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);

    assert!(
        frame
            .windows(b"\x1b]8;;https://example.test/docs\x1b\\".len())
            .any(|w| w == b"\x1b]8;;https://example.test/docs\x1b\\"),
        "safe OSC 8 link must be emitted from frame metadata: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(
        frame.windows(b"link".len()).any(|w| w == b"link"),
        "linked glyphs must still render: {:?}",
        String::from_utf8_lossy(&frame)
    );
}

#[test]
fn unsafe_pty_osc8_hyperlink_is_not_emitted_from_frame_metadata() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b]8;;javascript:alert(1)\x07link\x1b]8;;\x07");
    mux.session_supervisor.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);

    assert!(
        !frame
            .windows(b"javascript".len())
            .any(|w| w == b"javascript"),
        "unsafe OSC 8 URI must not be emitted: {:?}",
        String::from_utf8_lossy(&frame)
    );
    assert!(
        frame.windows(b"link".len()).any(|w| w == b"link"),
        "glyphs must render even when the hyperlink URI is filtered"
    );
}

#[test]
fn pty_sgr_metadata_emits_non_native_visible_attributes() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[4:3;58:2:12:34:56;53mstyled");
    mux.session_supervisor.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    let text = String::from_utf8_lossy(&frame);

    for sgr in ["\x1b[4:3m", "\x1b[58;2;12;34;56m", "\x1b[53m"] {
        assert!(
            text.contains(sgr),
            "SGR metadata {sgr:?} must be emitted in frame: {text:?}"
        );
    }
}

#[test]
fn scroll_region_ops_do_not_emit_decstbm_optimization() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[1;5r\x1b[5;1H");
    for i in 0..8 {
        session.feed_pty(format!("\r\nline {i}").as_bytes());
    }
    session.feed_pty(b"\x1b[r");
    mux.session_supervisor.sessions.insert(1, session);

    let frame = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    let text = String::from_utf8_lossy(&frame);

    assert!(
        !text.contains("\x1b[1;5r") && !text.contains("\x1b[r"),
        "DECSTBM scroll-region optimization must stay disabled: {text:?}"
    );
    assert!(
        !text.contains("\x1b[1S")
            && !text.contains("\x1b[S")
            && !text.contains("\x1b[1T")
            && !text.contains("\x1b[T"),
        "scroll op optimization bytes must stay disabled: {text:?}"
    );
}
