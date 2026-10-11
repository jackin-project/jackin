// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn editor_apply_tab_move_plan_resets_departed_tab_state() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Secrets;
    editor
        .unmasked_rows
        .insert((SecretsScopeTag::Workspace, "API_KEY".to_owned()));
    editor.secrets_expanded.insert("builder".to_owned());
    editor.set_tab_content_scroll_focused(true);

    editor.apply_tab_move_plan(crate::tui::screens::editor::update::editor_tab_move_plan(
        EditorTab::Secrets,
        1,
        true,
    ));

    assert_eq!(editor.active_tab, EditorTab::Auth);
    assert!(editor.tab_bar_focused());
    assert_eq!(editor.active_field, FieldFocus::Row(0));
    assert!(editor.unmasked_rows.is_empty());
    assert!(editor.secrets_expanded.is_empty());
}

#[test]
fn editor_apply_selection_and_scroll_plans_update_focus() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.apply_tab_select_plan(crate::tui::screens::editor::update::editor_tab_select_plan(
        EditorTab::Auth,
        EditorTab::Mounts,
    ));
    assert_eq!(editor.active_tab, EditorTab::Mounts);
    assert_eq!(editor.active_field, FieldFocus::Row(0));

    editor.apply_tab_bar_focus_plan(false);
    assert!(!editor.tab_bar_focused());

    editor.apply_mount_row_select_plan(
        crate::tui::screens::editor::update::editor_mount_row_select_plan(3),
    );
    assert_eq!(editor.active_field, FieldFocus::Row(3));
    assert!(editor.workspace_mounts_scroll_focused());

    editor.select_row(5);
    assert_eq!(editor.active_field, FieldFocus::Row(5));

    editor.set_hover_target(Some(EditorHoverTarget::MountRow(2)));
    assert_eq!(editor.hovered_mount_row(), Some(2));

    editor.apply_tab_horizontal_scroll_plan(
        crate::tui::screens::editor::update::editor_tab_horizontal_scroll_plan(0, 8, 20, 80),
    );
    assert_eq!(editor.tab_scroll.offset_x(), 8);
    assert!(editor.tab_content_scroll_focused());
}

#[test]
fn editor_apply_scroll_focus_plan_updates_focus_owner() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.apply_scroll_focus_plan(crate::tui::screens::editor::update::EditorScrollFocusPlan {
        workspace_mounts_scroll_focused: true,
        tab_content_scroll_focused: false,
    });
    assert!(editor.workspace_mounts_scroll_focused());

    editor.apply_scroll_focus_plan(crate::tui::screens::editor::update::EditorScrollFocusPlan {
        workspace_mounts_scroll_focused: false,
        tab_content_scroll_focused: true,
    });
    assert!(editor.tab_content_scroll_focused());
}

#[test]
fn editor_toggles_general_config_at_cursor() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.active_field = FieldFocus::Row(2);
    editor.toggle_general_selected();
    editor.active_field = FieldFocus::Row(3);
    editor.toggle_general_selected();

    assert!(editor.pending.keep_awake.enabled);
    assert!(editor.pending.git_pull_on_entry);
}

#[test]
fn editor_toggles_selected_mount_readonly() {
    let mut workspace = WorkspaceConfig::default();
    workspace.mounts.push(MountConfig {
        src: "/src".into(),
        dst: "/dst".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.active_field = FieldFocus::Row(0);
    editor.toggle_selected_mount_readonly();

    assert!(editor.pending.mounts[0].readonly);
}

#[test]
fn editor_sets_role_expansion_state() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.set_secrets_role_expanded(String::from("ops"), true);
    assert!(editor.secrets_expanded.contains("ops"));

    editor.set_secrets_role_expanded(String::from("ops"), false);
    assert!(!editor.secrets_expanded.contains("ops"));
}

#[test]
fn editor_toggles_secret_mask_state() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.toggle_secret_mask(SecretsScopeTag::Workspace, String::from("API_KEY"));
    assert!(
        editor
            .unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, String::from("API_KEY")))
    );

    editor.toggle_secret_mask(SecretsScopeTag::Workspace, String::from("API_KEY"));
    assert!(editor.unmasked_rows.is_empty());
}

#[test]
fn editor_dirty_tracks_pending_config_and_rename() {
    let workspace = WorkspaceConfig {
        workdir: "/work".into(),
        ..Default::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    assert!(!editor.is_dirty());
    editor.pending_name = Some("beta".into());
    assert!(editor.is_dirty());
}

#[test]
fn editor_workspace_name_for_panel_uses_create_fallback_or_pending_name() {
    let mut editor = TestEditor::new_create();

    assert_eq!(editor.workspace_name_for_panel(), "(new workspace)");

    editor.pending_name = Some("draft".into());
    assert_eq!(editor.workspace_name_for_panel(), "draft");
}

#[test]
fn new_create_with_workspace_sets_pending_name_and_config() {
    let workspace = WorkspaceConfig {
        workdir: "/repo".into(),
        ..Default::default()
    };

    let editor = TestEditor::new_create_with_workspace("draft".into(), workspace);

    assert!(matches!(editor.mode, EditorMode::Create));
    assert_eq!(editor.pending_name.as_deref(), Some("draft"));
    assert_eq!(editor.pending.workdir, "/repo");
}

#[test]
fn commit_workspace_name_input_updates_pending_name() {
    let mut editor = TestEditor::new_create();

    editor.commit_workspace_name_input("renamed");

    assert_eq!(editor.pending_name.as_deref(), Some("renamed"));
}

#[test]
fn dismiss_active_modal_preserves_modal_stack() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.modal = Some(TestStatusModal::Status);
    editor.modal_parents.push(TestStatusModal::Other);

    editor.dismiss_active_modal();

    assert!(editor.modal.is_none());
    assert_eq!(editor.modal_parents.len(), 1);
    assert!(matches!(editor.modal_parents[0], TestStatusModal::Other));
}

#[test]
fn has_modal_parent_tracks_modal_stack_presence() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

    assert!(!editor.has_modal_parent());

    editor.modal_parents.push(TestStatusModal::Other);

    assert!(editor.has_modal_parent());
}

#[test]
fn open_save_discard_cancel_sets_modal() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.open_save_discard_cancel(1);

    assert!(matches!(editor.modal, Some(TestStatusModal::Other)));
}

#[test]
fn open_error_popup_sets_modal() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.open_error_popup(1);

    assert!(matches!(editor.modal, Some(TestStatusModal::Other)));
}

#[test]
fn dismiss_status_popup_only_closes_status_modal() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.modal = Some(TestStatusModal::Status);

    editor.dismiss_status_popup();

    assert!(editor.modal.is_none());

    editor.modal = Some(TestStatusModal::Other);

    editor.dismiss_status_popup();

    assert!(matches!(editor.modal, Some(TestStatusModal::Other)));
}

#[test]
fn has_active_role_override_picker_checks_current_modal() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

    assert!(!editor.has_active_role_override_picker());

    editor.modal = Some(TestStatusModal::Status);
    assert!(!editor.has_active_role_override_picker());

    editor.modal = Some(TestStatusModal::Other);
    assert!(editor.has_active_role_override_picker());
}

#[test]
fn active_auth_form_focus_reads_only_auth_modal() {
    let mut editor = TestEditorWithAuthModal::new_edit("alpha".into(), WorkspaceConfig::default());

    assert_eq!(editor.active_auth_form_focus(), None);

    editor.modal = Some(TestAuthModal::Other);
    assert_eq!(editor.active_auth_form_focus(), None);

    editor.modal = Some(TestAuthModal::Auth {
        focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
    });
    assert_eq!(
        editor.active_auth_form_focus(),
        Some(crate::tui::screens::settings::model::AuthFormFocus::Save)
    );
}

#[test]
fn has_auth_form_parent_checks_top_parent_only() {
    let mut editor = TestEditorWithAuthModal::new_edit("alpha".into(), WorkspaceConfig::default());

    assert!(!editor.has_auth_form_parent());

    editor.modal_parents.push(TestAuthModal::Auth {
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
    });
    assert!(editor.has_auth_form_parent());

    editor.modal_parents.push(TestAuthModal::Other);
    assert!(!editor.has_auth_form_parent());
}

#[test]
fn commit_workdir_input_updates_pending_workdir() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.commit_workdir_input("/repo");

    assert_eq!(editor.pending.workdir, "/repo");
}

#[test]
fn commit_last_mount_dst_input_updates_last_mount() {
    let mut workspace = WorkspaceConfig::default();
    workspace.mounts.push(MountConfig {
        src: "/src".into(),
        dst: "/src".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.commit_last_mount_dst_input("/dst");

    assert_eq!(editor.pending.mounts[0].dst, "/dst");
}

#[test]
fn apply_confirmed_mounts_replaces_pending_mounts_when_present() {
    let mut workspace = WorkspaceConfig::default();
    workspace.mounts.push(MountConfig {
        src: "/old".into(),
        dst: "/old".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.apply_confirmed_mounts(Some(vec![MountConfig {
        src: "/new".into(),
        dst: "/new".into(),
        readonly: true,
        isolation: MountIsolation::Shared,
    }]));

    assert_eq!(editor.pending.mounts.len(), 1);
    assert_eq!(editor.pending.mounts[0].src, "/new");
    assert!(editor.pending.mounts[0].readonly);
}

#[test]
fn editor_save_mode_plan_classifies_edit_and_create() {
    assert_eq!(
        editor_save_mode_plan(&EditorMode::Edit {
            name: "alpha".into(),
        }),
        EditorSaveModePlan::Edit {
            original_name: "alpha".into(),
        }
    );

    assert_eq!(
        editor_save_mode_plan(&EditorMode::Create),
        EditorSaveModePlan::Create
    );
}

#[test]
fn editor_synthesizes_pending_workspace_for_auth_rows() {
    let mut editor = TestEditor::new_create();
    editor.pending_name = Some("draft".into());
    editor.pending.env.insert(
        jackin_core::ZAI_API_KEY_ENV_NAME.into(),
        jackin_config::EnvValue::Plain("zai".into()),
    );

    let synthesized = editor.synthesize_app_config_for_auth(&jackin_config::AppConfig::default());
    let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());

    assert!(synthesized.workspaces.contains_key("draft"));
    assert!(rows.iter().any(|row| matches!(
        row,
        AuthRow::WorkspaceMode {
            kind: crate::tui::auth::AuthKind::Github
        }
    )));
}

#[test]
fn editor_secrets_flat_rows_reads_pending_workspace_env() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

    assert!(editor.secrets_flat_rows().iter().any(|row| matches!(
        row,
        SecretsRow::WorkspaceKeyRow(key) if key == "TOKEN"
    )));
}
