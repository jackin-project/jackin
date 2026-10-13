// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn dismiss_settings_error_popup_restores_pending_auth_form() {
    let mut state = state_with_saved_count(0);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.error_popup = Some(ErrorPopupState::new("Token mint failed", "op item missing"));
    settings
        .auth
        .modals
        .parents_mut()
        .push(SettingsModal::AuthForm {
            target: AuthFormTarget::Workspace {
                kind: AuthKind::Claude,
            },
            state: Box::new(AuthForm::new(AuthKind::Claude)),
            focus: AuthFormFocus::Save,
            literal_buffer: "token".into(),
        });
    state.stage = ManagerStage::Settings(settings);

    update_manager(&mut state, ManagerMessage::DismissSettingsErrorPopup);

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.error_popup.is_none());
    assert!(settings.auth.modals.parents().is_empty());
    let Some(SettingsModal::AuthForm {
        target,
        focus,
        literal_buffer,
        ..
    }) = settings.auth.modals.current()
    else {
        panic!("expected auth form to be restored");
    };
    assert_eq!(
        *target,
        AuthFormTarget::Workspace {
            kind: AuthKind::Claude
        }
    );
    assert_eq!(*focus, AuthFormFocus::Save);
    assert_eq!(literal_buffer, "token");
}

#[test]
fn return_to_list_closes_confirm_stages() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::ConfirmDelete {
        name: "workspace".into(),
        state: crate::tui::components::ConfirmState::new("delete?"),
    };

    update_manager(&mut state, ManagerMessage::ReturnToList);

    assert!(matches!(state.stage, ManagerStage::List));
}

#[test]
fn reload_from_config_preserves_session_cache_and_rebuilds_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let mut state = state_with_saved_count(0);
    state.op_available = true;
    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));
    let cache = std::rc::Rc::clone(&state.op_cache);
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
        "reloaded".into(),
        jackin_config::WorkspaceConfig {
            workdir: cwd.display().to_string(),
            ..jackin_config::WorkspaceConfig::default()
        },
    );

    update_manager(
        &mut state,
        ManagerMessage::ReloadFromConfig {
            config: Box::new(config),
            cwd: cwd.to_path_buf(),
        },
    );

    assert!(std::rc::Rc::ptr_eq(&state.op_cache, &cache));
    assert!(state.op_available);
    assert!(matches!(state.stage, ManagerStage::List));
    assert_eq!(state.workspaces.len(), 1);
    assert_eq!(state.workspaces[0].name, "reloaded");
}

#[test]
fn stage_entry_messages_open_requested_stage() {
    let mut state = state_with_saved_count(0);

    update_manager(
        &mut state,
        ManagerMessage::EnterSettings(SettingsState::from_config(
            &jackin_config::AppConfig::default(),
        )),
    );
    assert!(matches!(state.stage, ManagerStage::Settings(_)));

    update_manager(
        &mut state,
        ManagerMessage::EnterEditor(EditorState::new_edit(
            "workspace".into(),
            jackin_config::WorkspaceConfig::default(),
        )),
    );
    assert!(matches!(state.stage, ManagerStage::Editor(_)));

    update_manager(
        &mut state,
        ManagerMessage::EnterCreateEditor {
            name: "new-workspace".into(),
            workspace: jackin_config::WorkspaceConfig::default(),
        },
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.pending_name.as_deref(), Some("new-workspace"));

    update_manager(
        &mut state,
        ManagerMessage::EnterCreatePrelude(CreatePreludeState::new()),
    );
    assert!(matches!(state.stage, ManagerStage::CreatePrelude(_)));

    update_manager(
        &mut state,
        ManagerMessage::EnterConfirmDelete {
            name: "workspace".into(),
        },
    );
    assert!(matches!(state.stage, ManagerStage::ConfirmDelete { .. }));

    update_manager(
        &mut state,
        ManagerMessage::EnterConfirmInstancePurge {
            container: "jk-test".into(),
            label: "jk-test (rust)".into(),
        },
    );
    assert!(matches!(
        state.stage,
        ManagerStage::ConfirmInstancePurge { .. }
    ));
}

#[test]
fn scroll_editor_tab_marks_panel_focus_and_updates_offset() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    update_manager(
        &mut state,
        ManagerMessage::ScrollEditorTabHorizontal {
            delta: 8,
            term_width: 10,
            content_width: 40,
        },
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.tab_content_scroll_focused());
    assert_eq!(editor.tab_scroll.offset_x(), 8);
}

#[test]
fn scroll_editor_workspace_mounts_marks_mounts_focus_and_updates_offset() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    update_manager(
        &mut state,
        ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
            delta: 8,
            term_width: 10,
            content_width: 40,
        },
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.workspace_mounts_scroll_focused());
    assert_eq!(editor.workspace_mounts_scroll.offset_x(), 8);
}

#[test]
fn scroll_settings_global_mounts_updates_offset() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));

    update_manager(
        &mut state,
        ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
            delta: 8,
            term_width: 10,
            content_width: 40,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.mounts.scroll.offset_x(), 8);
}

#[test]
fn move_settings_global_mounts_selection_clamps_to_add_row() {
    let mut state = state_with_saved_count(0);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.mounts.pending.push(jackin_config::GlobalMountRow {
        scope: None,
        name: "cache".into(),
        mount: jackin_config::MountConfig {
            src: "/tmp/cache".into(),
            dst: "/home/agent/.cache".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
    });
    state.stage = ManagerStage::Settings(settings);

    update_manager(
        &mut state,
        ManagerMessage::MoveSettingsGlobalMountsSelection {
            delta: 99,
            term: Rect::new(0, 0, 80, 24),
            footer_h: 1,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.mounts.selected, settings.mounts.pending.len());
}

#[test]
fn move_settings_env_selection_skips_section_spacers() {
    let mut state = state_with_saved_count(0);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings
        .env
        .pending
        .env
        .insert("ALPHA".into(), jackin_core::EnvValue::Plain("one".into()));
    settings
        .env
        .pending
        .env
        .insert("BETA".into(), jackin_core::EnvValue::Plain("two".into()));
    settings.env.selected = 1;
    state.stage = ManagerStage::Settings(settings);

    update_manager(
        &mut state,
        ManagerMessage::MoveSettingsEnvSelection {
            delta: 1,
            term: Rect::new(0, 0, 80, 24),
            footer_h: 1,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.env.selected, 3);
}

#[test]
fn settings_env_role_header_message_sets_expansion() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));

    update_manager(
        &mut state,
        ManagerMessage::SetSettingsEnvRoleExpanded {
            role: "smith".into(),
            expanded: true,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.env.expanded.contains("smith"));
}

#[test]
fn settings_mount_and_trust_toggle_messages_update_selected_rows() {
    let mut state = state_with_saved_count(0);
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert(
        "chainargos/agent-smith".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/agent-smith".into(),
            trusted: false,
            ..jackin_config::RoleSource::default()
        },
    );
    let mut settings = SettingsState::from_config(&config);
    settings.mounts.pending.push(jackin_config::GlobalMountRow {
        scope: None,
        name: "cache".into(),
        mount: jackin_config::MountConfig {
            src: "/tmp/cache".into(),
            dst: "/home/agent/.cache".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
    });
    state.stage = ManagerStage::Settings(settings);

    update_manager(
        &mut state,
        ManagerMessage::ToggleSettingsGlobalMountReadonly,
    );
    update_manager(&mut state, ManagerMessage::ToggleSettingsTrustSelected);

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.mounts.pending[0].mount.readonly);
    assert!(settings.trust.pending[0].trusted);
}

#[test]
fn scroll_settings_trust_updates_offset() {
    let mut state = state_with_saved_count(0);
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert(
        "chainargos/agent-smith".into(),
        jackin_config::RoleSource {
            git: "https://github.com/chainargos/agent-smith".into(),
            trusted: true,
            ..jackin_config::RoleSource::default()
        },
    );
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

    update_manager(
        &mut state,
        ManagerMessage::ScrollSettingsTrustHorizontal {
            delta: 8,
            term_width: 10,
            content_width: 40,
        },
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.trust.scroll.offset_x(), 8);
}
