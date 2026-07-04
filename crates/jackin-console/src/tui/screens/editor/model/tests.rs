#[cfg(test)]
use jackin_config::{
    MountConfig, MountIsolation, RoleSource, WorkspaceConfig, WorkspaceRoleOverride,
};

use super::{
    AuthEnterPlan, AuthRow, EditorAuthActionKeyPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan,
    EditorFieldSelectionKeyPlan, EditorHorizontalScrollKeyPlan, EditorImmediateActionKeyPlan,
    EditorMode, EditorMountActionKeyPlan, EditorMountGithubOpenPlan, EditorNavigationKeyPlan,
    EditorRoleActionKeyPlan, EditorRoleHeaderExpansionKeyPlan, EditorSaveKeyPlan,
    EditorSaveModePlan, EditorSecretsActionKeyPlan, EditorState, EditorStatusPopupModal, EditorTab,
    EditorTabActionKeyPlan, FieldFocus, RoleHeaderExpansionPlan, SecretsRow, editor_save_mode_plan,
};

type TestEditor = EditorState<(), (), (), jackin_config::EnvValue, (), (), (), (), (), ()>;
#[derive(Debug)]
enum TestStatusModal {
    Status,
    Other,
}

impl EditorStatusPopupModal for TestStatusModal {
    fn is_status_popup(&self) -> bool {
        matches!(self, Self::Status)
    }
}

impl super::EditorRoleOverridePickerModal for TestStatusModal {
    fn is_role_override_picker(&self) -> bool {
        matches!(self, Self::Other)
    }
}

impl super::EditorSaveDiscardModal<u8> for TestStatusModal {
    fn save_discard_cancel_modal(state: u8) -> Self {
        if state == 0 {
            Self::Status
        } else {
            Self::Other
        }
    }
}

impl super::EditorErrorPopupModal<u8> for TestStatusModal {
    fn error_popup_modal(state: u8) -> Self {
        if state == 0 {
            Self::Status
        } else {
            Self::Other
        }
    }
}

type TestEditorWithStatusModal =
    EditorState<(), TestStatusModal, (), jackin_config::EnvValue, (), (), (), (), (), ()>;
#[derive(Debug)]
enum TestAuthModal {
    Auth {
        focus: crate::tui::screens::settings::model::AuthFormFocus,
    },
    Other,
}

impl
    crate::tui::auth_config::ModalAuthFormFocusInspect<
        crate::tui::screens::settings::model::AuthFormFocus,
    > for TestAuthModal
{
    fn active_auth_form_focus(
        &self,
    ) -> Option<crate::tui::screens::settings::model::AuthFormFocus> {
        match self {
            Self::Auth { focus } => Some(*focus),
            Self::Other => None,
        }
    }
}

impl crate::tui::auth_config::ModalAuthFormParentInspect for TestAuthModal {
    fn is_auth_form_parent(&self) -> bool {
        matches!(self, Self::Auth { .. })
    }
}

type TestEditorWithAuthModal =
    EditorState<(), TestAuthModal, (), jackin_config::EnvValue, u8, (), (), (), (), ()>;
type TestEditorWithMountCache = EditorState<
    crate::mount_info_cache::MountInfoCache,
    (),
    (),
    jackin_config::EnvValue,
    (),
    (),
    (),
    (),
    (),
    (),
>;

#[test]
fn editor_apply_auth_kind_plan_updates_selection_and_scroll() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_field = FieldFocus::Row(9);
    editor.tab_scroll_x = 12;
    editor.tab_scroll_y = 4;

    editor.apply_auth_kind_plan(
        crate::tui::screens::editor::update::enter_editor_auth_kind_plan(
            crate::tui::auth::AuthKind::Claude,
        ),
    );

    assert_eq!(
        editor.auth_selected_kind,
        Some(crate::tui::auth::AuthKind::Claude)
    );
    assert_eq!(editor.active_field, FieldFocus::Row(0));
    assert_eq!(editor.tab_scroll_x, 0);
    assert_eq!(editor.tab_scroll_y, 0);

    editor.apply_auth_kind_plan(crate::tui::screens::editor::update::clear_editor_auth_kind_plan());
    assert_eq!(editor.auth_selected_kind, None);
}

#[test]
fn editor_apply_tab_move_plan_resets_departed_tab_state() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Secrets;
    editor
        .unmasked_rows
        .insert((super::SecretsScopeTag::Workspace, "API_KEY".to_owned()));
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
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);

    editor.apply_tab_select_plan(crate::tui::screens::editor::update::editor_tab_select_plan(
        EditorTab::Auth,
        EditorTab::Mounts,
    ));
    assert_eq!(editor.active_tab, EditorTab::Mounts);
    assert_eq!(editor.auth_selected_kind, None);
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

    editor.set_hover_target(Some(super::EditorHoverTarget::MountRow(2)));
    assert_eq!(editor.hovered_mount_row(), Some(2));

    editor.apply_tab_horizontal_scroll_plan(
        crate::tui::screens::editor::update::editor_tab_horizontal_scroll_plan(0, 8, 20, 80),
    );
    assert_eq!(editor.tab_scroll_x, 8);
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

    editor.set_auth_role_expanded(String::from("dev"), true);
    editor.set_secrets_role_expanded(String::from("ops"), true);
    assert!(editor.auth_expanded.contains("dev"));
    assert!(editor.secrets_expanded.contains("ops"));

    editor.set_auth_role_expanded(String::from("dev"), false);
    editor.set_secrets_role_expanded(String::from("ops"), false);
    assert!(!editor.auth_expanded.contains("dev"));
    assert!(!editor.secrets_expanded.contains("ops"));
}

#[test]
fn editor_toggles_secret_mask_state() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.toggle_secret_mask(super::SecretsScopeTag::Workspace, String::from("API_KEY"));
    assert!(
        editor
            .unmasked_rows
            .contains(&(super::SecretsScopeTag::Workspace, String::from("API_KEY")))
    );

    editor.toggle_secret_mask(super::SecretsScopeTag::Workspace, String::from("API_KEY"));
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
fn dismiss_active_modal_preserves_modal_stack_and_scratch() {
    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.modal = Some(TestStatusModal::Status);
    editor.modal_parents.push(TestStatusModal::Other);
    editor.pending_picker_value = Some(jackin_config::EnvValue::Plain("secret".into()));

    editor.dismiss_active_modal();

    assert!(editor.modal.is_none());
    assert_eq!(editor.modal_parents.len(), 1);
    assert!(matches!(editor.modal_parents[0], TestStatusModal::Other));
    assert!(matches!(
        editor.pending_picker_value,
        Some(jackin_config::EnvValue::Plain(_))
    ));
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
        jackin_core::env_model::ZAI_API_KEY_ENV_NAME.into(),
        jackin_config::EnvValue::Plain("zai".into()),
    );
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

    let synthesized = editor.synthesize_app_config_for_auth(&jackin_config::AppConfig::default());
    let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());

    assert!(synthesized.workspaces.contains_key("draft"));
    assert!(rows.iter().any(|row| matches!(
        row,
        AuthRow::WorkspaceMode {
            kind: crate::tui::auth::AuthKind::Zai
        }
    )));
}

#[test]
fn editor_focused_auth_form_prefills_workspace_layer() {
    let workspace = WorkspaceConfig {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Sync,
            sync_source_dir: Some(std::path::PathBuf::from("/host/claude")),
        }),
        ..WorkspaceConfig::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);

    let (target, form) = editor
        .focused_auth_form(&jackin_config::AppConfig::default())
        .expect("workspace mode row should open auth form");

    assert!(matches!(
        target,
        crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude
        }
    ));
    assert_eq!(form.mode, Some(crate::tui::auth::AuthMode::Sync));
    assert_eq!(
        form.source_folder,
        Some(std::path::PathBuf::from("/host/claude"))
    );
    assert!(form.shows_source_folder());
}

#[test]
fn editor_focused_auth_form_returns_none_for_non_form_rows() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
    editor.active_field = FieldFocus::Row(usize::MAX);

    assert!(
        editor
            .focused_auth_form(&jackin_config::AppConfig::default())
            .is_none()
    );
}

#[test]
fn editor_persist_auth_form_writes_workspace_layer() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Zai);
    form.set_mode(crate::tui::auth::AuthMode::ApiKey);
    form.set_literal("zai-key".into());

    editor.persist_auth_form(
        &crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Zai,
        },
        &form,
    );

    assert_eq!(
        editor
            .pending
            .env
            .get(jackin_core::env_model::ZAI_API_KEY_ENV_NAME),
        Some(&jackin_config::EnvValue::Plain("zai-key".into()))
    );
}

#[test]
fn editor_clear_auth_form_layer_clears_role_source_folder() {
    let mut workspace = WorkspaceConfig::default();
    workspace.roles.entry("dev".into()).or_default().claude =
        Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Sync,
            sync_source_dir: Some(std::path::PathBuf::from("/role/claude")),
        });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.clear_auth_form_layer(
        &crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
            role: "dev".into(),
            kind: crate::tui::auth::AuthKind::Claude,
        },
    );

    assert_eq!(editor.pending.roles["dev"].claude, None);
}

#[test]
fn editor_toggle_auth_role_expanded_flips_role_section() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.toggle_auth_role_expanded("dev".into());
    assert!(editor.auth_expanded.contains("dev"));

    editor.toggle_auth_role_expanded("dev".into());
    assert!(!editor.auth_expanded.contains("dev"));
}

#[test]
fn editor_focused_auth_role_expansion_plan_reads_current_row() {
    let workspace = WorkspaceConfig {
        roles: std::collections::BTreeMap::from([(
            "dev".into(),
            WorkspaceRoleOverride {
                github: Some(jackin_config::GithubAuthConfig {
                    auth_forward: jackin_config::GithubAuthMode::Token,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
    let config = jackin_config::AppConfig::default();
    editor.active_field = FieldFocus::Row(
        editor
            .auth_flat_rows(&config)
            .iter()
            .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
            .expect("role header row"),
    );

    assert_eq!(
        editor.focused_auth_role_expansion_plan(&config, true),
        RoleHeaderExpansionPlan::Set {
            role: "dev".into(),
            expanded: true
        }
    );

    editor.auth_expanded.insert("dev".into());
    assert_eq!(
        editor.focused_auth_role_expansion_plan(&config, true),
        RoleHeaderExpansionPlan::HeaderNoop
    );
    assert_eq!(
        editor.focused_auth_role_expansion_plan(&config, false),
        RoleHeaderExpansionPlan::Set {
            role: "dev".into(),
            expanded: false
        }
    );
}

#[test]
fn editor_focused_role_header_expansion_key_plan_routes_by_tab() {
    let mut workspace = WorkspaceConfig::default();
    workspace
        .roles
        .entry("dev".into())
        .or_default()
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    workspace.roles.entry("dev".into()).or_default().github =
        Some(jackin_config::GithubAuthConfig {
            auth_forward: jackin_config::GithubAuthMode::Token,
            ..Default::default()
        });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    let config = jackin_config::AppConfig::default();

    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(
        editor
            .secrets_flat_rows()
            .iter()
            .position(|row| matches!(row, SecretsRow::RoleHeader { role, .. } if role == "dev"))
            .expect("secrets role header row"),
    );
    assert_eq!(
        editor.focused_role_header_expansion_key_plan(&config, true),
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::Set {
            role: "dev".into(),
            expanded: true
        })
    );

    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
    editor.active_field = FieldFocus::Row(
        editor
            .auth_flat_rows(&config)
            .iter()
            .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
            .expect("auth role header row"),
    );
    assert_eq!(
        editor.focused_role_header_expansion_key_plan(&config, true),
        EditorRoleHeaderExpansionKeyPlan::Auth(RoleHeaderExpansionPlan::Set {
            role: "dev".into(),
            expanded: true
        })
    );

    editor.active_tab = EditorTab::Roles;
    assert_eq!(
        editor.focused_role_header_expansion_key_plan(&config, true),
        EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab
    );
}

#[test]
fn editor_focused_auth_kind_reads_current_row() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let config = jackin_config::AppConfig::default();

    assert_eq!(
        editor.focused_auth_kind(&config),
        Some(crate::tui::auth::AuthKind::Claude)
    );

    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
    assert_eq!(editor.focused_auth_kind(&config), None);
}

#[test]
fn editor_focused_auth_enter_plan_reads_current_row() {
    let mut workspace = WorkspaceConfig::default();
    workspace.roles.entry("dev".into()).or_default().github =
        Some(jackin_config::GithubAuthConfig {
            auth_forward: jackin_config::GithubAuthMode::Token,
            ..Default::default()
        });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    let config = jackin_config::AppConfig::default();

    assert_eq!(editor.focused_auth_enter_plan(&config), AuthEnterPlan::Noop);

    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
    assert_eq!(
        editor.focused_auth_enter_plan(&config),
        AuthEnterPlan::OpenForm
    );

    editor.active_field = FieldFocus::Row(
        editor
            .auth_flat_rows(&config)
            .iter()
            .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
            .expect("role header row"),
    );
    assert_eq!(
        editor.focused_auth_enter_plan(&config),
        AuthEnterPlan::ToggleRole("dev".into())
    );

    editor.active_field = FieldFocus::Row(editor.auth_flat_rows(&config).len() - 1);
    assert_eq!(
        editor.focused_auth_enter_plan(&config),
        AuthEnterPlan::AddRoleOverride
    );
}

#[test]
fn editor_clear_auth_row_at_cursor_clears_workspace_auth_layer() {
    let workspace = WorkspaceConfig {
        env: std::collections::BTreeMap::from([(
            jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
            jackin_config::EnvValue::Plain("zai".into()),
        )]),
        ..WorkspaceConfig::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

    editor.clear_auth_row_at_cursor(&jackin_config::AppConfig::default());

    assert!(
        !editor
            .pending
            .env
            .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
    );
}

#[test]
fn editor_clear_auth_row_at_cursor_clears_role_auth_layer() {
    let mut workspace = WorkspaceConfig::default();
    workspace.roles.entry("dev".into()).or_default().env.insert(
        jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
        jackin_config::EnvValue::Plain("zai".into()),
    );
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

    let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());
    editor.active_field = FieldFocus::Row(
        rows.iter()
            .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
            .expect("role header should be present"),
    );
    editor.clear_auth_row_at_cursor(&jackin_config::AppConfig::default());

    assert!(
        !editor.pending.roles["dev"]
            .env
            .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
    );
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

#[test]
fn editor_selection_bounds_reads_state_and_config_counts() {
    let workspace = WorkspaceConfig {
        mounts: vec![
            MountConfig {
                src: "/src-a".into(),
                dst: "/dst-a".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            MountConfig {
                src: "/src-b".into(),
                dst: "/dst-b".into(),
                readonly: true,
                isolation: MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert("alpha".into(), RoleSource::default());
    config.roles.insert("beta".into(), RoleSource::default());
    config.roles.insert("gamma".into(), RoleSource::default());
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.active_tab = EditorTab::Mounts;
    assert_eq!(editor.selection_bounds(&config), (2, Vec::new()));

    editor.active_tab = EditorTab::Roles;
    assert_eq!(editor.selection_bounds(&config), (3, Vec::new()));
}

#[test]
fn editor_field_selection_key_plan_includes_bounds_and_footer() {
    let workspace = WorkspaceConfig {
        mounts: vec![
            MountConfig {
                src: "/src-a".into(),
                dst: "/dst-a".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            MountConfig {
                src: "/src-b".into(),
                dst: "/dst-b".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.active_tab = EditorTab::Mounts;
    editor.cached_footer_h = 3;
    let term = ratatui::layout::Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };

    assert_eq!(
        editor.field_selection_key_plan(&jackin_config::AppConfig::default(), 1, term),
        EditorFieldSelectionKeyPlan {
            delta: 1,
            max_row: 2,
            skipped_rows: Vec::new(),
            term,
            footer_h: 3,
        }
    );
}

#[test]
fn editor_navigation_key_plan_follows_tab_focus() {
    use crossterm::event::KeyCode;

    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    assert_eq!(
        editor.navigation_key_plan(KeyCode::Left),
        EditorNavigationKeyPlan::MoveTab {
            delta: -1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        editor.navigation_key_plan(KeyCode::Right),
        EditorNavigationKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        editor.navigation_key_plan(KeyCode::Down),
        EditorNavigationKeyPlan::FocusContent
    );

    editor.set_tab_bar_focused(false);
    assert_eq!(
        editor.navigation_key_plan(KeyCode::Tab),
        EditorNavigationKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        editor.navigation_key_plan(KeyCode::BackTab),
        EditorNavigationKeyPlan::FocusTabBar
    );
    assert_eq!(
        editor.navigation_key_plan(KeyCode::Down),
        EditorNavigationKeyPlan::NotNavigation
    );
}

// Editor top-level dispatch precedence is covered against the real
// keymap-based resolver in
// `input::editor::tests::dispatch_editor_top_level_preserves_precedence`.

#[test]
fn editor_immediate_action_key_plan_routes_tab_actions() {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut workspace = WorkspaceConfig::default();
    workspace.env.insert(
        "TOKEN".into(),
        jackin_config::EnvValue::Plain("secret".into()),
    );
    workspace.mounts.push(MountConfig {
        src: "/src".into(),
        dst: "/dst".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    let config = jackin_config::AppConfig::default();

    editor.active_tab = EditorTab::Auth;
    assert_eq!(
        editor.immediate_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty()),
        EditorImmediateActionKeyPlan::EnterAuthKind(crate::tui::auth::AuthKind::Claude)
    );

    editor.active_tab = EditorTab::General;
    assert_eq!(
        editor.immediate_action_key_plan(&config, KeyCode::Char(' '), KeyModifiers::empty()),
        EditorImmediateActionKeyPlan::ToggleGeneralSelected
    );

    editor.active_tab = EditorTab::Mounts;
    assert_eq!(
        editor.immediate_action_key_plan(&config, KeyCode::Char('r'), KeyModifiers::empty()),
        EditorImmediateActionKeyPlan::ToggleMountReadonlySelected
    );

    editor.active_tab = EditorTab::Secrets;
    assert_eq!(
        editor.immediate_action_key_plan(&config, KeyCode::Char('m'), KeyModifiers::empty()),
        EditorImmediateActionKeyPlan::ToggleSecretMask {
            scope: super::SecretsScopeTag::Workspace,
            key: "TOKEN".into(),
        }
    );
    assert_eq!(
        editor.immediate_action_key_plan(&config, KeyCode::Char('m'), KeyModifiers::CONTROL),
        EditorImmediateActionKeyPlan::NotImmediateAction
    );
}

#[test]
fn editor_role_action_key_plan_routes_role_tab_actions() {
    use crossterm::event::KeyCode;

    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;

    assert_eq!(
        editor.role_action_key_plan(KeyCode::Char('a')),
        EditorRoleActionKeyPlan::OpenRoleInput
    );
    assert_eq!(
        editor.role_action_key_plan(KeyCode::Char('A')),
        EditorRoleActionKeyPlan::OpenRoleInput
    );
    assert_eq!(
        editor.role_action_key_plan(KeyCode::Char(' ')),
        EditorRoleActionKeyPlan::ToggleAllowed
    );
    assert_eq!(
        editor.role_action_key_plan(KeyCode::Char('*')),
        EditorRoleActionKeyPlan::ToggleDefault
    );
    assert_eq!(
        editor.role_action_key_plan(KeyCode::Char('x')),
        EditorRoleActionKeyPlan::NotRoleAction
    );

    editor.active_tab = EditorTab::Mounts;
    assert_eq!(
        editor.role_action_key_plan(KeyCode::Char('a')),
        EditorRoleActionKeyPlan::NotRoleAction
    );
}

#[test]
fn editor_mount_action_key_plan_routes_mount_tab_actions() {
    use crossterm::event::KeyCode;

    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;

    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('a')),
        EditorMountActionKeyPlan::AddMount
    );
    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('A')),
        EditorMountActionKeyPlan::AddMount
    );
    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('d')),
        EditorMountActionKeyPlan::RemoveSelectedMount
    );
    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('i')),
        EditorMountActionKeyPlan::CycleIsolation
    );
    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('o')),
        EditorMountActionKeyPlan::OpenGithub
    );
    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('x')),
        EditorMountActionKeyPlan::NotMountAction
    );

    editor.active_tab = EditorTab::Roles;
    assert_eq!(
        editor.mount_action_key_plan(KeyCode::Char('a')),
        EditorMountActionKeyPlan::NotMountAction
    );
}

#[test]
fn editor_secrets_action_key_plan_routes_secrets_tab_actions() {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Secrets;

    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('p'), KeyModifiers::empty(), true),
        EditorSecretsActionKeyPlan::OpenPicker
    );
    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('P'), KeyModifiers::SHIFT, true),
        EditorSecretsActionKeyPlan::OpenPicker
    );
    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('p'), KeyModifiers::empty(), false),
        EditorSecretsActionKeyPlan::NotSecretsAction
    );
    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('d'), KeyModifiers::empty(), true),
        EditorSecretsActionKeyPlan::OpenDeleteConfirm
    );
    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('a'), KeyModifiers::empty(), true),
        EditorSecretsActionKeyPlan::OpenAddModal
    );
    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('a'), KeyModifiers::CONTROL, true),
        EditorSecretsActionKeyPlan::NotSecretsAction
    );

    editor.active_tab = EditorTab::Roles;
    assert_eq!(
        editor.secrets_action_key_plan(KeyCode::Char('a'), KeyModifiers::empty(), true),
        EditorSecretsActionKeyPlan::NotSecretsAction
    );
}

#[test]
fn editor_auth_action_key_plan_routes_auth_tab_actions() {
    use crossterm::event::KeyCode;

    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Auth;

    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('a')),
        EditorAuthActionKeyPlan::NotAuthAction
    );

    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('a')),
        EditorAuthActionKeyPlan::OpenRolePicker
    );
    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('A')),
        EditorAuthActionKeyPlan::OpenRolePicker
    );
    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('d')),
        EditorAuthActionKeyPlan::ClearFocusedRow
    );
    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('x')),
        EditorAuthActionKeyPlan::NotAuthAction
    );

    editor.active_tab = EditorTab::Roles;
    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('d')),
        EditorAuthActionKeyPlan::NotAuthAction
    );
}

#[test]
fn editor_tab_action_key_plan_routes_active_tab_precedence() {
    use crossterm::event::{KeyCode, KeyModifiers};

    let config = jackin_config::AppConfig::default();
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.active_tab = EditorTab::Mounts;
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Char('a'), KeyModifiers::empty(), true,),
        EditorTabActionKeyPlan::Mount(EditorMountActionKeyPlan::AddMount)
    );

    editor.active_tab = EditorTab::Secrets;
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Char('p'), KeyModifiers::empty(), true,),
        EditorTabActionKeyPlan::Secrets(EditorSecretsActionKeyPlan::OpenPicker)
    );
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Char('p'), KeyModifiers::empty(), false,),
        EditorTabActionKeyPlan::Noop
    );

    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Char('a'), KeyModifiers::empty(), true,),
        EditorTabActionKeyPlan::Auth(EditorAuthActionKeyPlan::OpenRolePicker)
    );
}

#[test]
fn editor_tab_action_key_plan_delegates_enter_after_actions() {
    use crossterm::event::{KeyCode, KeyModifiers};

    let config = jackin_config::AppConfig::default();
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.active_tab = EditorTab::General;
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty(), true),
        EditorTabActionKeyPlan::Enter(EditorEnterKeyPlan::OpenGeneralField)
    );

    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = None;
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty(), true),
        EditorTabActionKeyPlan::Enter(EditorEnterKeyPlan::Auth(AuthEnterPlan::Noop))
    );
}

#[test]
fn editor_enter_key_plan_routes_tab_actions() {
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert("dev".into(), RoleSource::default());

    let mut workspace = WorkspaceConfig::default();
    workspace.env.insert(
        "A_PLAIN".into(),
        jackin_config::EnvValue::Plain("secret".into()),
    );
    workspace.env.insert(
        "Z_OP".into(),
        jackin_config::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".into(),
            path: "Vault/Item/Field".into(),
            account: None,
            on_demand: false,
        }),
    );
    workspace.mounts.push(MountConfig {
        src: "/src".into(),
        dst: "/dst".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });

    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.active_tab = EditorTab::General;
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::OpenGeneralField
    );

    editor.active_tab = EditorTab::Mounts;
    editor.active_field = FieldFocus::Row(0);
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::Noop
    );
    editor.active_field = FieldFocus::Row(1);
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::OpenMountFileBrowser
    );

    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::OpenSecretsEnterModal
    );
    editor.active_field = FieldFocus::Row(1);
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::OpenSecretsPicker
    );
    assert_eq!(
        editor.enter_key_plan(&config, false),
        EditorEnterKeyPlan::OpenSecretsEnterModal
    );

    editor.active_tab = EditorTab::Roles;
    editor.active_field = FieldFocus::Row(1);
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::OpenRoleInput
    );

    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
    editor.active_field = FieldFocus::Row(0);
    assert_eq!(
        editor.enter_key_plan(&config, true),
        EditorEnterKeyPlan::Auth(AuthEnterPlan::OpenForm)
    );
}

#[test]
fn editor_escape_key_plan_routes_focus_auth_and_dirty_state() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.set_tab_bar_focused(false);
    editor.active_tab = EditorTab::General;
    assert_eq!(editor.escape_key_plan(), EditorEscapeKeyPlan::FocusTabBar);

    editor.active_tab = EditorTab::Auth;
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
    assert_eq!(
        editor.escape_key_plan(),
        EditorEscapeKeyPlan::FocusTabBarAndClearAuthKind
    );

    editor.set_tab_bar_focused(true);
    assert_eq!(editor.escape_key_plan(), EditorEscapeKeyPlan::ClearAuthKind);

    editor.auth_selected_kind = None;
    assert_eq!(
        editor.escape_key_plan(),
        EditorEscapeKeyPlan::ReloadFromConfig
    );

    editor.pending_name = Some("beta".into());
    assert_eq!(
        editor.escape_key_plan(),
        EditorEscapeKeyPlan::OpenSaveDiscard
    );
}

#[test]
fn editor_save_key_plan_only_saves_dirty_editor() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    assert_eq!(editor.save_key_plan(), EditorSaveKeyPlan::Noop);

    editor.pending_name = Some("beta".into());
    assert_eq!(editor.save_key_plan(), EditorSaveKeyPlan::BeginSave);
}

#[test]
fn editor_focused_add_row_selection_reads_counts() {
    let workspace = WorkspaceConfig {
        mounts: vec![
            MountConfig {
                src: "/src-a".into(),
                dst: "/dst-a".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            MountConfig {
                src: "/src-b".into(),
                dst: "/dst-b".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    let mut config = jackin_config::AppConfig::default();
    config.roles.insert("alpha".into(), RoleSource::default());
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.active_field = FieldFocus::Row(1);
    assert!(!editor.focused_mount_add_row_selected());
    assert!(editor.focused_role_add_row_selected(&config));

    editor.active_field = FieldFocus::Row(2);
    assert!(editor.focused_mount_add_row_selected());
    assert!(!editor.focused_role_add_row_selected(&config));
}

#[test]
fn editor_focused_mount_github_open_plan_reads_cache() {
    let workspace = WorkspaceConfig {
        mounts: vec![
            MountConfig {
                src: "/repo".into(),
                dst: "/repo".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            MountConfig {
                src: "/folder".into(),
                dst: "/folder".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    let mut editor = TestEditorWithMountCache::new_edit("alpha".into(), workspace);
    editor.mount_info_cache.store_entries([
        (
            "/repo".into(),
            crate::mount_info::MountKind::Git {
                branch: crate::mount_info::GitBranch::Named("main".into()),
                origin: Some(crate::mount_info::GitOrigin::Github {
                    remote_url: "git@github.com:jackin-project/jackin.git".into(),
                    web_url: "https://github.com/jackin-project/jackin/tree/main".into(),
                }),
            },
        ),
        ("/folder".into(), crate::mount_info::MountKind::Folder),
    ]);

    assert_eq!(
        editor.focused_mount_github_open_plan(),
        EditorMountGithubOpenPlan::Open(
            "https://github.com/jackin-project/jackin/tree/main".into()
        )
    );

    editor.active_field = FieldFocus::Row(1);
    assert_eq!(
        editor.focused_mount_github_open_plan(),
        EditorMountGithubOpenPlan::NoGithubUrl
    );

    editor.active_field = FieldFocus::Row(2);
    assert_eq!(
        editor.focused_mount_github_open_plan(),
        EditorMountGithubOpenPlan::NoSelection
    );
}

#[test]
fn editor_horizontal_scroll_key_plan_targets_active_area() {
    let workspace = WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/repo".into(),
            dst: "/repo".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    };
    let mut editor = TestEditorWithMountCache::new_edit("alpha".into(), workspace);
    editor.tab_content_width = 123;

    assert_eq!(
        editor.horizontal_scroll_key_plan(-8),
        EditorHorizontalScrollKeyPlan::TabContent {
            delta: -8,
            content_width: 123,
        }
    );

    editor.active_tab = EditorTab::Mounts;
    let expected_content_width = editor.workspace_mounts_content_width();
    assert_eq!(
        editor.horizontal_scroll_key_plan(8),
        EditorHorizontalScrollKeyPlan::WorkspaceMounts {
            delta: 8,
            content_width: expected_content_width,
        }
    );
}

#[test]
fn editor_secret_value_reads_workspace_and_role_env() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    editor
        .pending
        .roles
        .entry("dev".into())
        .or_default()
        .env
        .insert(
            "ROLE_TOKEN".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
        );

    assert_eq!(
        editor.secret_value(&super::SecretsScopeTag::Workspace, "TOKEN"),
        Some(&jackin_config::EnvValue::Plain("one".into()))
    );
    assert!(
        editor
            .secret_value(&super::SecretsScopeTag::Role("dev".into()), "ROLE_TOKEN")
            .is_some_and(|value| matches!(value, jackin_config::EnvValue::OpRef(_)))
    );
    assert!(
        editor
            .secret_value(
                &super::SecretsScopeTag::Role("missing".into()),
                "ROLE_TOKEN"
            )
            .is_none()
    );
}

#[test]
fn editor_delete_env_var_removes_workspace_key() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

    editor
        .delete_env_var(&super::SecretsScopeTag::Workspace, "TOKEN")
        .unwrap();

    assert!(!editor.pending.env.contains_key("TOKEN"));
}

#[test]
fn editor_delete_env_var_removes_empty_role_override() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .roles
        .entry("dev".into())
        .or_default()
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

    editor
        .delete_env_var(&super::SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert!(!editor.pending.roles.contains_key("dev"));
}

#[test]
fn editor_delete_env_var_blocks_managed_claude_oauth_token() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.pending.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: jackin_config::AuthForwardMode::OAuthToken,
        sync_source_dir: None,
    });
    editor.pending.env.insert(
        jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME.into(),
        jackin_config::EnvValue::Plain("token".into()),
    );

    let err = editor
        .delete_env_var(
            &super::SecretsScopeTag::Workspace,
            jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
        )
        .unwrap_err();

    assert!(err.to_string().contains("claude-token revoke"));
    assert!(
        editor
            .pending
            .env
            .contains_key(jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
    );
}

#[test]
fn editor_secret_text_editability_rejects_op_refs() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("PLAIN".into(), jackin_config::EnvValue::Plain("one".into()));
    editor.pending.env.insert(
        "OP_REF".into(),
        jackin_config::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".into(),
            path: "Vault/Item/Field".into(),
            account: None,
            on_demand: false,
        }),
    );

    assert!(editor.secret_is_text_editable(&super::SecretsScopeTag::Workspace, "PLAIN"));
    assert!(!editor.secret_is_text_editable(&super::SecretsScopeTag::Workspace, "OP_REF"));
}

#[test]
fn editor_focused_secret_is_op_ref_reads_current_row() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.pending.env.insert(
        "A_OP_REF".into(),
        jackin_config::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".into(),
            path: "Vault/Item/Field".into(),
            account: None,
            on_demand: false,
        }),
    );
    editor.pending.env.insert(
        "Z_PLAIN".into(),
        jackin_config::EnvValue::Plain("one".into()),
    );

    editor.active_field = FieldFocus::Row(0);
    assert!(editor.focused_secret_is_op_ref());

    editor.active_field = FieldFocus::Row(1);
    assert!(!editor.focused_secret_is_op_ref());
}

#[test]
fn editor_focused_unmask_key_skips_op_refs() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.pending.env.insert(
        "A_TOKEN".into(),
        jackin_config::EnvValue::Plain("one".into()),
    );
    editor.pending.env.insert(
        "Z_OP_REF".into(),
        jackin_config::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".into(),
            path: "Vault/Item/Field".into(),
            account: None,
            on_demand: false,
        }),
    );

    editor.active_field = FieldFocus::Row(0);
    assert_eq!(
        editor.focused_unmask_key(),
        Some((super::SecretsScopeTag::Workspace, "A_TOKEN".into()))
    );

    editor.active_field = FieldFocus::Row(1);
    assert_eq!(editor.focused_unmask_key(), None);
}

#[test]
fn editor_focused_secret_enter_plan_reads_current_row() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

    assert_eq!(
        editor.focused_secret_enter_plan(),
        super::SecretsEnterPlan::EditValue {
            scope: super::SecretsScopeTag::Workspace,
            key: "TOKEN".into()
        }
    );

    editor.active_field = FieldFocus::Row(1);
    assert_eq!(
        editor.focused_secret_enter_plan(),
        super::SecretsEnterPlan::Noop
    );

    editor.active_field = FieldFocus::Row(2);
    assert_eq!(
        editor.focused_secret_enter_plan(),
        super::SecretsEnterPlan::OpenScopePicker
    );
}

#[test]
fn editor_focused_secrets_role_expansion_plan_reads_current_row() {
    let workspace = WorkspaceConfig {
        roles: std::collections::BTreeMap::from([("dev".into(), WorkspaceRoleOverride::default())]),
        ..Default::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.active_field = FieldFocus::Row(
        editor
            .secrets_flat_rows()
            .iter()
            .position(|row| matches!(row, SecretsRow::RoleHeader { role, .. } if role == "dev"))
            .expect("role header row"),
    );

    assert_eq!(
        editor.focused_secrets_role_expansion_plan(true),
        RoleHeaderExpansionPlan::Set {
            role: "dev".into(),
            expanded: true
        }
    );

    editor.secrets_expanded.insert("dev".into());
    assert_eq!(
        editor.focused_secrets_role_expansion_plan(true),
        RoleHeaderExpansionPlan::HeaderNoop
    );
    assert_eq!(
        editor.focused_secrets_role_expansion_plan(false),
        RoleHeaderExpansionPlan::Set {
            role: "dev".into(),
            expanded: false
        }
    );
}

#[test]
fn editor_focused_secret_targets_read_current_row() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

    assert_eq!(
        editor.focused_secret_delete_target(),
        Some((super::SecretsScopeTag::Workspace, "TOKEN".into()))
    );
    assert_eq!(
        editor.focused_secret_add_target(),
        Some(super::SecretsScopeTag::Workspace)
    );

    editor.active_field = FieldFocus::Row(1);
    assert_eq!(editor.focused_secret_delete_target(), None);
    assert_eq!(editor.focused_secret_add_target(), None);
}

#[test]
fn editor_change_count_tracks_env_and_role_auth() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    assert_eq!(editor.change_count(), 0);

    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    editor.pending.roles.entry("dev".into()).or_default().github =
        Some(jackin_config::GithubAuthConfig {
            auth_forward: jackin_config::GithubAuthMode::Token,
            ..Default::default()
        });

    assert_eq!(editor.change_count(), 4);
}

#[test]
fn editor_cycle_isolation_for_selected_mount_updates_pending_mount() {
    let mut workspace = WorkspaceConfig::default();
    workspace.mounts.push(MountConfig {
        src: "/host".into(),
        dst: "/work".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.cycle_isolation_for_selected_mount();

    assert_eq!(editor.pending.mounts[0].isolation, MountIsolation::Worktree);
}

#[test]
fn editor_remove_selected_mount_deletes_pending_mount() {
    let mut workspace = WorkspaceConfig::default();
    workspace.mounts.push(MountConfig {
        src: "/host".into(),
        dst: "/work".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    workspace.mounts.push(MountConfig {
        src: "/host2".into(),
        dst: "/work2".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.active_field = FieldFocus::Row(1);

    editor.remove_selected_mount();

    assert_eq!(editor.pending.mounts.len(), 1);
    assert_eq!(editor.pending.mounts[0].src, "/host");
}

#[test]
fn editor_add_shared_mount_appends_pending_mount() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

    editor.add_shared_mount("/host", "/work");

    assert_eq!(editor.pending.mounts.len(), 1);
    assert_eq!(editor.pending.mounts[0].src, "/host");
    assert_eq!(editor.pending.mounts[0].dst, "/work");
    assert_eq!(editor.pending.mounts[0].isolation, MountIsolation::Shared);
}

#[test]
fn editor_eligible_role_override_selectors_use_workspace_allowed_roles() {
    let mut workspace = WorkspaceConfig {
        allowed_roles: vec!["beta".into()],
        ..Default::default()
    };
    workspace.roles.entry("alpha".into()).or_default();
    let editor = TestEditor::new_edit("alpha".into(), workspace);
    let registered = ["alpha".to_owned(), "beta".to_owned(), "bad role".to_owned()];

    let eligible = editor.eligible_role_override_selectors(registered.iter());

    assert_eq!(eligible.len(), 1);
    assert_eq!(eligible[0].name.as_str(), "beta");
}

#[test]
fn editor_auth_role_override_selectors_filter_existing_overrides() {
    let mut workspace = WorkspaceConfig {
        allowed_roles: vec!["alpha".into(), "beta".into()],
        ..Default::default()
    };
    workspace.roles.entry("alpha".into()).or_default().github =
        Some(jackin_config::GithubAuthConfig {
            auth_forward: jackin_config::GithubAuthMode::Token,
            ..Default::default()
        });
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);
    editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
    let registered = ["alpha".to_owned(), "beta".to_owned(), "bad role".to_owned()];

    let eligible = editor
        .auth_role_override_selectors(registered.iter())
        .expect("selected kind should produce candidates");

    assert_eq!(eligible.len(), 1);
    assert_eq!(eligible[0].name.as_str(), "beta");
}

#[test]
fn editor_auth_role_override_selectors_require_selected_kind() {
    let editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let registered = ["alpha".to_owned()];

    assert!(
        editor
            .auth_role_override_selectors(registered.iter())
            .is_none()
    );
}

#[test]
fn editor_toggle_allowed_role_at_cursor_updates_pending_allow_list_and_default() {
    let workspace = WorkspaceConfig {
        default_role: Some("alpha".into()),
        ..Default::default()
    };
    let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
    let mut editor = TestEditor::new_edit("alpha".into(), workspace);

    editor.toggle_allowed_role_at_cursor(&role_names);

    assert_eq!(editor.pending.allowed_roles, vec!["beta".to_owned()]);
    assert_eq!(editor.pending.default_role, None);
}

#[test]
fn editor_toggle_default_role_at_cursor_only_sets_allowed_role() {
    let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_field = FieldFocus::Row(1);

    editor.toggle_default_role_at_cursor(&role_names);
    assert_eq!(editor.pending.default_role.as_deref(), Some("beta"));

    editor.pending.allowed_roles = vec!["alpha".into()];
    editor.pending.default_role = None;
    editor.toggle_default_role_at_cursor(&role_names);
    assert_eq!(editor.pending.default_role, None);
}
