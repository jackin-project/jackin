// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn prefix_palette_uses_overlay_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::Palette)
        .expect("prefix palette should redraw command palette");

    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::CommandPalette { .. })
    ));
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix palette must not clear the full terminal screen"
    );
}

#[test]
fn prefix_move_focus_uses_diff_frame_without_screen_erase() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::MoveFocus(ArrowDir::Right))
        .expect("prefix focus move should redraw");

    assert_eq!(
        mux.session_supervisor.tabs[mux.session_supervisor.active_tab].focused_id,
        2
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix focus move must not clear the full terminal screen"
    );
}

#[test]
fn prefix_clear_pane_uses_diff_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::ClearPane)
        .expect("prefix clear-pane should redraw");

    assert_eq!(
        input_rx
            .try_recv()
            .expect("prefix clear-pane should send Ctrl+L"),
        b"\x0c"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix clear-pane must not clear the full terminal screen"
    );
}

#[test]
fn prefix_redraw_repaints_in_place_without_screen_erase() {
    // The explicit-redraw chord must not clear the full terminal; under the
    // wipe policy (I4) only FirstAttach/Resize erase.
    let mut mux = single_pane_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = prefix_command_frame(&mut mux, PrefixCommand::Redraw)
        .expect("prefix redraw should emit a repaint frame");

    assert!(
        !frame_contains_screen_erase(&frame),
        "prefix redraw repaints in place under the wipe policy (no 2J)"
    );
}

#[test]
fn apply_action_focus_pane_at_changes_focus() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    let target = mux
        .visible_panes()
        .into_iter()
        .find(|pane| pane.id == 2)
        .expect("second pane should be visible")
        .inner;

    let frame = apply_action_frame(
        &mut mux,
        Action::FocusPaneAt {
            row: target.row,
            col: target.col,
        },
    )
    .expect("focus change should redraw");

    assert_eq!(
        mux.session_supervisor.tabs[mux.session_supervisor.active_tab].focused_id,
        2
    );
    assert!(!frame.is_empty(), "focus redraw frame should be emitted");
    assert!(
        !frame_contains_screen_erase(&frame),
        "mouse focus change must not clear the full screen"
    );
}

#[test]
fn apply_action_move_focus_uses_diff_frame_without_screen_erase() {
    let mut mux = split_tab_mux();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::MoveFocus(ArrowDir::Right))
        .expect("keyboard focus move should redraw");

    assert_eq!(
        mux.session_supervisor.tabs[mux.session_supervisor.active_tab].focused_id,
        2
    );
    assert!(!frame.is_empty(), "focus redraw frame should be emitted");
    assert!(
        !frame_contains_screen_erase(&frame),
        "keyboard focus change must not clear the full screen"
    );
}

#[test]
fn apply_action_clear_focused_pane_uses_diff_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame =
        apply_action_frame(&mut mux, Action::ClearFocusedPane).expect("clear pane should redraw");

    assert_eq!(
        input_rx.try_recv().expect("clear pane should send Ctrl+L"),
        b"\x0c"
    );
    assert!(
        !frame.is_empty(),
        "clear pane redraw frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "clear pane must not clear the full terminal screen"
    );
}

#[test]
fn palette_clear_pane_uses_diff_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_shell_session(20, 78);
    mux.session_supervisor.sessions.insert(1, session);
    mux.open_command_palette();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::Palette(PaletteCommand::ClearPane))
        .expect("palette clear pane should redraw");

    assert!(!mux.dialog_open(), "palette clear pane should close dialog");
    assert_eq!(
        input_rx.try_recv().expect("clear pane should send Ctrl+L"),
        b"\x0c"
    );
    assert!(
        !frame.is_empty(),
        "clear pane redraw frame should be emitted"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "palette clear pane must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_forward_mouse_sends_to_focused_pane() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    session.feed_pty(b"\x1b[?1003h\x1b[?1006h");
    mux.session_supervisor.sessions.insert(1, session);

    let frame = apply_action_frame(
        &mut mux,
        Action::ForwardMouse {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 0,
            press: true,
        },
    );

    assert!(frame.is_none(), "PTY mouse forward should not redraw");
    assert_eq!(
        input_rx.try_recv().expect("mouse press should reach PTY"),
        b"\x1b[<0;1;1M"
    );
}

#[test]
fn apply_action_dialog_consume_keeps_dialog_open() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    // Consume should leave the dialog open (key was absorbed, no state change).
    let frame = apply_action_frame(&mut mux, Action::Dialog(DialogAction::Consume))
        .expect("dialog consume should redraw");

    assert!(mux.dialog_open(), "Consume must not close the dialog");
    assert!(
        !frame_contains_screen_erase(&frame),
        "dialog consume must not clear the full terminal screen"
    );
}

#[test]
fn account_model_is_used_for_agent_launch() {
    let mut mux = test_mux(24, 80);
    mux.launch_env
        .launch_config
        .models
        .insert("work@opencode".to_owned(), "minimax/custom".to_owned());
    assert_eq!(
        mux.launch_env
            .launch_config
            .model_for_instance("work@opencode"),
        Some("minimax/custom")
    );
}

#[test]
fn env_for_spawn_keeps_allowlisted_drops_unknown() {
    let mux = test_mux(24, 80);
    let env = mux.env_for_spawn(&[
        ("TZ".to_owned(), "UTC".to_owned()),
        ("TOTALLY_NOT_ALLOWLISTED".to_owned(), "x".to_owned()),
    ]);
    assert!(
        env.iter().any(|(k, v)| k == "TZ" && v == "UTC"),
        "TZ must survive the passthrough allowlist"
    );
    assert!(
        !env.iter().any(|(k, _)| k == "TOTALLY_NOT_ALLOWLISTED"),
        "non-allowlisted keys must be dropped"
    );
}

#[test]
fn apply_action_dialog_click_routes_to_dialog_handler() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());

    mux.apply_action(Action::DialogClick { row: 0, col: 0 });

    assert!(!mux.dialog_open(), "outside click should dismiss dialog");
}

#[test]
fn apply_action_focus_report_does_not_open_dialog() {
    let mut mux = single_pane_tab_mux();
    assert!(!mux.dialog_open());

    mux.apply_action(Action::FocusReport(true));
    assert!(!mux.dialog_open());

    mux.apply_action(Action::FocusReport(false));
    assert!(!mux.dialog_open());
}

#[test]
fn apply_action_mouse_chrome_update_sets_pointer_shape() {
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

    mux.apply_action(Action::MouseChromeUpdate {
        row: 0,
        col: tab_col,
        button: SGR_NO_BUTTON_MOTION,
    });

    mux.client_registry.client.flush_out_of_band();
    let mut outputs = Vec::new();
    while let Ok(output) = rx.try_recv() {
        outputs.push(output);
    }
    let frame: Vec<u8> = outputs.iter().flatten().copied().collect();
    assert!(
        !frame.is_empty(),
        "mouse chrome action should emit hover repaint and pointer shape update"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "mouse chrome hover must not clear the full screen"
    );
    assert!(
        outputs
            .iter()
            .any(|output| output.ends_with(b"\x1b]22;pointer\x1b\\")),
        "mouse chrome action should emit pointer shape update"
    );
}

#[test]
fn apply_action_wheel_scrolls_scrollback() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(
        &mut mux,
        Action::Wheel {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        },
    )
    .expect("wheel over retained scrollback should redraw");

    input_rx
        .try_recv()
        .expect_err("mouse-disabled pane must not receive raw wheel bytes");
    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        3
    );
    assert!(
        !frame.is_empty(),
        "scrollback redraw frame should be emitted"
    );
    assert!(
        !frame.windows(b"\x1b[2J".len()).any(|w| w == b"\x1b[2J"),
        "scrollback wheel movement should diff the pane instead of clearing the full terminal"
    );
}

#[test]
fn typed_input_snaps_scrollback_to_live_without_screen_erase() {
    let mut mux = single_pane_tab_mux();
    let (mut session, mut input_rx) = test_shell_session(20, 78);
    for i in 0..40 {
        session.feed_pty(format!("line {i}\r\n").as_bytes());
    }
    session.scroll_by(3);
    assert_eq!(session.scrollback_offset(), 3);
    mux.session_supervisor.sessions.insert(1, session);
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::PaneData(b"x".to_vec()))
        .expect("typing while viewing scrollback should snap to live and repaint");

    assert_eq!(
        mux.session_supervisor
            .sessions
            .get(1)
            .unwrap()
            .scrollback_offset(),
        0,
        "typing should return the pane to the live tail"
    );
    assert_eq!(input_rx.try_recv().unwrap(), b"x");
    assert!(
        !frame.is_empty(),
        "scrollback snap repaint should emit a frame"
    );
    assert!(
        !frame_contains_screen_erase(&frame),
        "typing scrollback snap must not clear the full screen"
    );
}
