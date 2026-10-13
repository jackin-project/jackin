// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn keyboard_and_mouse_tab_switches_share_one_semantic_action() {
    let keyboard = ManagerMessage::MoveEditorTab {
        delta: 1,
        focus_tab_bar: true,
    };
    let mouse = ManagerMessage::SelectEditorTab(EditorTab::Mounts);

    assert_eq!(
        action_of(&keyboard),
        Some(jackin_telemetry::schema::enums::UiActionName::TabSwitch)
    );
    assert_eq!(action_of(&mouse), action_of(&keyboard));
}

#[test]
fn move_list_selection_clamps() {
    let mut state = state_with_saved_count(2);
    state.selected = 1;

    update_manager(&mut state, ManagerMessage::MoveListSelection(99));

    assert_eq!(state.selected, state.row_count() - 1);
}

#[test]
fn select_list_row_resets_selection_local_state() {
    let mut state = state_with_saved_count(2);
    state.selected = 0;
    crate::tui::scroll_block::scroll_area_set_x(&mut state.list_mounts_scroll, 4);

    update_manager(&mut state, ManagerMessage::SelectListRow(1));

    assert_eq!(state.selected, 1);
    assert_eq!(state.list_mounts_scroll.offset_x(), 0);
}

#[test]
fn preview_focus_messages_toggle_preview_focus() {
    let mut state = state_with_saved_count(1);

    update_manager(&mut state, ManagerMessage::EnterPreview);
    assert!(state.preview_focused);

    update_manager(&mut state, ManagerMessage::ExitPreview);
    assert!(!state.preview_focused);
}

#[test]
fn tab_bar_focus_messages_update_editor_and_settings_focus() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    update_manager(&mut state, ManagerMessage::FocusEditorTabBar);
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor");
    };
    assert!(editor.tab_bar_focused());

    update_manager(&mut state, ManagerMessage::FocusEditorContent);
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor");
    };
    assert!(!editor.tab_bar_focused());

    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));
    update_manager(&mut state, ManagerMessage::FocusSettingsTabBar);
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings");
    };
    assert!(settings.tab_bar_focused());

    update_manager(&mut state, ManagerMessage::FocusSettingsContent);
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings");
    };
    assert!(!settings.tab_bar_focused());
}

#[test]
fn focus_editor_content_on_mounts_focuses_mount_rows() {
    let mut state = state_with_saved_count(0);
    let mut editor = EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    );
    editor.active_tab = EditorTab::Mounts;
    state.stage = ManagerStage::Editor(editor);

    update_manager(&mut state, ManagerMessage::FocusEditorContent);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor");
    };
    assert!(!editor.tab_bar_focused());
    assert!(editor.workspace_mounts_scroll_focused());
    assert!(!editor.tab_content_scroll_focused());
}

#[test]
fn mouse_selection_messages_update_tabs_and_rows() {
    let mut state = state_with_saved_count(0);
    let mut editor = EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    );
    editor.active_tab = EditorTab::Secrets;
    editor.secrets_expanded.insert("smith".into());
    editor.unmasked_rows.insert((
        crate::tui::state::SecretsScopeTag::Workspace,
        "TOKEN".into(),
    ));
    state.stage = ManagerStage::Editor(editor);

    update_manager(
        &mut state,
        ManagerMessage::SelectEditorTab(EditorTab::Mounts),
    );
    update_manager(&mut state, ManagerMessage::SelectEditorMountRow(2));

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_tab, EditorTab::Mounts);
    assert_eq!(editor.active_field, FieldFocus::Row(2));
    assert!(editor.workspace_mounts_scroll_focused());
    assert!(editor.secrets_expanded.is_empty());
    assert!(editor.unmasked_rows.is_empty());

    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));
    update_manager(
        &mut state,
        ManagerMessage::SelectSettingsTab(SettingsTab::Trust),
    );
    update_manager(&mut state, ManagerMessage::SelectSettingsTrustRow(0));

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.active_tab, SettingsTab::Trust);
    assert!(settings.content_focused(SettingsTab::Trust));
}

#[test]
fn scroll_focused_list_block_updates_selected_axis() {
    let mut state = state_with_saved_count(1);
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));

    update_manager(
        &mut state,
        ManagerMessage::ScrollFocusedListBlockVertical(3),
    );

    assert_eq!(state.list_mounts_scroll.offset_y(), 3);
}

#[test]
fn current_dir_tree_messages_respect_instance_gate() {
    let mut state = state_with_saved_count(1);

    update_manager(&mut state, ManagerMessage::ExpandSelectedTree);
    assert!(!state.current_dir_expanded);

    state.current_dir_expanded = true;
    update_manager(&mut state, ManagerMessage::CollapseSelectedTree);
    assert!(!state.current_dir_expanded);
}

#[test]
fn move_editor_tab_resets_tab_local_view_state() {
    let mut state = state_with_saved_count(0);
    let mut editor = EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    );
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(7);
    crate::tui::scroll_block::scroll_area_set_x(&mut editor.tab_scroll, 4);
    crate::tui::scroll_block::scroll_area_set_y(&mut editor.tab_scroll, 5);
    editor.secrets_expanded.insert("role".into());
    state.stage = ManagerStage::Editor(editor);

    update_manager(
        &mut state,
        ManagerMessage::MoveEditorTab {
            delta: 1,
            focus_tab_bar: true,
        },
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_tab, EditorTab::Auth);
    assert!(editor.tab_bar_focused());
    assert_eq!(editor.active_field, FieldFocus::Row(0));
    assert_eq!(editor.tab_scroll.offset_x(), 0);
    assert_eq!(editor.tab_scroll.offset_y(), 0);
    assert!(editor.secrets_expanded.is_empty());
}

#[test]
fn editor_role_header_messages_set_expansion() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    update_manager(
        &mut state,
        ManagerMessage::SetEditorSecretsRoleExpanded {
            role: "smith".into(),
            expanded: true,
        },
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.secrets_expanded.contains("smith"));
}

#[test]
fn move_editor_field_selection_skips_rows_and_scrolls() {
    let mut state = state_with_saved_count(0);
    let mut editor = EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    );
    editor.active_field = FieldFocus::Row(1);
    state.stage = ManagerStage::Editor(editor);

    update_manager(
        &mut state,
        ManagerMessage::MoveEditorFieldSelection {
            delta: 1,
            max_row: 4,
            skipped_rows: vec![2],
            term: Rect::new(0, 0, 80, 24),
            footer_h: 1,
        },
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_field, FieldFocus::Row(3));
}

#[test]
fn editor_toggle_messages_update_selected_content() {
    let mut state = state_with_saved_count(0);
    let mut editor = EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    );
    editor.active_field = FieldFocus::Row(2);
    editor.pending.keep_awake.enabled = false;
    editor.pending.mounts.push(jackin_config::MountConfig {
        src: "/tmp/cache".into(),
        dst: "/home/agent/.cache".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    });
    state.stage = ManagerStage::Editor(editor);

    update_manager(&mut state, ManagerMessage::ToggleEditorGeneralSelected);

    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.pending.keep_awake.enabled);
    editor.active_field = FieldFocus::Row(0);

    update_manager(
        &mut state,
        ManagerMessage::ToggleEditorMountReadonlySelected,
    );
    update_manager(
        &mut state,
        ManagerMessage::ToggleEditorSecretMask {
            scope: crate::tui::state::SecretsScopeTag::Workspace,
            key: "TOKEN".into(),
        },
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.pending.mounts[0].readonly);
    assert!(editor.unmasked_rows.contains(&(
        crate::tui::state::SecretsScopeTag::Workspace,
        "TOKEN".into()
    )));
}

#[test]
fn move_settings_tab_cycles_and_sets_focus() {
    let mut state = state_with_saved_count(0);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.active_tab = SettingsTab::Trust;
    settings.set_tab_bar_focused(false);
    state.stage = ManagerStage::Settings(settings);

    update_manager(
        &mut state,
        ManagerMessage::MoveSettingsTab {
            delta: 1,
            focus_tab_bar: true,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.active_tab, SettingsTab::General);
    assert!(settings.tab_bar_focused());
}

#[test]
fn settings_general_selection_and_toggle_update_state() {
    let mut state = state_with_saved_count(0);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.general.pending_dco = false;
    state.stage = ManagerStage::Settings(settings);

    update_manager(
        &mut state,
        ManagerMessage::MoveSettingsGeneralSelection { delta: 1 },
    );
    update_manager(&mut state, ManagerMessage::ToggleSettingsGeneralSelected);

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.general.selected, 1);
    assert!(settings.general.pending_dco);
}

#[test]
fn settings_auth_selection_and_kind_entry_update_state() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));

    // Select the GitHub row explicitly: the scan row sits last, so a
    // clamp-to-end move no longer lands on GitHub.
    let github_row = settings_github_row(&state);
    update_manager(
        &mut state,
        ManagerMessage::MoveSettingsAuthSelection {
            delta: github_row as isize,
        },
    );
    update_manager(&mut state, ManagerMessage::EnterSettingsAuthKind);

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.auth.selected, github_row);
    assert_eq!(settings.auth.selected_kind, Some(AuthKind::Github));

    update_manager(&mut state, ManagerMessage::ClearSettingsAuthKind);

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.auth.selected, 0);
    assert!(settings.auth.selected_kind.is_none());
}
