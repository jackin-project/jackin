// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn selection_motion_above_pane_scrolls_into_history() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(session.scrollback_filled() > 0);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 5,
        anchor_col: 0,
        end_row: 5,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::SelectionMotion {
            row: inner.row.saturating_sub(1),
            col: inner.col,
        },
    )
    .expect("selection auto-scroll should repaint");

    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        1,
        "dragging above pane should move selection into retained history"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection edge auto-scroll must not clear the full screen"
    );
    let selection = mux
        .clipboard
        .selection
        .expect("selection should remain active");
    let session = mux.session_supervisor.sessions.get(1).unwrap();
    assert_eq!(
        selection.end_row,
        session
            .scrollback_filled()
            .saturating_sub(session.scrollback_offset()),
        "selection end should clamp to the top visible content row"
    );
}

#[test]
fn selection_motion_below_pane_scrolls_toward_live_tail() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    session.scroll_by(4);
    assert_eq!(
        session.scrollback_offset(),
        4,
        "test setup should start away from the live tail"
    );
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 5,
        anchor_col: 0,
        end_row: 5,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::SelectionMotion {
            row: inner.row.saturating_add(inner.rows),
            col: inner.col,
        },
    )
    .expect("selection auto-scroll should repaint");

    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        3,
        "dragging below pane should move selection toward the live tail"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection edge auto-scroll must not clear the full screen"
    );
    let selection = mux
        .clipboard
        .selection
        .expect("selection should remain active");
    let session = mux.session_supervisor.sessions.get(1).unwrap();
    let prefix = session
        .scrollback_offset()
        .min(session.scrollback_filled())
        .min(inner.rows as usize);
    assert_eq!(
        selection.end_row,
        session
            .scrollback_filled()
            .saturating_add(inner.rows.saturating_sub(1) as usize)
            .saturating_sub(prefix),
        "selection end should clamp to the bottom visible content row"
    );
}

#[test]
fn apply_action_pane_button_motion_updates_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = Rect::new(STATUS_BAR_ROWS + 1, 1, 10, 20);
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 0,
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::PaneButtonMotion {
            row: inner.row + 2,
            col: inner.col + 3,
        },
    )
    .expect("button motion should repaint active selection");

    let selection = mux
        .clipboard
        .selection
        .expect("selection should remain active");
    assert_eq!((selection.end_row, selection.end_col), (2, 3));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection button motion must not clear the full screen"
    );
}

#[test]
fn finalize_selection_keeps_highlight_and_shows_copied_toast() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"copy this text");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    let frame = apply_action_frame(&mut mux, Action::FinalizeSelection)
        .expect("finalizing dragged selection should repaint");

    assert!(
        mux.clipboard.selection.is_some(),
        "copied selection should remain visible"
    );
    assert!(
        mux.clipboard.selection_copied,
        "copied toast state should be active"
    );
    assert!(
        mux.clipboard.selection_copy_feedback_deadline.is_some(),
        "selection copied toast should expire automatically"
    );
    mux.client_registry.client.flush_out_of_band();
    let clipboard = rx.try_recv().expect("selection should write OSC 52");
    assert!(
        clipboard
            .windows(b"\x1b]52;c;".len())
            .any(|w| w == b"\x1b]52;c;"),
        "selection should copy through OSC 52: {clipboard:?}"
    );
    let rendered = String::from_utf8_lossy(&frame);
    assert!(
        !frame_contains_screen_erase(&frame),
        "finalizing selection must not clear the full screen"
    );
    assert!(
        rendered.contains("Selection copied"),
        "copied selection toast should render: {rendered:?}"
    );
    assert!(
        !rendered.contains("selection copied"),
        "copied selection feedback must not replace the action hint row: {rendered:?}"
    );
}

#[test]
fn selection_copy_feedback_expires_without_clearing_highlight() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    mux.clipboard.selection_copied = true;
    let now = Instant::now();
    mux.clipboard.selection_copy_feedback_deadline = Some(now);

    assert!(mux.expire_selection_copy_feedback(now));
    assert!(
        mux.clipboard.selection.is_some(),
        "selection highlight should persist"
    );
    assert!(
        !mux.clipboard.selection_copied,
        "toast should hide after deadline"
    );
    assert!(mux.clipboard.selection_copy_feedback_deadline.is_none());
}

#[test]
fn click_after_copied_selection_clears_highlight() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    mux.clipboard.selection_copied = true;
    drop(compose_after(&mut mux, selection_change_redraw_reason()));

    let frame = apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress {
            row: inner.row,
            col: inner.col,
        },
    )
    .expect("click should clear copied selection");

    assert!(
        mux.clipboard.selection.is_none(),
        "click should clear selection"
    );
    assert!(
        !mux.clipboard.selection_copied,
        "click should clear copied toast"
    );
    assert!(mux.clipboard.selection_copy_feedback_deadline.is_none());
    assert!(
        !frame_contains_screen_erase(&frame),
        "click-clearing selection must not clear the full screen"
    );
    assert!(
        !String::from_utf8_lossy(&frame).contains("Selection copied"),
        "selection toast should disappear after click"
    );
}

#[test]
fn typed_input_after_copied_selection_clears_and_forwards() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    mux.clipboard.selection = Some(SelectionState {
        session_id: 1,
        inner,
        anchor_row: 0,
        anchor_col: 0,
        end_row: 0,
        end_col: 8,
    });
    mux.clipboard.selection_copied = true;

    let frame = apply_action_frame(&mut mux, Action::PaneData(b"x".to_vec()))
        .expect("typing should clear copied selection and repaint");

    assert!(
        mux.clipboard.selection.is_none(),
        "typing should clear selection"
    );
    assert!(
        !mux.clipboard.selection_copied,
        "typing should clear copied toast"
    );
    assert!(mux.clipboard.selection_copy_feedback_deadline.is_none());
    assert_eq!(input_rx.try_recv().unwrap(), b"x");
    assert!(
        !frame_contains_screen_erase(&frame),
        "typing-clearing selection must not clear the full screen"
    );
    assert!(
        !String::from_utf8_lossy(&frame).contains("Selection copied"),
        "selection toast should disappear after typing"
    );
}

#[test]
fn split_close_frame_repaints_in_place_without_screen_erase() {
    // Layout reflow must converge through Ratatui's diff without flashing the
    // screen blank (I4).
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = compose_after(&mut mux, FullRedrawReason::SplitClose);
    assert!(
        !frame.windows(4).any(|w| w == b"\x1b[2J"),
        "SplitClose must repaint in place under the wipe policy (no 2J)"
    );
}
