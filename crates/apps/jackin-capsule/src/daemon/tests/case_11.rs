// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn feed_while_scrolled_keeps_view_anchored() {
    let (mut session, _rx) = test_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(session.scroll_by(5));
    let offset_before = session.scrollback_offset();
    let top_before = view_row_text(&session, 0);

    for i in 40..45 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }

    assert_eq!(
        session.scrollback_offset(),
        offset_before + 5,
        "offset must grow by the rows evicted into scrollback"
    );
    assert_eq!(
        view_row_text(&session, 0),
        top_before,
        "the row under the reader must hold still while the agent streams"
    );
}

#[test]
fn cursor_reconciliation_hides_cursor_while_scrolled() {
    // Frame-model contract (§3.4): the cursor is hidden whenever the view is
    // not live, re-shown at the VT position when it is — derived per frame,
    // no assertion site outside the encoder.
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let mut mux = single_pane_tab_mux();
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let (mut session, _rx) = test_session(pane.inner.rows, pane.inner.cols);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.session_supervisor.sessions.insert(1, session);
    let live = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    assert!(
        contains(&live, b"\x1b[?25h"),
        "live pane with a visible VT cursor must show it"
    );

    let scrolled = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("wheel into history must repaint");
    assert!(
        scrolled.ends_with(b"\x1b[?25l") || contains(&scrolled, b"\x1b[?25l"),
        "scrolled pane must hide the cursor"
    );
    assert!(
        !scrolled.windows(6).any(|w| w == b"\x1b[?25h"),
        "scrolled pane must not re-show the cursor"
    );

    let back = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        },
    )
    .expect("wheel back to live must repaint");
    assert!(
        contains(&back, b"\x1b[?25h"),
        "returning to live must re-show the cursor"
    );
}

#[test]
fn mode_reconciliation_resets_agent_modes_on_focus_swap() {
    // The reconciliation replaces the focus_swap_reset + current_mode_state
    // pair: swapping focus from a pane with bracketed paste, application
    // cursor, and a kitty push to a plain pane must switch each mode off,
    // while the client-owned mouse/focus/alt-screen modes stay untouched.
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);
    let mut mux = split_tab_mux();
    let panes = mux.visible_panes();
    for pane in &panes {
        let (session, rx) = test_session(pane.inner.rows, pane.inner.cols);
        drop(rx);
        mux.session_supervisor.sessions.insert(pane.id, session);
    }
    mux.session_supervisor
        .sessions
        .get_mut(1)
        .expect("first pane")
        .feed_pty(b"\x1b[?2004h\x1b[?1h\x1b[>1u");
    let asserted = compose_after(&mut mux, FullRedrawReason::FirstAttach);
    for needle in [&b"\x1b[?2004h"[..], &b"\x1b[?1h"[..], &b"\x1b[>1u"[..]] {
        assert!(
            contains(&asserted, needle),
            "focused pane's modes must be asserted: missing {needle:?}"
        );
    }

    let target = panes.iter().find(|pane| pane.id == 2).expect("second pane");
    let swapped = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: target.inner.row + 1,
            col: target.inner.col + 1,
            button: 0,
        },
    )
    .expect("focus swap must repaint");
    for needle in [&b"\x1b[?2004l"[..], &b"\x1b[?1l"[..], &b"\x1b[<u"[..]] {
        assert!(
            contains(&swapped, needle),
            "swap to a plain pane must switch agent modes off: missing {needle:?}"
        );
    }
    for forbidden in [
        &b"\x1b[?1000l"[..],
        &b"\x1b[?1003l"[..],
        &b"\x1b[?1006l"[..],
        &b"\x1b[?1004l"[..],
        &b"\x1b[?1049l"[..],
    ] {
        assert!(
            !contains(&swapped, forbidden),
            "reconciliation must not toggle client-owned mode {forbidden:?}"
        );
    }
}

#[test]
fn scrollbar_click_jumps_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    let filled = session.scrollback_filled();
    assert!(filled > 0);
    mux.session_supervisor.sessions.insert(1, session);
    let pane = mux.visible_panes().into_iter().next().expect("one pane");
    let track_col = pane.outer.col + pane.outer.cols - 1;
    let track_top = pane.outer.row + 1;
    let track_bottom = pane.outer.row + pane.outer.rows - 2;

    // Click the top of the track → jump to the oldest retained rows.
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: track_top,
            col: track_col,
            button: 0,
        },
    );
    assert!(frame.is_some(), "scrollbar jump must repaint");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        filled,
        "top-of-track click must jump to the top of history"
    );

    // Click the bottom of the track → back to the live tail.
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: track_bottom,
            col: track_col,
            button: 0,
        },
    );
    assert!(frame.is_some(), "scrollbar jump back to live must repaint");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        0,
        "bottom-of-track click must return to the live tail"
    );
}

#[test]
fn diff_frames_repaint_in_place_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session(20, 78);
    session.feed_pty(b"hello capsule");
    mux.session_supervisor.sessions.insert(1, session);
    let contains = |frame: &[u8], needle: &[u8]| frame.windows(needle.len()).any(|w| w == needle);

    let first = compose_after(&mut mux, FullRedrawReason::FocusChange);
    let second = compose_after(&mut mux, FullRedrawReason::FocusChange);
    assert!(
        !frame_contains_screen_erase(&first),
        "first diff frame must not erase the screen"
    );
    assert!(
        contains(&first, b"hello") && contains(&first, b"capsule"),
        "first diff frame must emit changed pane cells: {:?}",
        String::from_utf8_lossy(&first)
    );
    assert!(
        !frame_contains_screen_erase(&second),
        "unchanged diff frame must not erase the screen"
    );
    assert!(
        !contains(&second, b"hello") && !contains(&second, b"capsule"),
        "unchanged diff frame must trust Ratatui's previous buffer: {:?}",
        String::from_utf8_lossy(&second)
    );
}

#[test]
fn wheel_noops_for_focused_normal_screen_pane_without_scrollback() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(55, 200);
        let (mut session, mut input_rx) = test_pane_session(51, 198, agent);
        session.feed_pty(b"\x1b[49;3Hcodex prompt");
        assert_eq!(session.scrollback_filled(), 0);
        mux.session_supervisor.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 10,
                col: 10,
                button: 64,
            },
        );

        assert!(
            redraw.is_none(),
            "{pane_kind} normal-screen pane without scrollback should not redraw jackin❯"
        );
        input_rx.try_recv().expect_err(&format!("normal-screen {pane_kind} pane without scrollback must not receive cursor-key wheel fallback"));
        assert_eq!(
            mux.session_supervisor
                .sessions
                .get(1)
                .unwrap()
                .scrollback_offset(),
            0
        );
    }
}

#[test]
fn wheel_scrolls_top_anchored_inline_history_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        feed_top_anchored_inline_history(&mut session, 5, 12);
        session.feed_pty(b"\x1b[8;1Hlive prompt");
        assert!(
            session.scrollback_filled() >= 3,
            "{pane_kind} pane should retain top-anchored inline history"
        );
        mux.session_supervisor.sessions.insert(1, session);

        let redraw = handle_input_frame(
            &mut mux,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );

        let frame = redraw.expect("inline history wheel should redraw");
        input_rx.try_recv().expect_err(&format!(
            "{pane_kind} pane must not receive cursor-key wheel fallback"
        ));
        assert_eq!(
            mux.session_supervisor
                .sessions
                .get(1)
                .unwrap()
                .scrollback_offset(),
            3
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("history"),
            "normal-screen {pane_kind} wheel should render retained inline history"
        );
    }
}

#[test]
fn scrolled_inline_history_preserves_color_and_selection_highlight() {
    let mut mux = single_pane_tab_mux_with_size(12, 40);
    let (mut session, mut input_rx) = test_pane_session(8, 38, Some("codex"));
    session.feed_pty(b"\x1b[1;5r\x1b[5;1H");
    for i in 0..12 {
        session.feed_pty(format!("\r\n\x1b[2K\x1b[31mred history {i}\x1b[0m").as_bytes());
    }
    session.feed_pty(b"\x1b[r\x1b[8;1Hlive prompt");
    mux.session_supervisor.sessions.insert(1, session);

    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("inline history wheel should redraw");

    input_rx
        .try_recv()
        .expect_err("Codex-style inline history scroll must not forward wheel bytes");
    let rendered = String::from_utf8_lossy(&frame);
    assert!(
        rendered.contains("\x1b[38;5;1mred history"),
        "scrolled Codex inline history should preserve red SGR styling: {rendered:?}"
    );

    let inner = mux.visible_panes()[0].inner;
    let session = mux.session_supervisor.sessions.get(1).unwrap();
    let offset = session.scrollback_offset();
    let filled = session.scrollback_filled();
    let top_content_row = filled.saturating_sub(offset);
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: top_content_row,
        anchor_col: 0,
        end_row: top_content_row,
        end_col: 10,
    });
    let selected_frame = compose_after(&mut mux, FullRedrawReason::SelectionRepaint);
    let selected = String::from_utf8_lossy(&selected_frame);
    assert!(
        selected.contains("\x1b[7m\x1b[38;5;1mred history"),
        "selection overlay should repaint scrolled inline history with reverse-video red styling: {selected:?}"
    );
}
