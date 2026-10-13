// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn image_path_paste_uses_plain_bytes_when_bracketed_paste_is_off() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);

    assert!(mux.paste_text_to_focused_pane(b"/jackin/run/clipboard/clipboard-test.png"));

    assert_eq!(
        input_rx.try_recv().unwrap(),
        b"/jackin/run/clipboard/clipboard-test.png"
    );
    input_rx
        .try_recv()
        .expect_err("plain paste should not produce extra PTY input");
}

#[test]
fn image_path_paste_uses_bracketed_paste_when_enabled() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?2004h");
    assert!(
        session.bracketed_paste(),
        "test session should track bracketed-paste mode"
    );
    mux.session_supervisor.sessions.insert(1, session);

    assert!(mux.paste_text_to_focused_pane(b"/jackin/run/clipboard/clipboard-test.png"));

    assert_eq!(
        input_rx.try_recv().unwrap(),
        b"\x1b[200~/jackin/run/clipboard/clipboard-test.png\x1b[201~"
    );
    input_rx
        .try_recv()
        .expect_err("bracketed paste should be one PTY input chunk");
}

#[test]
fn image_path_paste_reports_missing_focused_session() {
    let mut mux = single_pane_tab_mux();

    assert!(!mux.paste_text_to_focused_pane(b"/jackin/run/clipboard/clipboard-test.png"));
}

#[test]
fn apply_action_wheel_noops_at_scrollback_boundary() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..25 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    let filled = session.scrollback_filled();
    assert!(filled > 0, "setup should retain scrollback");
    mux.session_supervisor.sessions.insert(1, session);

    let mut last = Some(Vec::new());
    for _ in 0..(filled + 2) {
        last = apply_action_frame(
            &mut mux,
            Action::Wheel {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );
        if last.is_none() {
            break;
        }
    }

    input_rx
        .try_recv()
        .expect_err("mouse-disabled pane must not receive raw wheel bytes");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        filled
    );
    assert!(
        last.is_none(),
        "wheel event at max scrollback offset should not redraw"
    );
}

#[test]
fn apply_action_end_drag_resize_clears_drag_state() {
    let mut mux = single_pane_tab_mux();
    mux.render.drag = Some(DragState {
        tab_idx: 0,
        path: Vec::new(),
        orient: SplitOrient::Horizontal,
        rect: Rect::new(
            STATUS_BAR_ROWS,
            0,
            mux.render.content_rows,
            mux.render.term_cols,
        ),
    });

    let frame = apply_action_frame(&mut mux, Action::EndDragResize)
        .expect("ending drag should redraw layout");

    assert!(mux.render.drag.is_none(), "drag state should be cleared");
    assert!(!frame.is_empty(), "layout redraw frame should be emitted");
}

#[test]
fn apply_action_mouse_release_ends_drag_resize() {
    let mut mux = single_pane_tab_mux();
    mux.render.drag = Some(DragState {
        tab_idx: 0,
        path: Vec::new(),
        orient: SplitOrient::Horizontal,
        rect: Rect::new(
            STATUS_BAR_ROWS,
            0,
            mux.render.content_rows,
            mux.render.term_cols,
        ),
    });

    let frame = apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row: STATUS_BAR_ROWS,
            col: 1,
            button: 0,
        },
    )
    .expect("left-button release should redraw layout after drag");

    assert!(mux.render.drag.is_none(), "drag state should be cleared");
    assert!(!frame.is_empty(), "layout redraw frame should be emitted");
}

#[test]
fn apply_action_start_drag_resize_sets_drag_state() {
    let mut mux = split_tab_mux();
    let (row, col) = (0..mux.render.term_rows)
        .flat_map(|row| (0..mux.render.term_cols).map(move |col| (row, col)))
        .find(|(row, col)| mux.detect_drag_start(*row, *col).is_some())
        .expect("split tab should expose a draggable border");

    let frame = apply_action_frame(&mut mux, Action::StartDragResize { row, col });

    assert!(frame.is_none(), "drag start should not redraw yet");
    assert!(mux.render.drag.is_some(), "drag state should be active");
}

#[test]
fn apply_action_pane_primary_press_starts_drag_on_border() {
    let mut mux = split_tab_mux();
    let (row, col) = (0..mux.render.term_rows)
        .flat_map(|row| (0..mux.render.term_cols).map(move |col| (row, col)))
        .find(|(row, col)| mux.detect_drag_start(*row, *col).is_some())
        .expect("split tab should expose a draggable border");

    let frame = apply_action_frame(&mut mux, Action::PanePrimaryPress { row, col });

    assert!(frame.is_none(), "drag start should not redraw yet");
    assert!(mux.render.drag.is_some(), "drag state should be active");
}

#[test]
fn apply_action_pane_primary_press_only_arms_selection_for_shell() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
        },
    );

    input_rx
        .try_recv()
        .expect_err("mouse-disabled pane should arm selection instead of receiving raw mouse");
    assert!(
        mux.clipboard.selection.is_none(),
        "plain press should not select yet"
    );
    assert!(
        mux.clipboard.pending_selection.is_some(),
        "selection should be pending until drag motion"
    );
    assert!(
        frame.is_none(),
        "arming selection should not repaint or flash selection chrome"
    );
}

#[test]
fn pane_button_motion_promotes_pending_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let press_row = STATUS_BAR_ROWS + 1;
    let press_col = 1;
    assert!(
        apply_action_frame(
            &mut mux,
            Action::PanePrimaryPress {
                row: press_row,
                col: press_col,
            }
        )
        .is_none()
    );

    let frame = apply_action_frame(
        &mut mux,
        Action::PaneButtonMotion {
            row: press_row + 1,
            col: press_col + 2,
        },
    )
    .expect("drag motion should promote pending selection and repaint");

    assert!(mux.clipboard.pending_selection.is_none());
    let selection = mux
        .clipboard
        .selection
        .expect("selection should be active after drag");
    assert_eq!((selection.anchor_row, selection.anchor_col), (0, 0));
    assert_eq!((selection.end_row, selection.end_col), (1, 2));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection start must not clear the full screen"
    );
}

#[test]
fn mouse_release_without_drag_clears_pending_selection() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let row = STATUS_BAR_ROWS + 1;
    let col = 1;
    assert!(apply_action_frame(&mut mux, Action::PanePrimaryPress { row, col }).is_none());

    let frame = apply_action_frame(
        &mut mux,
        Action::MouseRelease {
            row,
            col,
            button: 0,
        },
    );

    assert!(frame.is_none(), "plain click release should not repaint");
    assert!(mux.clipboard.pending_selection.is_none());
    assert!(
        mux.clipboard.selection.is_none(),
        "plain click must not leave selection"
    );
}

#[test]
fn apply_action_start_selection_sets_selection_state() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::StartSelection {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
        },
    )
    .expect("selection start should repaint");

    let selection = mux.clipboard.selection.expect("selection should be active");
    assert_eq!((selection.anchor_row, selection.anchor_col), (0, 0));
    assert!(
        !frame.is_empty(),
        "selection repaint frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "selection start must not clear the full screen"
    );
}

#[test]
fn apply_action_selection_motion_updates_selection() {
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
        Action::SelectionMotion {
            row: inner.row + 2,
            col: inner.col + 3,
        },
    )
    .expect("selection motion should redraw");

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
        "selection motion must not clear the full screen"
    );
}
