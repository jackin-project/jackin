// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn modal_mouse_layer_plan_gives_quit_confirm_precedence() {
    let quit_rect = Rect::new(10, 5, 20, 8);
    let list_rect = Rect::new(30, 5, 20, 8);

    let plan = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        ConsoleModalMouseLayerFacts {
            quit_confirm_rect: Some(quit_rect),
            list_modal_rect: Some(list_rect),
            ..ConsoleModalMouseLayerFacts::default()
        },
    );

    assert_eq!(
        plan,
        ConsoleModalMouseLayerPlan {
            consumed: true,
            dismiss_quit_confirm: true,
            dismiss_list_modal: false,
        }
    );
}

#[test]
fn modal_mouse_layer_plan_dismisses_list_modal_only_when_allowed() {
    let list_rect = Rect::new(10, 5, 20, 8);

    let dismiss = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(list_rect),
            ..ConsoleModalMouseLayerFacts::default()
        },
    );
    assert_eq!(
        dismiss,
        ConsoleModalMouseLayerPlan {
            consumed: true,
            dismiss_quit_confirm: false,
            dismiss_list_modal: true,
        }
    );

    let startup_error = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(list_rect),
            startup_error_modal_active: true,
            ..ConsoleModalMouseLayerFacts::default()
        },
    );
    assert!(startup_error.consumed);
    assert!(!startup_error.dismiss_list_modal);

    let inside = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            12,
            7,
        ),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(list_rect),
            ..ConsoleModalMouseLayerFacts::default()
        },
    );
    assert!(inside.consumed);
    assert!(!inside.dismiss_list_modal);
}

#[test]
fn modal_mouse_layer_plan_allows_container_info_wheel_fallthrough() {
    let plan = modal_mouse_layer_plan(
        mouse(MouseEventKind::ScrollDown),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(Rect::new(10, 5, 20, 8)),
            list_modal_container_info: true,
            ..ConsoleModalMouseLayerFacts::default()
        },
    );

    assert_eq!(
        plan,
        ConsoleModalMouseLayerPlan {
            consumed: false,
            dismiss_quit_confirm: false,
            dismiss_list_modal: false,
        }
    );
}

#[test]
fn debug_chip_activation_requires_click_hover_and_run() {
    assert!(debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        true,
        true,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Moved),
        true,
        true,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        false,
        true,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        true,
        false,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        true,
        true,
        false,
    ));
}

#[test]
fn console_pointer_shape_uses_chrome_or_base_clickability() {
    assert_eq!(
        console_pointer_shape(false, false),
        termrock::osc::PointerShape::Default
    );
    assert_eq!(
        console_pointer_shape(true, false),
        termrock::osc::PointerShape::Pointer
    );
    assert_eq!(
        console_pointer_shape(false, true),
        termrock::osc::PointerShape::Pointer
    );
    assert_eq!(
        console_pointer_shape(true, true),
        termrock::osc::PointerShape::Pointer
    );
}

#[test]
fn startup_error_modal_blocks_outside_click_dismissal() {
    let modal_rect = Rect::new(10, 5, 30, 10);

    assert!(!should_dismiss_list_modal_for_outside_click(
        true, modal_rect, 0, 0
    ));
    assert!(!should_dismiss_list_modal_for_outside_click(
        true, modal_rect, 12, 8
    ));
    assert!(should_dismiss_list_modal_for_outside_click(
        false, modal_rect, 0, 0
    ));
    assert!(!should_dismiss_list_modal_for_outside_click(
        false, modal_rect, 12, 8
    ));
}

#[test]
fn no_modal_open_returns_false_while_list_modal_open() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::tui::console::{ConsoleStage, ConsoleState};
    use crate::tui::state::{ManagerState, update::ManagerMessage, update::update_manager};

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();

    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let clean_manager = ManagerState::from_config(&config, cwd);
    let clean_state = ConsoleState::new(ConsoleStage::Manager(clean_manager), op_cache, false);
    assert!(
        no_modal_open(&clean_state),
        "no modal by default — chip is active"
    );

    let mut manager_with_modal = ManagerState::from_config(&config, cwd);
    update_manager(
        &mut manager_with_modal,
        ManagerMessage::OpenListErrorPopup {
            title: "Error".into(),
            message: "something failed".into(),
        },
    );
    let op_cache2 = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let state_with_modal =
        ConsoleState::new(ConsoleStage::Manager(manager_with_modal), op_cache2, false);
    assert!(
        !no_modal_open(&state_with_modal),
        "list_modal open → chip and base surface must not fire"
    );
}

#[test]
fn no_modal_open_returns_false_while_quit_confirm_open() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::tui::console::{ConsoleStage, ConsoleState};
    use crate::tui::state::ManagerState;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let manager = ManagerState::from_config(&config, cwd);
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let mut state = ConsoleState::new(ConsoleStage::Manager(manager), op_cache, false);

    assert!(no_modal_open(&state), "no modal by default");
    state.open_quit_confirm();
    assert!(!no_modal_open(&state), "quit_confirm → chip must not fire");
}

#[test]
fn startup_error_exit_gate_fires_after_dialog_dismissal() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::tui::console::{ConsoleStage, ConsoleState};
    use crate::tui::state::ManagerState;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, cwd);
    manager.open_list_error_popup("Docker daemon not reachable", "docker socket missing");
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let mut state = ConsoleState::new(ConsoleStage::Manager(manager), op_cache, false);

    assert!(!startup_error_dismissed(&state, true));

    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager.list_modal = None;

    assert!(startup_error_dismissed(&state, true));
    assert!(!startup_error_dismissed(&state, false));
}

#[test]
fn keyboard_help_opens_on_question_mark_with_or_without_shift() {
    assert!(should_open_keyboard_help(key(
        KeyCode::Char('?'),
        KeyModifiers::NONE
    )));
    assert!(should_open_keyboard_help(key(
        KeyCode::Char('?'),
        KeyModifiers::SHIFT
    )));
    assert!(!should_open_keyboard_help(key(
        KeyCode::Char('?'),
        KeyModifiers::CONTROL
    )));
    assert!(!should_open_keyboard_help(key(
        KeyCode::Char('q'),
        KeyModifiers::NONE
    )));
}
