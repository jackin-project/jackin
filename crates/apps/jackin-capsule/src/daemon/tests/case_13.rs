// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn dialog_copy_hover_uses_overlay_frame_without_screen_erase() {
    let mut mux = single_pane_tab_mux_with_size(32, 100);
    mux.client_registry.pointer_shapes_supported = false;
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));
    drop(
        apply_action_frame(&mut mux, Action::OpenContainerInfo)
            .expect("debug info dialog should render an overlay frame"),
    );

    let (hover_row, hover_col) = {
        let github = mux.github_context_view();
        let dialog = mux.dialog_top().expect("debug info dialog should be open");
        (0..mux.render.term_rows)
            .flat_map(|row| (0..mux.render.term_cols).map(move |col| (row, col)))
            .find(|(row, col)| {
                dialog.clickable_at(
                    row.saturating_add(1),
                    col.saturating_add(1),
                    mux.render.term_rows,
                    mux.render.term_cols,
                    Some(&github),
                )
            })
            .expect("debug info dialog should expose a copyable value")
    };

    let frame = apply_action_frame(
        &mut mux,
        Action::MouseChromeUpdate {
            row: hover_row,
            col: hover_col,
            button: SGR_NO_BUTTON_MOTION,
        },
    )
    .expect("dialog copy hover should repaint the hovered row");
    assert!(
        !frame_contains_screen_erase(&frame),
        "dialog copy hover must not clear the full screen: {:?}",
        String::from_utf8_lossy(&frame)
    );
}

#[test]
fn wheel_scrolls_container_info_dialog_horizontally() {
    let mut mux = single_pane_tab_mux_with_size(24, 80);
    mux.status.status_bar.identity_label =
        "jk-test-container-with-long-debug-value-that-overflows-dialog-width".to_owned();
    mux.open_container_info_dialog();

    let frame = apply_action_frame(
        &mut mux,
        Action::Wheel {
            row: 10,
            col: 10,
            button: 67,
        },
    )
    .expect("horizontal wheel over debug dialog should redraw");

    assert!(!frame.is_empty());
    let Some(Dialog::ContainerInfo { scroll, .. }) = mux.dialog_top() else {
        panic!("container info dialog should remain open");
    };
    assert!(
        scroll.scroll_x > 0,
        "native horizontal touchpad wheel should move the dialog body"
    );
}

#[test]
fn wheel_on_container_info_unsupported_axis_does_not_scroll() {
    let mut mux = single_pane_tab_mux_with_size(40, 160);
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.open_container_info_dialog();

    // The hover pass may invalidate (first pointer position over the
    // dialog), so assert on the scroll state — the wheel on an
    // unsupported axis must not move the body.
    drop(apply_action_frame(
        &mut mux,
        Action::Wheel {
            row: 10,
            col: 10,
            button: 65,
        },
    ));

    let Some(Dialog::ContainerInfo { scroll, .. }) = mux.dialog_top() else {
        panic!("container info dialog should remain open");
    };
    assert_eq!(scroll.scroll_y, 0);
}

#[test]
fn bottom_container_click_opens_container_info_without_copying() {
    let mut mux = test_mux(24, 80);
    mux.client_registry.pointer_shapes_supported = false;
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status.status_bar.instance_id_label = "test".to_owned();
    mux.status.status_bar.role = "the-architect".to_owned();
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
    .and_then(|layout| layout.container)
    .expect("container should fit");

    let press_row = mux.render.term_rows - 1;
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: press_row,
            col: hit.start - 1,
            button: 0,
        },
    )
    .expect("container click should redraw");

    while let Ok(output) = rx.try_recv() {
        assert!(
            !output
                .windows(b"\x1b]52;c;".len())
                .any(|w| w == b"\x1b]52;c;"),
            "opening container info must not send OSC 52"
        );
    }
    assert!(!String::from_utf8_lossy(&frame).contains("Copied!"));
    let Some(Dialog::ContainerInfo {
        copied_row: None,
        workdir,
        ..
    }) = mux.dialog_top()
    else {
        panic!("identity click should open container info")
    };
    assert_eq!(workdir, "/workspace");
}

#[test]
fn bottom_context_click_opens_github_context_dialog() {
    let mut mux = test_mux(24, 100);
    mux.status.status_bar.identity_label = "jk-test-container".to_owned();
    mux.status.status_bar.instance_id_label = "test".to_owned();
    mux.pr_watch.pull_request_context_branch = Some(branch("feature/context"));
    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
    mux.launch_env.workdir_context.gh_available = false;
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
    .expect("GitHub context should fit");

    let press_row = mux.render.term_rows - 1;
    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: press_row,
            col: hit.start - 1,
            button: 0,
        },
    )
    .expect("context click should redraw");

    let rendered = String::from_utf8_lossy(&frame);
    assert!(rendered.contains("GitHub context"));
    assert!(
        rendered.contains("copy GitHub URL"),
        "dialog hint must render with the dialog chrome: {rendered:?}"
    );
    let hint_row = mux.render.term_rows - 2;
    let bottom_row = mux.render.term_rows;
    assert!(
        rendered.contains(&format!("\x1b[{hint_row};")),
        "dialog hint should render in the reserved hint region: {rendered:?}"
    );
    // Outside a debug launch the bottom branch/context bar is hidden under a
    // dialog (commit 5f2076a6); this mux has no debug run id, so the final row
    // must stay clear — only the dialog hint renders below the dialog.
    assert!(
        !rendered.contains(&format!("\x1b[{bottom_row};")),
        "bottom branch/context bar must be hidden under a dialog outside debug: {rendered:?}"
    );
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::GitHubContext { copied: false, .. })
    ));
    assert_eq!(
        mux.pr_watch.pull_request_context_branch.as_deref(),
        Some("feature/context")
    );
    assert_eq!(
        mux.pr_watch
            .pull_request_context
            .as_ref()
            .map(|pr| pr.number),
        Some(434)
    );
}

#[test]
fn container_info_copy_feedback_expires() {
    let mut mux = test_mux(24, 80);
    mux.dialog_push(Dialog::ContainerInfo {
        container_name: "jk-test-container".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace".to_owned(),
        diagnostics: crate::tui::components::dialog::ContainerInfoDiagnostics::default(),
        copied_row: Some(0),
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    });
    let now = Instant::now();
    mux.clipboard.dialog_copy_feedback_deadline = Some(now);

    assert!(mux.expire_dialog_copy_feedback(now));
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            copied_row: None,
            ..
        })
    ));
}

#[test]
fn container_info_id_click_copies_and_renders_feedback() {
    let mut mux = test_mux(40, 120);
    mux.client_registry.pointer_shapes_supported = false;
    mux.dialog_push(Dialog::ContainerInfo {
        container_name: "jk-test-container".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace".to_owned(),
        diagnostics: crate::tui::components::dialog::ContainerInfoDiagnostics::default(),
        copied_row: None,
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    });
    let (tx, mut rx) = mpsc::unbounded_channel();
    mux.client_registry.client.attach(tx);
    let (box_row, box_col, _, _) = mux
        .dialog_top()
        .expect("container info dialog should be open")
        .box_rect(mux.render.term_rows, mux.render.term_cols);

    let frame = handle_input_frame(
        &mut mux,
        InputEvent::MousePress {
            row: box_row + 1,
            // Click the value column (the cyan link), past the widest label.
            col: box_col + 22,
            button: 0,
        },
    )
    .expect("container id click should redraw copy feedback");

    mux.client_registry.client.flush_out_of_band();
    let mut saw_osc52 = false;
    while let Ok(output) = rx.try_recv() {
        saw_osc52 |= output
            .windows(b"\x1b]52;c;".len())
            .any(|w| w == b"\x1b]52;c;");
    }
    assert!(saw_osc52, "copy should emit OSC 52");
    assert!(String::from_utf8_lossy(&frame).contains('✓'));
    assert!(matches!(
        mux.dialog_top(),
        Some(Dialog::ContainerInfo {
            copied_row: Some(0),
            ..
        })
    ));
}

#[test]
fn prefix_ctrl_l_has_named_pane_clear_reason() {
    assert_eq!(
        prefix_full_redraw_reason(&PrefixCommand::ClearPane),
        FullRedrawReason::PaneClear
    );
}

#[test]
fn command_stdout_trimmed_returns_trimmed_stdout() {
    let mut command = Command::new("printf");
    command.arg("  branch-name\n");

    assert_eq!(
        command_stdout_trimmed(&mut command),
        Some("branch-name".to_owned())
    );
}

#[test]
fn command_stdout_trimmed_rejects_known_failure_status() {
    // `sleep 0.05` keeps the child alive long enough for the
    // try_wait poll loop to observe `Ok(None)` first and then the
    // failing `Ok(Some(1))` exit on the next tick. Without the
    // sleep the child can vanish between spawn and the first
    // try_wait, which collapses the Err(ECHILD) "status lost"
    // arm and the Ok(Some(false)) "failed" arm into one path.
    let mut command = Command::new("sh");
    command.args(["-c", "printf branch-name; sleep 0.05; exit 1"]);

    assert_eq!(command_stdout_trimmed(&mut command), None);
}

#[test]
fn gh_lookup_output_rejects_statusless_stderr_only_failure() {
    let err = command_output_or_lookup_error("gh", None, b"", b"HTTP 401: Bad credentials\n")
        .expect_err("stderr-only statusless gh output is a transient failure");

    assert_eq!(err, crate::pr_context::LookupError::Io);
    assert!(!err.to_string().contains("HTTP 401"));
}

#[test]
fn apply_action_dismiss_closes_top_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open(), "palette should be open");
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame = apply_action_frame(&mut mux, Action::Dialog(DialogAction::Dismiss))
        .expect("dialog dismiss should redraw");

    assert!(!mux.dialog_open(), "dismiss should close the dialog");
    assert_eq!(mux.mux_mode(), MuxMode::Normal);
    assert!(
        !frame_contains_screen_erase(&frame),
        "dialog dismiss must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_palette_pushes_palette_dialog() {
    let mut mux = single_pane_tab_mux();
    assert!(!mux.dialog_open());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame =
        apply_action_frame(&mut mux, Action::OpenPalette).expect("open palette should redraw");

    assert!(
        matches!(mux.dialog_top(), Some(Dialog::CommandPalette { .. })),
        "OpenPalette should push CommandPalette dialog"
    );
    assert_eq!(mux.mux_mode(), MuxMode::Dialog);
    assert!(
        !frame_contains_screen_erase(&frame),
        "open palette must not clear the full terminal screen"
    );
}

#[test]
fn apply_action_open_palette_closes_existing_dialog() {
    let mut mux = single_pane_tab_mux();
    mux.open_command_palette();
    assert!(mux.dialog_open());
    drop(compose_after(&mut mux, FullRedrawReason::FirstAttach));

    let frame =
        apply_action_frame(&mut mux, Action::OpenPalette).expect("close palette should redraw");

    assert!(
        !mux.dialog_open(),
        "palette toggle should close open dialog"
    );
    assert_eq!(mux.mux_mode(), MuxMode::Normal);
    assert!(
        !frame_contains_screen_erase(&frame),
        "close palette must not clear the full terminal screen"
    );
}
