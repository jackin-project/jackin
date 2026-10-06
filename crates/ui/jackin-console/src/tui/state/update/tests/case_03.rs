// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn move_settings_trust_selection_clamps_to_role_rows() {
    let mut state = state_with_saved_count(0);
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert(
        "chainargos/agent-a".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/agent-a".into(),
            trusted: false,
            ..jackin_config::RoleSource::default()
        },
    );
    config.roles.insert(
        "chainargos/agent-b".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/agent-b".into(),
            trusted: true,
            ..jackin_config::RoleSource::default()
        },
    );
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

    update_manager(
        &mut state,
        ManagerMessage::MoveSettingsTrustSelection {
            delta: 99,
            term: Rect::new(0, 0, 80, 24),
            footer_h: 1,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.trust.selected, settings.trust.pending.len() - 1);
}

#[test]
fn set_list_scroll_focus_stores_focus() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    assert!(state.list_scroll_focus().is_none());

    update_manager(
        &mut state,
        ManagerMessage::SetListScrollFocus(Some(MountScrollFocus::Workspace)),
    );
    assert_eq!(state.list_scroll_focus(), Some(MountScrollFocus::Workspace));
    assert_eq!(
        state.list_focus_owner.focused(),
        ConsoleFocusTarget::Content(MountScrollFocus::Workspace)
    );

    update_manager(&mut state, ManagerMessage::SetListScrollFocus(None));
    assert!(state.list_scroll_focus().is_none());
    assert_eq!(state.list_focus_owner.focused(), ConsoleFocusTarget::TabBar);
}

#[test]
fn set_list_names_focused_stores_flag() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    update_manager(&mut state, ManagerMessage::SetListNamesFocused(true));
    assert!(state.list_names_focused());
    assert_eq!(state.list_focus_owner.focused(), ConsoleFocusTarget::TabBar);
    update_manager(&mut state, ManagerMessage::SetListNamesFocused(false));
    assert!(!state.list_names_focused());
    assert_eq!(
        state.list_focus_owner.focused(),
        ConsoleFocusTarget::Content(MountScrollFocus::Workspace)
    );
}

#[test]
fn set_drag_state_stores_and_clears() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    assert!(state.drag_state.is_none());

    let drag = DragState {
        anchor_pct: 50,
        anchor_x: 40,
    };
    update_manager(&mut state, ManagerMessage::SetDragState(Some(drag)));
    assert!(state.drag_state.is_some());
    update_manager(&mut state, ManagerMessage::SetDragState(None));
    assert!(state.drag_state.is_none());
}

#[test]
fn set_list_split_pct_stores_value() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let original = state.list_split_pct;

    update_manager(&mut state, ManagerMessage::SetListSplitPct(75));
    assert_eq!(state.list_split_pct, 75);
    assert_ne!(state.list_split_pct, original);
}

#[test]
fn open_list_error_popup_sets_error_modal() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    assert!(state.list_modal.is_none());

    update_manager(
        &mut state,
        ManagerMessage::OpenListErrorPopup {
            title: "Test error".into(),
            message: "Something went wrong.".into(),
        },
    );
    assert!(matches!(
        state.list_modal,
        Some(crate::tui::state::Modal::ErrorPopup { .. })
    ));
}

#[test]
fn status_popup_messages_open_and_dismiss_overlay() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    assert!(state.status_overlay.is_none());

    update_manager(
        &mut state,
        ManagerMessage::OpenStatusPopup {
            title: "Stopping".into(),
            message: "Stopping capsule-a...".into(),
        },
    );
    assert!(state.status_overlay.is_some());

    update_manager(&mut state, ManagerMessage::DismissStatusPopup);
    assert!(state.status_overlay.is_none());
}

#[test]
fn dismiss_list_modal_clears_modal() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    update_manager(
        &mut state,
        ManagerMessage::OpenListErrorPopup {
            title: "x".into(),
            message: "y".into(),
        },
    );
    assert!(state.list_modal.is_some());

    update_manager(&mut state, ManagerMessage::DismissListModal);
    assert!(state.list_modal.is_none());
}
