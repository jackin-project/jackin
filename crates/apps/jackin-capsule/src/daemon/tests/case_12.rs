// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn wheel_scrolls_normal_screen_history_preserved_before_clear_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        for i in 0..5 {
            session.feed_pty(format!("release note {i}\r\n").as_bytes());
        }
        assert_eq!(
            session.scrollback_filled(),
            0,
            "{pane_kind} setup output fits without native scrollback before clear"
        );

        session.feed_pty(b"\x1b[1;1H\x1b[Jlive prompt");
        assert!(
            session.scrollback_filled() >= 5,
            "{pane_kind} pane should preserve normal-screen rows erased by clear/redraw"
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

        let frame = redraw.expect("clear-preserved history wheel should redraw");
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
            String::from_utf8_lossy(&frame).contains("release"),
            "normal-screen {pane_kind} wheel should render rows preserved before clear"
        );
    }
}

#[test]
fn wheel_scrolls_csi_scroll_up_inline_history_for_all_panes() {
    for (agent, pane_kind) in pane_kind_cases() {
        let mut mux = single_pane_tab_mux_with_size(12, 40);
        let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
        session.feed_pty(b"\x1b[1;5r\x1b[1;1Htop row\x1b[2;1Hsecond row\x1b[3;1Hthird row");
        session.feed_pty(b"\x1b[2S\x1b[r\x1b[8;1Hlive prompt");
        assert!(
            session.scrollback_filled() >= 2,
            "{pane_kind} pane should retain rows removed by top-anchored CSI S"
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

        let frame = redraw.expect("CSI S inline history wheel should redraw");
        input_rx.try_recv().expect_err(&format!(
            "{pane_kind} pane must not receive cursor-key wheel fallback"
        ));
        assert_eq!(
            mux.session_supervisor
                .sessions
                .get(1)
                .unwrap()
                .scrollback_offset(),
            2
        );
        assert!(
            String::from_utf8_lossy(&frame).contains("top"),
            "normal-screen {pane_kind} wheel should render CSI S retained history"
        );
    }
}

#[test]
fn wheel_sends_cursor_fallback_to_mouse_disabled_alt_screen_tui() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1049h");
    mux.session_supervisor.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    );

    assert!(
        redraw.is_none(),
        "pane-owned fallback should not redraw jackin❯"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        0
    );
}

#[test]
fn wheel_sends_cursor_fallback_to_alt_screen_tui_with_retained_primary_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    assert!(
        session.scrollback_filled() > 0,
        "setup should leave retained primary-screen scrollback"
    );
    session.feed_pty(b"\x1b[?1049h");
    assert!(
        session.alternate_screen(),
        "setup should leave pane in the alternate screen"
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

    assert!(
        redraw.is_none(),
        "alternate-screen fallback should not redraw jackin❯"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        0
    );
}

#[test]
fn wheel_cursor_fallback_respects_application_cursor_mode() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_session(20, 78);
    session.feed_pty(b"\x1b[?1049h\x1b[?1h");
    mux.session_supervisor.sessions.insert(1, session);

    let redraw = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        },
    );

    assert!(
        redraw.is_none(),
        "pane-owned fallback should not redraw jackin❯"
    );
    assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1bOB\x1bOB\x1bOB");
}

#[test]
fn alt_screen_exit_resets_keyboard_modes_for_shell_prompt() {
    let (mut session, _input_rx) = test_session(8, 20);
    session.feed_pty(b"\x1b[?1049h\x1b[>1u\x1b[>4;2m");
    drop(session.drain_passthrough());

    session.feed_pty(b"\x1b[?1049l");
    let drained = session.drain_passthrough();

    assert!(
        drained.iter().any(|bytes| bytes == b"\x1b[<u"),
        "kitty keyboard reset missing from {drained:?}"
    );
    assert!(
        drained.iter().any(|bytes| bytes == b"\x1b[>4;0m"),
        "modifyOtherKeys reset missing from {drained:?}"
    );
}

#[test]
fn pointer_shape_updates_only_when_shape_changes() {
    let mut mux = test_mux(24, 80);
    mux.client_registry.pointer_shapes_supported = true;
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status.status_bar.instance_id_label = "test".to_owned();
    mux.pr_watch.pull_request_context_branch = Some(branch("feature/context"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    let hit = branch_context_bar_layout(
        mux.render.term_rows,
        mux.render.term_cols,
        mux.pr_watch.pull_request_context_branch.as_deref(),
        None,
        mux.pr_watch.pull_request_context.as_deref(),
        mux.pull_request_context_loading(),
        None,
        mux.status.status_bar.instance_id_label(),
    )
    .and_then(|layout| layout.left)
    .expect("branch context should fit");

    mux.update_pointer_shape_for_mouse(23, hit.start - 1, SGR_NO_BUTTON_MOTION);
    mux.client_registry.client.flush_out_of_band();
    let first = rx.try_recv().expect("first pointer-shape update");
    assert!(first.ends_with(b"\x1b]22;pointer\x1b\\"));

    mux.update_pointer_shape_for_mouse(23, hit.start, SGR_NO_BUTTON_MOTION);
    mux.client_registry.client.flush_out_of_band();
    rx.try_recv()
        .expect_err("unchanged shape should not re-emit");
}

#[tokio::test]
async fn drain_and_exit_delivers_shutdown_before_closing_attach_socket() {
    let mut mux = test_mux(24, 80);
    let (daemon_stream, mut client_stream) = UnixStream::pair().unwrap();
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (_completion_tx, completion_rx) = mpsc::unbounded_channel();
    let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(out_tx);
    mux.client_registry.attached_task = Some(tokio::spawn(handle_attach_client_with_handshake(
        daemon_stream,
        out_rx,
        completion_rx,
        cmd_tx,
        None,
    )));

    let read_shutdown = async {
        let mut tag = [0u8; 1];
        client_stream
            .read_exact(&mut tag)
            .await
            .expect("shutdown tag should be readable");
        read_server_frame(&mut client_stream, tag[0])
            .await
            .expect("shutdown frame should decode")
            .expect("shutdown frame should be present")
    };

    let ((), frame) = tokio::join!(drain_and_exit(&mut mux), read_shutdown);
    assert_eq!(frame, ServerFrame::Shutdown { reason: None });
}

#[test]
fn pointer_shape_updates_for_clickable_top_chrome() {
    let mut mux = single_pane_tab_mux();
    mux.client_registry.pointer_shapes_supported = true;
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    let tab_col = mux
        .status
        .status_bar
        .tab_regions
        .first()
        .map(|(start, _)| start.saturating_sub(1))
        .expect("tab region should render");

    mux.update_pointer_shape_for_mouse(0, tab_col, SGR_NO_BUTTON_MOTION);
    mux.client_registry.client.flush_out_of_band();
    let tab_shape = rx.try_recv().expect("tab pointer-shape update");
    assert!(tab_shape.ends_with(b"\x1b]22;pointer\x1b\\"));

    let mut mux = single_pane_tab_mux();
    mux.client_registry.pointer_shapes_supported = true;
    drop(compose_after(&mut mux, FullRedrawReason::ExplicitRedraw));
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    let menu_col = mux
        .status
        .status_bar
        .hint_region
        .map(|(start, _)| start.saturating_sub(1))
        .expect("menu region should render");

    mux.update_pointer_shape_for_mouse(0, menu_col, SGR_NO_BUTTON_MOTION);
    mux.client_registry.client.flush_out_of_band();
    let menu_shape = rx.try_recv().expect("menu pointer-shape update");
    assert!(menu_shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_clickable_dialog_copy_target() {
    let mut mux = single_pane_tab_mux();
    mux.client_registry.pointer_shapes_supported = true;
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.open_container_info_dialog();
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    let dialog = mux.dialog_top().expect("container info dialog should open");
    let (row, col, _, _) = dialog.box_rect(mux.render.term_rows, mux.render.term_cols);

    mux.update_pointer_shape_for_mouse(
        row.saturating_add(1),
        // Hover the value column (the cyan link), past the widest label.
        col.saturating_add(22),
        SGR_NO_BUTTON_MOTION,
    );
    mux.client_registry.client.flush_out_of_band();
    let shape = rx.try_recv().expect("dialog pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_modified_link_hover() {
    let mut mux = single_pane_tab_mux();
    mux.client_registry.pointer_shapes_supported = true;
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[1;1Hvisit https://example.com/visible now");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.update_pointer_shape_for_mouse(inner.row, inner.col + 7, 43);
    mux.client_registry.client.flush_out_of_band();
    let shape = rx.try_recv().expect("link hover pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}

#[test]
fn pointer_shape_updates_for_usage_dialog_tabs() {
    let mut mux = single_pane_tab_mux();
    mux.client_registry.pointer_shapes_supported = true;
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("seed", 1);
    view.focused_provider = Some("OpenAI".to_owned());
    view.tabs = vec![jackin_protocol::control::UsageProviderTab {
        id: "test-tab-openai".to_owned(),
        label: "OpenAI".to_owned(),
        status_label: "usage unavailable".to_owned(),
        account_label: "seed".to_owned(),
        plan_label: None,
        source_label: None,
        active: true,
    }];
    mux.dialog_push(Dialog::new_usage(view.clone()));
    let dialog = mux.dialog_top().expect("usage dialog should open");
    let (row, col, rows, cols) = dialog.box_rect(mux.render.term_rows, mux.render.term_cols);
    let area = ratatui::layout::Rect {
        x: col,
        y: row,
        width: cols,
        height: rows,
    };
    let inner = crate::tui::components::dialog_widgets::usage_dialog_inner_area(area);
    let tabs = crate::tui::components::dialog_widgets::usage_tab_strip_labels(
        &view,
        crate::tui::components::dialog::UsageDialogTab::Provider,
    );
    let tab_area = crate::tui::components::dialog_widgets::usage_tab_strip_area(inner, &tabs);
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);

    mux.update_pointer_shape_for_mouse(tab_area.y, tab_area.x, SGR_NO_BUTTON_MOTION);
    mux.client_registry.client.flush_out_of_band();

    let shape = rx.try_recv().expect("usage tab pointer-shape update");
    assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
}
