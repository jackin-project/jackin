// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn settings_global_mounts_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('s'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenSavePreview
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('s'), false, true, 0, 0),
        SettingsGlobalMountsKeyPlan::ConfirmSensitiveSave
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('h'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta: -8 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('l'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta: 8 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Up, false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Down, false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('r'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ToggleReadonly
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Esc, true, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Esc, false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Enter, false, false, 2, 2),
        SettingsGlobalMountsKeyPlan::OpenAdd
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Enter, false, false, 1, 2),
        SettingsGlobalMountsKeyPlan::Noop
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('a'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenAdd
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('d'), false, false, 0, 1),
        SettingsGlobalMountsKeyPlan::ConfirmRemove
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('d'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::Noop
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('o'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenGithub
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('n'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Rename)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('1'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Source)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('2'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Destination)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('3'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Scope)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('x'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::Noop
    );
}

#[test]
fn global_mount_scope_picker_commit_plan_routes_choices() {
    assert_eq!(
        global_mount_scope_picker_commit_plan(ScopeChoice::AllAgents),
        GlobalMountScopePickerCommitPlan::ApplyAllAgentsScope
    );
    assert_eq!(
        global_mount_scope_picker_commit_plan(ScopeChoice::SpecificAgent),
        GlobalMountScopePickerCommitPlan::OpenRolePicker
    );
}

#[test]
fn global_mount_role_picker_roles_parse_trust_rows() {
    let rows = vec![
        SettingsTrustRow {
            role: "ops".to_owned(),
            git: "https://example.invalid/ops.git".to_owned(),
            trusted: true,
        },
        SettingsTrustRow {
            role: "chainargos/agent-brown".to_owned(),
            git: "https://example.invalid/brown.git".to_owned(),
            trusted: false,
        },
    ];

    let keys = global_mount_role_picker_roles(&rows)
        .into_iter()
        .map(|role| role.key())
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec!["ops".to_owned(), "chainargos/agent-brown".to_owned()]
    );
}

#[test]
fn global_mount_role_picker_open_plan_requires_roles() {
    assert_eq!(
        global_mount_role_picker_open_plan(&[]),
        RolePickerOpenPlan::NoRoles
    );

    let rows = vec![SettingsTrustRow {
        role: "ops".to_owned(),
        git: "https://example.invalid/ops.git".to_owned(),
        trusted: true,
    }];
    assert!(matches!(
        global_mount_role_picker_open_plan(&rows),
        RolePickerOpenPlan::Open(roles) if roles.len() == 1 && roles[0].key() == "ops"
    ));
}

#[test]
fn global_mount_role_picker_commit_plan_sets_draft_scope() {
    let role = RoleSelector::parse("ops").unwrap();
    let mut draft = Some(GlobalMountDraft::default());
    assert_eq!(
        global_mount_role_picker_commit_plan(&mut draft, &role),
        GlobalMountRolePickerCommitPlan::OpenFileBrowser
    );
    assert_eq!(
        draft.as_ref().and_then(|draft| draft.scope.clone()),
        Some("ops".to_owned())
    );

    let mut missing = None;
    assert_eq!(
        global_mount_role_picker_commit_plan(&mut missing, &role),
        GlobalMountRolePickerCommitPlan::MissingDraft
    );
}

#[test]
fn global_mount_github_open_plan_uses_selected_row_cache_entry() {
    let rows = vec![
        jackin_config::GlobalMountRow {
            scope: None,
            name: "plain".to_owned(),
            mount: jackin_config::MountConfig {
                src: "/plain".to_owned(),
                dst: "/jackin/plain".to_owned(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
        },
        jackin_config::GlobalMountRow {
            scope: Some("ops".to_owned()),
            name: "repo".to_owned(),
            mount: jackin_config::MountConfig {
                src: "/repo".to_owned(),
                dst: "/jackin/repo".to_owned(),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
        },
    ];
    let cache = crate::mount_info_cache::MountInfoCache::default();
    cache.store_entries([
        ("/plain".to_owned(), crate::mount_info::MountKind::Folder),
        (
            "/repo".to_owned(),
            crate::mount_info::MountKind::Git {
                branch: crate::mount_info::GitBranch::Named("main".to_owned()),
                origin: Some(crate::mount_info::GitOrigin::Github {
                    remote_url: "git@github.com:owner/repo.git".to_owned(),
                    web_url: "https://github.com/owner/repo/tree/main".to_owned(),
                }),
            },
        ),
    ]);

    assert_eq!(
        global_mount_github_open_plan(&rows, 0, &cache),
        GlobalMountGithubOpenPlan::NoGithubUrl
    );
    assert_eq!(
        global_mount_github_open_plan(&rows, 1, &cache),
        GlobalMountGithubOpenPlan::Open("https://github.com/owner/repo/tree/main".to_owned())
    );
    assert_eq!(
        global_mount_github_open_plan(&rows, 2, &cache),
        GlobalMountGithubOpenPlan::NoSelection
    );
}

#[test]
fn settings_env_text_commit_plan_routes_keys_and_values() {
    let role_scope = SettingsEnvScope::Role("ops".to_owned());
    let target = SettingsEnvTextTarget::EnvKey {
        scope: role_scope.clone(),
    };
    assert_eq!(
        settings_env_text_commit_plan(&target, " ", false),
        SettingsEnvTextCommitPlan::EmptyKey {
            scope: role_scope.clone(),
        }
    );
    assert_eq!(
        settings_env_text_commit_plan(&target, " TOKEN ", true),
        SettingsEnvTextCommitPlan::SetCarriedPickerValue {
            scope: role_scope.clone(),
            key: "TOKEN".to_owned(),
        }
    );
    assert_eq!(
        settings_env_text_commit_plan(&target, " TOKEN ", false),
        SettingsEnvTextCommitPlan::OpenSourcePicker {
            scope: role_scope.clone(),
            key: "TOKEN".to_owned(),
        }
    );
    assert_eq!(
        settings_env_text_commit_plan(
            &SettingsEnvTextTarget::EnvValue {
                scope: SettingsEnvScope::Global,
                key: "TOKEN".to_owned(),
            },
            " value with spaces ",
            false,
        ),
        SettingsEnvTextCommitPlan::SetPlainValue {
            scope: SettingsEnvScope::Global,
            key: "TOKEN".to_owned(),
            value: " value with spaces ".to_owned(),
        }
    );
}

#[test]
fn settings_env_source_picker_commit_plan_routes_key_context() {
    let pending = (SettingsEnvScope::Role("ops".to_owned()), "TOKEN".to_owned());
    assert_eq!(
        settings_env_source_picker_commit_plan(SettingsEnvSourcePickerSelection::Plain, &pending,),
        SettingsEnvSourcePickerCommitPlan::OpenPlainText {
            scope: SettingsEnvScope::Role("ops".to_owned()),
            key: "TOKEN".to_owned(),
        }
    );
    assert_eq!(
        settings_env_source_picker_commit_plan(SettingsEnvSourcePickerSelection::Op, &pending),
        SettingsEnvSourcePickerCommitPlan::OpenOpPicker {
            scope: SettingsEnvScope::Role("ops".to_owned()),
            key: "TOKEN".to_owned(),
        }
    );
}

#[test]
fn settings_env_scope_picker_commit_plan_routes_scope_choices() {
    assert_eq!(
        settings_env_scope_picker_commit_plan(SettingsEnvScopePickerSelection::AllAgents),
        SettingsEnvScopePickerCommitPlan::OpenGlobalKeyInput {
            scope: SettingsEnvScope::Global,
        }
    );
    assert_eq!(
        settings_env_scope_picker_commit_plan(SettingsEnvScopePickerSelection::SpecificAgent),
        SettingsEnvScopePickerCommitPlan::OpenRolePicker
    );
}

#[test]
fn settings_env_role_picker_commit_plan_maps_role_to_scope() {
    let role = RoleSelector::parse("ops").unwrap();
    assert_eq!(
        settings_env_role_picker_commit_plan(&role),
        SettingsEnvRolePickerCommitPlan {
            scope: SettingsEnvScope::Role("ops".to_owned()),
        }
    );
}

#[test]
fn settings_env_role_picker_roles_parse_registered_roles() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::from([
            ("ops".to_owned(), BTreeMap::<String, &'static str>::new()),
            ("chainargos/agent-brown".to_owned(), BTreeMap::new()),
        ]),
    };

    let keys = settings_env_role_picker_roles(&pending)
        .into_iter()
        .map(|role| role.key())
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec!["chainargos/agent-brown".to_owned(), "ops".to_owned()]
    );
}

#[test]
fn settings_env_role_picker_open_plan_requires_roles() {
    let empty = SettingsEnvConfig::<&'static str> {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    };
    assert_eq!(
        settings_env_role_picker_open_plan(&empty),
        RolePickerOpenPlan::NoRoles
    );

    let pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::from([("ops".to_owned(), BTreeMap::<String, &'static str>::new())]),
    };
    assert!(matches!(
        settings_env_role_picker_open_plan(&pending),
        RolePickerOpenPlan::Open(roles) if roles.len() == 1 && roles[0].key() == "ops"
    ));
}

#[test]
fn settings_tab_at_position_maps_tab_strip_cells() {
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(SettingsTab::General)
    );
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(SettingsTab::Mounts)
    );
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn settings_tab_hover_plan_maps_strip() {
    assert_eq!(
        settings_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(0)
    );
    assert_eq!(
        settings_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(1)
    );
    assert_eq!(
        settings_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn settings_tab_hover_target_plan_maps_strip_without_blocking_modals() {
    assert_eq!(
        settings_tab_hover_target_plan(false, false, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(SettingsHoverTarget::Tab(0))
    );
    assert_eq!(
        settings_tab_hover_target_plan(true, false, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        None
    );
    assert_eq!(
        settings_tab_hover_target_plan(false, true, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        None
    );
}
