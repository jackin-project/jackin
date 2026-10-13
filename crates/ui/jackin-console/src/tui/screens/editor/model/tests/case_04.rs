// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn editor_delete_env_var_save_diff_preserves_role_github_token() {
    let github = jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: [(
            "GH_TOKEN".into(),
            jackin_config::EnvValue::Plain("test".into()),
        )]
        .into(),
    };
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    role.github = Some(github.clone());
    let original = WorkspaceConfig {
        roles: [("dev".into(), role)].into(),
        ..Default::default()
    };
    let mut editor = TestEditor::new_edit("alpha".into(), original.clone());

    editor
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert_eq!(
        editor
            .pending
            .roles
            .get("dev")
            .and_then(|role| role.github.as_ref()),
        Some(&github),
        "deleting the final role env key must retain its GitHub token override"
    );
    assert_eq!(
        crate::services::config_save::workspace_save_diff_plan(
            &jackin_core::WorkspaceName::parse("alpha").unwrap(),
            &original,
            &editor.pending,
        ),
        vec![
            crate::services::config_save::WorkspaceSaveDiffOp::EnvRemove {
                scope: jackin_config::EnvScope::WorkspaceRole {
                    workspace: "alpha".into(),
                    role: "dev".into(),
                },
                key: "TOKEN".into(),
            }
        ]
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

    assert!(editor.secret_is_text_editable(&SecretsScopeTag::Workspace, "PLAIN"));
    assert!(!editor.secret_is_text_editable(&SecretsScopeTag::Workspace, "OP_REF"));
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
        Some((SecretsScopeTag::Workspace, "A_TOKEN".into()))
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
        SecretsEnterPlan::EditValue {
            scope: SecretsScopeTag::Workspace,
            key: "TOKEN".into()
        }
    );

    editor.active_field = FieldFocus::Row(1);
    assert_eq!(editor.focused_secret_enter_plan(), SecretsEnterPlan::Noop);

    editor.active_field = FieldFocus::Row(2);
    assert_eq!(
        editor.focused_secret_enter_plan(),
        SecretsEnterPlan::OpenScopePicker
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
        Some((SecretsScopeTag::Workspace, "TOKEN".into()))
    );
    assert_eq!(
        editor.focused_secret_add_target(),
        Some(SecretsScopeTag::Workspace)
    );

    editor.active_field = FieldFocus::Row(1);
    assert_eq!(editor.focused_secret_delete_target(), None);
    assert_eq!(editor.focused_secret_add_target(), None);
}

#[test]
fn editor_change_count_tracks_env_and_role_account_binding() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    assert_eq!(editor.change_count(), 0);

    editor
        .pending
        .env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    editor
        .pending
        .roles
        .entry("dev".into())
        .or_default()
        .account_bindings
        .insert(jackin_core::Agent::Claude, "personal".into());

    assert_eq!(editor.change_count(), 2);
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

#[test]
fn trparity_editor_focus_owner_survives_modal_cancel() {
    use crate::tui::focus::ConsoleFocusTarget;

    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.set_focus_owner(ConsoleFocusTarget::Content(
        EditorFocusTarget::WorkspaceMounts,
    ));
    editor.open_sub_modal(TestStatusModal::Other);

    assert_eq!(
        editor.focus_owner(),
        ConsoleFocusTarget::Content(EditorFocusTarget::WorkspaceMounts)
    );

    editor.dismiss_active_modal();

    assert!(editor.modal.is_none());
    assert_eq!(
        editor.focus_owner(),
        ConsoleFocusTarget::Content(EditorFocusTarget::WorkspaceMounts)
    );
}

#[test]
fn trparity_editor_focus_owner_survives_modal_commit() {
    use crate::tui::focus::ConsoleFocusTarget;

    let mut editor =
        TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.set_focus_owner(ConsoleFocusTarget::Content(
        EditorFocusTarget::WorkspaceMounts,
    ));
    editor.open_sub_modal(TestStatusModal::Status);
    editor.open_sub_modal(TestStatusModal::Other);

    // Commit path: clear the whole chain.
    editor.clear_modal_chain();
    assert!(editor.modal.is_none());
    assert!(!editor.has_modal_parent());
    assert_eq!(
        editor.focus_owner(),
        ConsoleFocusTarget::Content(EditorFocusTarget::WorkspaceMounts)
    );

    // Pop path from a two-level chain restores the parent modal, focus unchanged.
    editor.open_sub_modal(TestStatusModal::Status);
    editor.open_sub_modal(TestStatusModal::Other);
    editor.pop_modal_chain();
    assert!(matches!(editor.modal, Some(TestStatusModal::Status)));
    assert!(!editor.has_modal_parent());
    assert_eq!(
        editor.focus_owner(),
        ConsoleFocusTarget::Content(EditorFocusTarget::WorkspaceMounts)
    );
}
