// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn build_log_mouse_wheel_scrolls_tail_when_vertical_bar_visible() {
    let mut view = crate::tui::update::initial_view();
    view.build_log_lines = (0..30).map(|idx| format!("line {idx}")).collect();
    let area = Rect::new(0, 0, 40, 8);

    assert!(update_build_log_mouse_scroll(
        &mut view,
        area,
        MouseEventKind::ScrollUp,
        KeyModifiers::NONE,
    ));

    assert_eq!(view.build_log_scroll.offset(), BUILD_LOG_SCROLL_STEP);
}

#[test]
fn build_log_mouse_wheel_ignores_axes_without_visible_scrollbar() {
    let mut view = crate::tui::update::initial_view();
    view.build_log_lines = vec!["short".to_owned()];
    let area = Rect::new(0, 0, 40, 8);

    assert!(!update_build_log_mouse_scroll(
        &mut view,
        area,
        MouseEventKind::ScrollUp,
        KeyModifiers::NONE,
    ));
    assert!(!update_build_log_mouse_scroll(
        &mut view,
        area,
        MouseEventKind::ScrollRight,
        KeyModifiers::NONE,
    ));

    assert_eq!(view.build_log_scroll.offset(), 0);
}

#[test]
fn build_log_mouse_debug_telemetry_does_not_escape_to_compact_output() {
    let terminal = RecordingTerminal::new();
    let mut view = crate::tui::update::initial_view();
    view.build_log_open = true;
    let mut last_cell = None;

    emit_dialog_mouse_debug_telemetry(
        &terminal,
        view.container_info_open,
        view.build_log_open,
        &mut last_cell,
        crossterm::event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        },
    );

    assert!(
        terminal.compact().is_empty(),
        "mouse telemetry must not write operator-visible compact lines"
    );
    let debug = terminal.debug();
    assert_eq!(debug.len(), 1);
    assert_eq!(debug[0].0, "cockpit-dialog-mouse");
    assert!(debug[0].1.contains("build_log_open=true"));
}

#[test]
fn dialog_mouse_debug_telemetry_coalesces_same_cell_moves() {
    assert!(should_emit_dialog_mouse(
        MouseEventKind::Moved,
        None,
        (10, 10)
    ));
    assert!(!should_emit_dialog_mouse(
        MouseEventKind::Moved,
        Some((10, 10)),
        (10, 10)
    ));
    assert!(should_emit_dialog_mouse(
        MouseEventKind::Moved,
        Some((10, 10)),
        (11, 10)
    ));
    assert!(should_emit_dialog_mouse(
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        Some((10, 10)),
        (10, 10)
    ));
}

#[test]
fn failure_popup_outside_click_acknowledges() {
    let mut view = crate::tui::update::initial_view();
    view.failure = Some(failure_failure());
    let area = Rect::new(0, 0, 96, 24);
    let terminal = RecordingTerminal::new();
    let ctx = CockpitContext {
        area,
        run_id: "jk-run-test",
        terminal: &terminal,
        jackin_version: "jackin 0.0.0-test",
    };

    // The top-left corner is outside the centered popup → acknowledge failure
    // through the same path as Enter/Esc (FailureAcknowledged).
    handle_cockpit_mouse_down(&mut view, ctx, 1, 1);

    assert!(
        view.failure_ack,
        "outside click must acknowledge the failure popup"
    );
}

#[test]
fn failure_popup_inside_non_target_click_is_swallowed() {
    let mut view = crate::tui::update::initial_view();
    view.failure = Some(failure_failure());
    // A build-log line is present so we can prove the swallowed click does not
    // fall through to build-log/container-info behavior.
    view.build_log_lines = vec!["short".to_owned()];
    let area = Rect::new(0, 0, 96, 24);
    let terminal = RecordingTerminal::new();
    let ctx = CockpitContext {
        area,
        run_id: "jk-run-test",
        terminal: &terminal,
        jackin_version: "jackin 0.0.0-test",
    };

    let failure = view.failure.as_ref().expect("failure set");
    let rect = failure_popup_block_rect(area, failure, "jk-run-test", true);
    // A point inside the popup border (top border row) that is not a copyable
    // value cell: it must be swallowed, not acknowledged and not routed to a
    // background overlay.
    let inside_col = rect.x.saturating_add(2);
    let inside_row = rect.y;
    handle_cockpit_mouse_down(&mut view, ctx, inside_col, inside_row);

    assert!(
        !view.failure_ack,
        "inside non-target click must not acknowledge the failure"
    );
    assert!(
        !view.build_log_open,
        "inside non-target click must not open the build-log overlay"
    );
    assert!(
        view.failure.is_some(),
        "failure popup must stay open after a swallowed inside click"
    );
}

#[test]
fn build_log_body_click_is_swallowed() {
    let mut view = crate::tui::update::initial_view();
    view.build_log_open = true;
    view.build_log_lines = vec!["short".to_owned()];
    let area = Rect::new(0, 0, 80, 24);
    let terminal = crate::fixtures::test_host_terminal();

    handle_cockpit_mouse_down(
        &mut view,
        CockpitContext {
            area,
            run_id: "jk-run-test",
            terminal,
            jackin_version: "jackin 0.0.0-test",
        },
        2,
        2,
    );

    assert!(view.build_log_open);
    assert!(!view.build_log_scroll_dragging);
}

#[test]
fn container_info_click_copies_real_run_id() {
    let mut view = crate::tui::update::initial_view();
    view.container_info_open = true;
    let area = Rect::new(0, 0, 96, 24);
    let run_id = "jk-run-test";
    let terminal = RecordingTerminal::new();
    let ctx = CockpitContext {
        area,
        run_id,
        terminal: &terminal,
        jackin_version: "jackin 0.0.0-test",
    };

    let state = launch_container_info_state(&view, run_id, true, "jackin 0.0.0-test");
    let (run_col, run_row) = hit_point_for_payload(area, &state, run_id);
    handle_cockpit_mouse_down(&mut view, ctx, run_col, run_row);

    assert_eq!(terminal.copied(), vec![run_id.to_owned()]);
}

#[test]
fn is_ctrl_c_only_matches_ctrl_c() {
    assert!(is_ctrl_c(&Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL
    ))));
    // Ctrl+Q is not a hard cancel — it must not match.
    assert!(!is_ctrl_c(&Event::Key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL
    ))));
    // Plain 'c' (no modifier) is just input.
    assert!(!is_ctrl_c(&Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::NONE
    ))));
}

#[test]
fn quit_confirm_yes_confirms_and_closes() {
    let mut view = quit_confirm_view();
    let out = apply_quit_confirm_key(
        &mut view,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
    );
    assert_eq!(out, QuitConfirmOutcome::Confirmed);
    assert!(view.quit_confirm.is_none(), "confirm closes on Yes");
}

#[test]
fn confirmed_quit_maps_to_hard_exit() {
    assert_eq!(
        cockpit_outcome_for_quit_confirm(QuitConfirmOutcome::Confirmed),
        CockpitOutcome::HardExit
    );
}

#[test]
fn quit_confirm_esc_dismisses_and_closes() {
    let mut view = quit_confirm_view();
    let out = apply_quit_confirm_key(&mut view, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(out, QuitConfirmOutcome::Dismissed);
    assert!(view.quit_confirm.is_none(), "Esc dismisses");
}

#[test]
fn quit_confirm_enter_confirms_exit_prompt() {
    // The exit confirmation is the one Confirm variant whose default focus is
    // Yes; destructive confirmations still default to No.
    let mut view = quit_confirm_view();
    let out = apply_quit_confirm_key(&mut view, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(out, QuitConfirmOutcome::Confirmed);
}

#[test]
fn quit_confirm_focus_toggle_keeps_dialog_open() {
    let mut view = quit_confirm_view();
    let out = apply_quit_confirm_key(&mut view, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(out, QuitConfirmOutcome::Pending);
    assert!(view.quit_confirm.is_some(), "Tab only toggles focus");
    // After toggling away from the exit prompt's focused Yes, Enter dismisses.
    let out = apply_quit_confirm_key(&mut view, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(out, QuitConfirmOutcome::Dismissed);
}

#[test]
fn cockpit_actions_are_exhaustively_semantic() {
    use crate::tui::keymap::CockpitAction;
    use jackin_telemetry::schema::enums::UiActionName;

    assert_eq!(
        cockpit_action_name(CockpitAction::HardExit),
        UiActionName::AppExitRequest
    );
    assert_eq!(
        cockpit_action_name(CockpitAction::OpenQuitConfirm),
        UiActionName::AppExitRequest
    );
}

#[test]
fn build_log_actions_classify_close_only() {
    use crate::tui::keymap::BuildLogAction;
    use jackin_telemetry::schema::enums::UiActionName;

    assert_eq!(
        build_log_action_name(BuildLogAction::Close),
        Some(UiActionName::DialogCancel)
    );
    for action in [
        BuildLogAction::ScrollUp,
        BuildLogAction::ScrollDown,
        BuildLogAction::PageUp,
        BuildLogAction::PageDown,
    ] {
        assert_eq!(build_log_action_name(action), None);
    }
}
