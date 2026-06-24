use std::sync::Mutex;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::layout::Rect;

use super::{
    BUILD_LOG_SCROLL_STEP, CockpitContext, QuitConfirmOutcome, apply_quit_confirm_key,
    cockpit_outcome_for_quit_confirm, handle_cockpit_mouse_down, is_ctrl_c,
    update_build_log_mouse_scroll,
};
use crate::LaunchHostTerminal;
use crate::tui::components::container_info_dialog::{
    launch_container_info_rect, launch_container_info_state,
};

struct RecordingTerminal {
    copied: Mutex<Vec<String>>,
}

impl RecordingTerminal {
    const fn new() -> Self {
        Self {
            copied: Mutex::new(Vec::new()),
        }
    }

    fn copied(&self) -> Vec<String> {
        self.copied.lock().expect("test clipboard lock").clone()
    }
}

impl LaunchHostTerminal for RecordingTerminal {
    fn set_rich_surface_active(&self, _active: bool) {}
    fn host_screen_owned(&self) -> bool {
        false
    }
    fn is_debug_mode(&self) -> bool {
        true
    }
    fn emit_compact_line(&self, _kind: &str, _line: &str) {}
    fn set_pointer_shape(&self, _pointer: bool) {}
    fn copy_to_clipboard(&self, payload: &str) -> bool {
        self.copied
            .lock()
            .expect("test clipboard lock")
            .push(payload.to_owned());
        true
    }
}

fn hit_point_for_payload(
    area: Rect,
    state: &jackin_tui::components::ContainerInfoState,
    payload: &str,
) -> (u16, u16) {
    let rect = launch_container_info_rect(area, state);
    for row in rect.y..rect.y.saturating_add(rect.height) {
        for col in rect.x..rect.x.saturating_add(rect.width) {
            if jackin_tui::components::container_info_copy_payload_at(rect, state, col, row)
                .is_some_and(|(_, candidate)| candidate == payload)
            {
                return (col, row);
            }
        }
    }
    panic!("copy target for {payload:?} not found");
}

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
fn build_log_body_click_is_swallowed() {
    let mut view = crate::tui::update::initial_view();
    view.build_log_open = true;
    view.build_log_lines = vec!["short".to_owned()];
    let area = Rect::new(0, 0, 80, 24);
    let terminal = crate::test_support::test_host_terminal();

    handle_cockpit_mouse_down(
        &mut view,
        CockpitContext {
            area,
            run_id: "jk-run-test",
            run_log_path: "/tmp/jk-run-test.jsonl",
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
fn container_info_click_copies_real_run_id_and_log_path() {
    let mut view = crate::tui::update::initial_view();
    view.container_info_open = true;
    let area = Rect::new(0, 0, 96, 24);
    let run_id = "jk-run-test";
    let run_log_path = "/tmp/jackin/runs/jk-run-test.jsonl";
    let terminal = RecordingTerminal::new();
    let ctx = CockpitContext {
        area,
        run_id,
        run_log_path,
        terminal: &terminal,
        jackin_version: "jackin 0.0.0-test",
    };

    let state = launch_container_info_state(&view, run_id, run_log_path, true, "jackin 0.0.0-test");
    let (run_col, run_row) = hit_point_for_payload(area, &state, run_id);
    handle_cockpit_mouse_down(&mut view, ctx, run_col, run_row);

    let state = launch_container_info_state(&view, run_id, run_log_path, true, "jackin 0.0.0-test");
    let (log_col, log_row) = hit_point_for_payload(area, &state, run_log_path);
    handle_cockpit_mouse_down(&mut view, ctx, log_col, log_row);

    assert_eq!(
        terminal.copied(),
        vec![run_id.to_owned(), run_log_path.to_owned()]
    );
}

fn quit_confirm_view() -> crate::LaunchView {
    let mut view = crate::initial_view();
    view.quit_confirm = Some(jackin_tui::components::exit_confirm_state());
    view
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
        super::CockpitOutcome::HardExit
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
