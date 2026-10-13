// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    editor.active_field = FieldFocus::Row(jackin_core::Agent::ALL.len());
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
    assert_eq!(editor.escape_key_plan(), EditorEscapeKeyPlan::FocusTabBar);

    editor.set_tab_bar_focused(true);
    assert_eq!(
        editor.escape_key_plan(),
        EditorEscapeKeyPlan::ReloadFromConfig
    );

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
        editor.secret_value(&SecretsScopeTag::Workspace, "TOKEN"),
        Some(&jackin_config::EnvValue::Plain("one".into()))
    );
    assert!(
        editor
            .secret_value(&SecretsScopeTag::Role("dev".into()), "ROLE_TOKEN")
            .is_some_and(|value| matches!(value, jackin_config::EnvValue::OpRef(_)))
    );
    assert!(
        editor
            .secret_value(&SecretsScopeTag::Role("missing".into()), "ROLE_TOKEN")
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
        .delete_env_var(&SecretsScopeTag::Workspace, "TOKEN")
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
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert!(!editor.pending.roles.contains_key("dev"));
}

#[test]
fn editor_delete_env_var_preserves_explicit_empty_github_override() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    role.github = Some(jackin_config::GithubAuthConfig::default());
    editor.pending.roles.insert("dev".into(), role);

    editor
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert_eq!(
        editor.pending.roles["dev"].github,
        Some(jackin_config::GithubAuthConfig::default())
    );
}

#[test]
fn editor_delete_env_var_preserves_configured_github_override() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    role.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: std::collections::BTreeMap::from([(
            "GH_TOKEN".into(),
            jackin_config::EnvValue::Plain("test".into()),
        )]),
    });
    editor.pending.roles.insert("dev".into(), role);

    editor
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert_eq!(
        editor.pending.roles["dev"].github,
        Some(jackin_config::GithubAuthConfig {
            auth_forward: jackin_config::GithubAuthMode::Token,
            env: std::collections::BTreeMap::from([(
                "GH_TOKEN".into(),
                jackin_config::EnvValue::Plain("test".into()),
            )]),
        })
    );
}

#[test]
fn editor_delete_env_var_preserves_explicit_empty_default_launch() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    role.default_launch = Some(Vec::new());
    editor.pending.roles.insert("dev".into(), role);

    editor
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert_eq!(editor.pending.roles["dev"].default_launch, Some(Vec::new()));
}

#[test]
fn editor_delete_env_var_preserves_configured_default_launch() {
    let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
    let mut role = WorkspaceRoleOverride::default();
    role.env
        .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
    role.default_launch = Some(vec!["codex-main".into()]);
    editor.pending.roles.insert("dev".into(), role);

    editor
        .delete_env_var(&SecretsScopeTag::Role("dev".into()), "TOKEN")
        .unwrap();

    assert_eq!(
        editor.pending.roles["dev"].default_launch,
        Some(vec!["codex-main".into()])
    );
}
