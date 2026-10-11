// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn triple_click_clears_then_two_more_presses_reselect() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"see /model to change");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row;
    let col = inner.col + 6;
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(
        mux.clipboard.selection.is_some(),
        "second press selects the word"
    );

    // Third quick press clears the highlight (and stamps a fresh cycle).
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(mux.clipboard.selection.is_none(), "third press clears");

    // Fourth quick press completes a new double-click on the same word.
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col },
    ));
    assert!(mux.clipboard.selection.is_some(), "fourth press re-selects");

    assert_osc52_payloads(&mut mux, &mut rx, &["/model", "/model"]);
}

#[test]
fn double_click_on_a_second_word_needs_only_two_presses() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"alpha beta");
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let inner = mux.visible_panes()[0].inner;
    let row = inner.row;
    let col_a = inner.col + 1; // inside "alpha"
    let col_b = inner.col + 7; // inside "beta"
    let mut rx = attach_drained_client(&mut mux);

    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_a },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_a },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_b },
    ));
    drop(apply_action_frame(
        &mut mux,
        Action::PanePrimaryPress { row, col: col_b },
    ));

    assert!(mux.clipboard.selection.is_some(), "second word selected");
    assert_osc52_payloads(&mut mux, &mut rx, &["alpha", "beta"]);
}

#[test]
fn session_terminal_carries_the_attached_client_palette() {
    let mut mux = single_pane_tab_mux();
    mux.client_registry.attached_terminal.default_fg = Some((1, 2, 3));
    mux.client_registry.attached_terminal.default_bg = Some((4, 5, 6));
    let terminal = mux.session_terminal(10, 20);
    assert_eq!(terminal.rows, 10);
    assert_eq!(terminal.cols, 20);
    assert_eq!(terminal.default_fg, Some((1, 2, 3)));
    assert_eq!(terminal.default_bg, Some((4, 5, 6)));
}

#[test]
fn reattach_updates_capabilities_without_resetting_model_palette() {
    let mut mux = single_pane_tab_mux();
    let (session, mut rx) = test_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);

    let ghostty = ClientTerminal {
        term: Some("xterm-ghostty".to_owned()),
        colorterm: Some("truecolor".to_owned()),
        default_fg: Some((1, 2, 3)),
        default_bg: Some((4, 5, 6)),
        ..ClientTerminal::default()
    };
    mux.client_registry.attached_capabilities = ghostty.attach_capabilities();
    mux.client_registry.pointer_shapes_supported =
        mux.client_registry.attached_capabilities.pointer_shapes;
    mux.client_registry.attached_terminal = ghostty;
    mux.apply_client_colors_to_sessions();

    let dumb = ClientTerminal {
        term: Some("dumb".to_owned()),
        ..ClientTerminal::default()
    };
    mux.client_registry.attached_capabilities = dumb.attach_capabilities();
    mux.client_registry.pointer_shapes_supported =
        mux.client_registry.attached_capabilities.pointer_shapes;
    mux.client_registry.attached_terminal = dumb;
    mux.apply_client_colors_to_sessions();

    assert!(!mux.client_registry.attached_capabilities.pointer_shapes);
    assert!(!mux.client_registry.pointer_shapes_supported);

    let session = mux.session_supervisor.sessions.get_mut(1).expect("session");
    session.feed_pty(b"\x1b]10;?\x07\x1b]11;?\x07\x1b[6n");
    drop(session.drain_passthrough());
    let replies = vec![
        rx.try_recv().expect("OSC 10 reply"),
        rx.try_recv().expect("OSC 11 reply"),
        rx.try_recv().expect("DSR reply"),
    ];
    assert_eq!(
        replies,
        [
            b"\x1b]10;rgb:0101/0202/0303\x07".to_vec(),
            b"\x1b]11;rgb:0404/0505/0606\x07".to_vec(),
            b"\x1b[1;1R".to_vec(),
        ],
        "reattach without colors must not reset model palette or DSR semantics"
    );
}

#[test]
fn stream_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..60 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
        if i % 13 == 0 {
            assert_frame_conformance(&mut mux, &client, &format!("stream chunk {i}"));
        }
    }
    assert_frame_conformance(&mut mux, &client, "stream end");
}

#[test]
fn full_scroll_cycle_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..60 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    // Wheel up three steps into history.
    for step in 0..3 {
        dispatch_and_compose(
            &mut mux,
            &mut client,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            },
        );
        assert_frame_conformance(&mut mux, &client, &format!("wheel up step {step}"));
    }
    assert_ne!(
        mux.session_supervisor
            .sessions
            .get(sid)
            .unwrap()
            .scrollback_offset(),
        0
    );

    // Stream while scrolled: the anchored view must stay equal to the model.
    for i in 60..70 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "anchored feed while scrolled");

    // Wheel back to the live tail — wheel only.
    while mux
        .session_supervisor
        .sessions
        .get(sid)
        .unwrap()
        .scrollback_offset()
        != 0
    {
        dispatch_and_compose(
            &mut mux,
            &mut client,
            InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 65,
            },
        );
    }
    assert_frame_conformance(&mut mux, &client, "wheel back to live");
}

#[test]
fn focus_swap_mid_stream_keeps_screen_equal_to_model() {
    let mut mux = split_tab_mux();
    let panes = mux.visible_panes();
    assert_eq!(panes.len(), 2);
    for pane in &panes {
        let (session, rx) = test_session_with_agent(
            pane.inner.rows,
            pane.inner.cols,
            Some(format!("agent-{}", pane.id)),
        );
        drop(rx);
        mux.session_supervisor.sessions.insert(pane.id, session);
    }
    let mut client = VirtualClient::new(mux.render.term_rows, mux.render.term_cols);
    mux.invalidate(FullRedrawReason::FirstAttach);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);

    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, panes[0].id, &codex_chunk(i));
        feed_and_compose(
            &mut mux,
            &mut client,
            panes[1].id,
            format!("pane two output {i}\r\n").as_bytes(),
        );
    }
    assert_frame_conformance(&mut mux, &client, "split stream");

    // Click into the second pane mid-stream, then keep streaming.
    let target = &panes[1];
    dispatch_and_compose(
        &mut mux,
        &mut client,
        InputEvent::MousePress {
            row: target.inner.row + 1,
            col: target.inner.col + 1,
            button: 0,
        },
    );
    for i in 20..30 {
        feed_and_compose(&mut mux, &mut client, panes[0].id, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "focus swap mid-stream");
}

#[test]
fn resize_mid_stream_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..30 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.resize(30, 100);
    client.resize(30, 100);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    assert_frame_conformance(&mut mux, &client, "after grow resize");

    for i in 30..40 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }
    assert_frame_conformance(&mut mux, &client, "stream after resize");
}

#[test]
fn dialog_open_close_over_streaming_leaves_no_residue() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.apply_action(Action::OpenGithubContext);
    let frame = mux.compose_pending_frame();
    assert!(!frame.is_empty(), "opening a dialog composes a frame");
    client.apply(&frame);
    assert!(mux.dialog_open());

    // Stream under the open dialog — frames keep flowing.
    for i in 20..30 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    mux.apply_dialog_action(DialogAction::Dismiss);
    let frame = mux.compose_pending_frame();
    client.apply(&frame);
    assert!(!mux.dialog_open());
    assert_frame_conformance(&mut mux, &client, "after dialog close over streaming");
}

#[test]
fn alt_screen_session_enter_exit_keeps_screen_equal_to_model() {
    let (mut mux, mut client, sid) = attached_single_pane();
    for i in 0..20 {
        feed_and_compose(&mut mux, &mut client, sid, &codex_chunk(i));
    }

    // Claude-style alt-screen TUI: enter, paint a frame, exit.
    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[?1049h\x1b[2J\x1b[H");
    feed_and_compose(
        &mut mux,
        &mut client,
        sid,
        b"\x1b[1;1H\x1b[44m claude \x1b[0m\x1b[3;2HWelcome back\x1b[10;2H> ",
    );
    assert_frame_conformance(&mut mux, &client, "alt screen painted");

    feed_and_compose(&mut mux, &mut client, sid, b"\x1b[?1049l");
    assert_frame_conformance(&mut mux, &client, "after alt-screen exit");
}

#[test]
fn recorded_pty_fixtures_keep_screen_equal_to_model() {
    for (label, bytes) in [
        (
            "codex version fixture",
            include_bytes!("../../../tests/fixtures/pty/codex-version.bin").as_slice(),
        ),
        (
            "vim alt-screen fixture",
            include_bytes!("../../../tests/fixtures/pty/vim-tiny-open-edit-quit.bin").as_slice(),
        ),
    ] {
        let (mut mux, mut client, sid) = attached_single_pane();
        feed_and_compose(&mut mux, &mut client, sid, bytes);
        assert_frame_conformance(&mut mux, &client, label);
    }
}
