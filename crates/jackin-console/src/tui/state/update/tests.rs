use crate::tui::auth::AuthKind;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    AuthForm, AuthFormFocus, AuthFormTarget, CreatePreludeState, DragState, EditorState, EditorTab,
    FieldFocus, ManagerStage, ManagerState, MountScrollFocus, SettingsModal, SettingsState,
    SettingsTab,
};
use jackin_tui::components::{ErrorPopupState, FocusOwner};
use ratatui::layout::Rect;

fn state_with_saved_count(count: usize) -> ManagerState<'static> {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let mut config = jackin_config::AppConfig::default();
    for idx in 0..count {
        config.workspaces.insert(
            format!("workspace-{idx}"),
            jackin_config::WorkspaceConfig {
                workdir: format!("/tmp/workspace-{idx}"),
                ..jackin_config::WorkspaceConfig::default()
            },
        );
    }
    ManagerState::from_config(&config, cwd)
}

#[test]
fn move_list_selection_clamps() {
    let mut state = state_with_saved_count(2);
    state.selected = 1;

    assert!(update_manager(&mut state, ManagerMessage::MoveListSelection(99)).is_dirty());

    assert_eq!(state.selected, state.row_count() - 1);
}

#[test]
fn select_list_row_resets_selection_local_state() {
    let mut state = state_with_saved_count(2);
    state.selected = 0;
    state.list_mounts_scroll_x = 4;

    assert!(update_manager(&mut state, ManagerMessage::SelectListRow(1)).is_dirty());

    assert_eq!(state.selected, 1);
    assert_eq!(state.list_mounts_scroll_x, 0);
}

#[test]
fn preview_focus_messages_toggle_preview_focus() {
    let mut state = state_with_saved_count(1);

    assert!(update_manager(&mut state, ManagerMessage::EnterPreview).is_dirty());
    assert!(state.preview_focused);

    assert!(update_manager(&mut state, ManagerMessage::ExitPreview).is_dirty());
    assert!(!state.preview_focused);
}

#[test]
fn tab_bar_focus_messages_update_editor_and_settings_focus() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    assert!(update_manager(&mut state, ManagerMessage::FocusEditorTabBar).is_dirty());
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor");
    };
    assert!(editor.tab_bar_focused());

    assert!(update_manager(&mut state, ManagerMessage::FocusEditorContent).is_dirty());
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor");
    };
    assert!(!editor.tab_bar_focused());

    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));
    assert!(update_manager(&mut state, ManagerMessage::FocusSettingsTabBar).is_dirty());
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings");
    };
    assert!(settings.tab_bar_focused());

    assert!(update_manager(&mut state, ManagerMessage::FocusSettingsContent).is_dirty());
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

    assert!(update_manager(&mut state, ManagerMessage::FocusEditorContent).is_dirty());

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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::SelectEditorTab(EditorTab::Mounts)
        )
        .is_dirty()
    );
    assert!(update_manager(&mut state, ManagerMessage::SelectEditorMountRow(2)).is_dirty());

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
    assert!(
        update_manager(
            &mut state,
            ManagerMessage::SelectSettingsTab(SettingsTab::Trust)
        )
        .is_dirty()
    );
    assert!(update_manager(&mut state, ManagerMessage::SelectSettingsTrustRow(0)).is_dirty());

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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ScrollFocusedListBlockVertical(3),
        )
        .is_dirty()
    );

    assert_eq!(state.list_mounts_scroll_y, 3);
}

#[test]
fn current_dir_tree_messages_respect_instance_gate() {
    let mut state = state_with_saved_count(1);

    assert!(update_manager(&mut state, ManagerMessage::ExpandSelectedTree).is_dirty());
    assert!(!state.current_dir_expanded);

    state.current_dir_expanded = true;
    assert!(update_manager(&mut state, ManagerMessage::CollapseSelectedTree).is_dirty());
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
    editor.tab_scroll_x = 4;
    editor.tab_scroll_y = 5;
    editor.secrets_expanded.insert("role".into());
    state.stage = ManagerStage::Editor(editor);

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveEditorTab {
                delta: 1,
                focus_tab_bar: true,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.active_tab, EditorTab::Auth);
    assert!(editor.tab_bar_focused());
    assert_eq!(editor.active_field, FieldFocus::Row(0));
    assert_eq!(editor.tab_scroll_x, 0);
    assert_eq!(editor.tab_scroll_y, 0);
    assert!(editor.secrets_expanded.is_empty());
}

#[test]
fn editor_auth_kind_messages_reset_local_view_state() {
    let mut state = state_with_saved_count(0);
    let mut editor = EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    );
    editor.active_field = FieldFocus::Row(5);
    editor.tab_scroll_x = 9;
    editor.tab_scroll_y = 7;
    state.stage = ManagerStage::Editor(editor);

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterEditorAuthKind {
                kind: AuthKind::Claude,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.auth_selected_kind, Some(AuthKind::Claude));
    assert_eq!(editor.active_field, FieldFocus::Row(0));
    assert_eq!(editor.tab_scroll_x, 0);
    assert_eq!(editor.tab_scroll_y, 0);

    assert!(update_manager(&mut state, ManagerMessage::ClearEditorAuthKind).is_dirty());

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.auth_selected_kind.is_none());
    assert_eq!(editor.active_field, FieldFocus::Row(0));
}

#[test]
fn editor_role_header_messages_set_expansion() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::SetEditorSecretsRoleExpanded {
                role: "smith".into(),
                expanded: true,
            },
        )
        .is_dirty()
    );
    assert!(
        update_manager(
            &mut state,
            ManagerMessage::SetEditorAuthRoleExpanded {
                role: "smith".into(),
                expanded: true,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.secrets_expanded.contains("smith"));
    assert!(editor.auth_expanded.contains("smith"));
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveEditorFieldSelection {
                delta: 1,
                max_row: 4,
                skipped_rows: vec![2],
                term: Rect::new(0, 0, 80, 24),
                footer_h: 1,
            },
        )
        .is_dirty()
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

    assert!(update_manager(&mut state, ManagerMessage::ToggleEditorGeneralSelected).is_dirty());

    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.pending.keep_awake.enabled);
    editor.active_field = FieldFocus::Row(0);

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ToggleEditorMountReadonlySelected
        )
        .is_dirty()
    );
    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ToggleEditorSecretMask {
                scope: crate::tui::state::SecretsScopeTag::Workspace,
                key: "TOKEN".into(),
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveSettingsTab {
                delta: 1,
                focus_tab_bar: true,
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveSettingsGeneralSelection { delta: 1 },
        )
        .is_dirty()
    );
    assert!(update_manager(&mut state, ManagerMessage::ToggleSettingsGeneralSelected).is_dirty());

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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveSettingsAuthSelection { delta: 99 },
        )
        .is_dirty()
    );
    assert!(update_manager(&mut state, ManagerMessage::EnterSettingsAuthKind).is_dirty());

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.auth.selected, 0);
    assert!(settings.auth.selected_kind.is_some());

    assert!(update_manager(&mut state, ManagerMessage::ClearSettingsAuthKind).is_dirty());

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.auth.selected, 0);
    assert!(settings.auth.selected_kind.is_none());
}

#[test]
fn dismiss_settings_error_popup_restores_pending_auth_form() {
    let mut state = state_with_saved_count(0);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings.error_popup = Some(ErrorPopupState::new("Token mint failed", "op item missing"));
    settings.auth.modal_parents.push(SettingsModal::AuthForm {
        target: AuthFormTarget::Workspace {
            kind: AuthKind::Claude,
        },
        state: Box::new(AuthForm::new(AuthKind::Claude)),
        focus: AuthFormFocus::Save,
        literal_buffer: "token".into(),
    });
    state.stage = ManagerStage::Settings(settings);

    assert!(update_manager(&mut state, ManagerMessage::DismissSettingsErrorPopup).is_dirty());

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert!(settings.error_popup.is_none());
    assert!(settings.auth.modal_parents.is_empty());
    let Some(SettingsModal::AuthForm {
        target,
        focus,
        literal_buffer,
        ..
    }) = settings.auth.modal
    else {
        panic!("expected auth form to be restored");
    };
    assert_eq!(
        target,
        AuthFormTarget::Workspace {
            kind: AuthKind::Claude
        }
    );
    assert_eq!(focus, AuthFormFocus::Save);
    assert_eq!(literal_buffer, "token");
}

#[test]
fn return_to_list_closes_confirm_stages() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::ConfirmDelete {
        name: "workspace".into(),
        state: jackin_tui::components::ConfirmState::new("delete?"),
    };

    assert!(update_manager(&mut state, ManagerMessage::ReturnToList).is_dirty());

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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ReloadFromConfig {
                config: Box::new(config),
                cwd: cwd.to_path_buf(),
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterSettings(SettingsState::from_config(
                &jackin_config::AppConfig::default(),
            )),
        )
        .is_dirty()
    );
    assert!(matches!(state.stage, ManagerStage::Settings(_)));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterEditor(EditorState::new_edit(
                "workspace".into(),
                jackin_config::WorkspaceConfig::default(),
            )),
        )
        .is_dirty()
    );
    assert!(matches!(state.stage, ManagerStage::Editor(_)));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterCreateEditor {
                name: "new-workspace".into(),
                workspace: jackin_config::WorkspaceConfig::default(),
            },
        )
        .is_dirty()
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected editor stage");
    };
    assert_eq!(editor.pending_name.as_deref(), Some("new-workspace"));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterCreatePrelude(CreatePreludeState::new()),
        )
        .is_dirty()
    );
    assert!(matches!(state.stage, ManagerStage::CreatePrelude(_)));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterConfirmDelete {
                name: "workspace".into(),
            },
        )
        .is_dirty()
    );
    assert!(matches!(state.stage, ManagerStage::ConfirmDelete { .. }));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::EnterConfirmInstancePurge {
                container: "jk-test".into(),
                label: "jk-test (rust)".into(),
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ScrollEditorTabHorizontal {
                delta: 8,
                term_width: 10,
                content_width: 40,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.tab_content_scroll_focused());
    assert_eq!(editor.tab_scroll_x, 8);
}

#[test]
fn scroll_editor_workspace_mounts_marks_mounts_focus_and_updates_offset() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "workspace".into(),
        jackin_config::WorkspaceConfig::default(),
    ));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                delta: 8,
                term_width: 10,
                content_width: 40,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Editor(editor) = state.stage else {
        panic!("expected editor stage");
    };
    assert!(editor.workspace_mounts_scroll_focused());
    assert_eq!(editor.workspace_mounts_scroll_x, 8);
}

#[test]
fn scroll_settings_global_mounts_updates_offset() {
    let mut state = state_with_saved_count(0);
    state.stage = ManagerStage::Settings(SettingsState::from_config(
        &jackin_config::AppConfig::default(),
    ));

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                delta: 8,
                term_width: 10,
                content_width: 40,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.mounts.scroll_x, 8);
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveSettingsGlobalMountsSelection {
                delta: 99,
                term: Rect::new(0, 0, 80, 24),
                footer_h: 1,
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveSettingsEnvSelection {
                delta: 1,
                term: Rect::new(0, 0, 80, 24),
                footer_h: 1,
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::SetSettingsEnvRoleExpanded {
                role: "smith".into(),
                expanded: true,
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ToggleSettingsGlobalMountReadonly
        )
        .is_dirty()
    );
    assert!(update_manager(&mut state, ManagerMessage::ToggleSettingsTrustSelected).is_dirty());

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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::ScrollSettingsTrustHorizontal {
                delta: 8,
                term_width: 10,
                content_width: 40,
            },
        )
        .is_dirty()
    );

    let ManagerStage::Settings(settings) = state.stage else {
        panic!("expected settings stage");
    };
    assert_eq!(settings.trust.scroll_x, 8);
}

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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::MoveSettingsTrustSelection {
                delta: 99,
                term: Rect::new(0, 0, 80, 24),
                footer_h: 1,
            },
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::SetListScrollFocus(Some(MountScrollFocus::Workspace))
        )
        .is_dirty()
    );
    assert_eq!(state.list_scroll_focus(), Some(MountScrollFocus::Workspace));
    assert_eq!(
        state.list_focus_owner,
        FocusOwner::Content(MountScrollFocus::Workspace)
    );

    assert!(update_manager(&mut state, ManagerMessage::SetListScrollFocus(None)).is_dirty());
    assert!(state.list_scroll_focus().is_none());
    assert_eq!(state.list_focus_owner, FocusOwner::TabBar);
}

#[test]
fn set_list_names_focused_stores_flag() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    assert!(update_manager(&mut state, ManagerMessage::SetListNamesFocused(true)).is_dirty());
    assert!(state.list_names_focused());
    assert_eq!(state.list_focus_owner, FocusOwner::TabBar);
    assert!(update_manager(&mut state, ManagerMessage::SetListNamesFocused(false)).is_dirty());
    assert!(!state.list_names_focused());
    assert_eq!(
        state.list_focus_owner,
        FocusOwner::Content(MountScrollFocus::Workspace)
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
    assert!(update_manager(&mut state, ManagerMessage::SetDragState(Some(drag))).is_dirty());
    assert!(state.drag_state.is_some());
    assert!(update_manager(&mut state, ManagerMessage::SetDragState(None)).is_dirty());
    assert!(state.drag_state.is_none());
}

#[test]
fn set_list_split_pct_stores_value() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let original = state.list_split_pct;

    assert!(update_manager(&mut state, ManagerMessage::SetListSplitPct(75)).is_dirty());
    assert_eq!(state.list_split_pct, 75);
    assert_ne!(state.list_split_pct, original);
}

#[test]
fn open_list_error_popup_sets_error_modal() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    assert!(state.list_modal.is_none());

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::OpenListErrorPopup {
                title: "Test error".into(),
                message: "Something went wrong.".into(),
            }
        )
        .is_dirty()
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

    assert!(
        update_manager(
            &mut state,
            ManagerMessage::OpenStatusPopup {
                title: "Stopping".into(),
                message: "Stopping capsule-a...".into(),
            }
        )
        .is_dirty()
    );
    assert!(state.status_overlay.is_some());

    assert!(update_manager(&mut state, ManagerMessage::DismissStatusPopup).is_dirty());
    assert!(state.status_overlay.is_none());
}

#[test]
fn dismiss_list_modal_clears_modal() {
    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    let _unused = update_manager(
        &mut state,
        ManagerMessage::OpenListErrorPopup {
            title: "x".into(),
            message: "y".into(),
        },
    );
    assert!(state.list_modal.is_some());

    assert!(update_manager(&mut state, ManagerMessage::DismissListModal).is_dirty());
    assert!(state.list_modal.is_none());
}
