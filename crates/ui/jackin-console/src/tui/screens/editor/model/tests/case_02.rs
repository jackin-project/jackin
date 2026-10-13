// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        EditorImmediateActionKeyPlan::NotImmediateAction
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
            scope: SecretsScopeTag::Workspace,
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

    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('a')),
        EditorAuthActionKeyPlan::NotAuthAction
    );
    assert_eq!(
        editor.auth_action_key_plan(KeyCode::Char('A')),
        EditorAuthActionKeyPlan::NotAuthAction
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
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Char('a'), KeyModifiers::empty(), true,),
        EditorTabActionKeyPlan::Noop
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
    assert_eq!(
        editor.tab_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty(), true),
        EditorTabActionKeyPlan::Enter(EditorEnterKeyPlan::Auth(AuthEnterPlan::Noop))
    );
}
