// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn secret_row_targets_follow_scope() {
    let workspace = SecretsRow::WorkspaceKeyRow("TOKEN".to_owned());
    let role = SecretsRow::RoleAddSentinel("alpha".to_owned());

    assert_eq!(
        secret_delete_target_for_row(Some(&workspace)),
        Some((SecretsScopeTag::Workspace, "TOKEN".to_owned()))
    );
    assert_eq!(
        secret_add_target_for_row(Some(&role)),
        Some(SecretsScopeTag::Role("alpha".to_owned()))
    );
    assert_eq!(
        secret_picker_target_for_row(Some(&role)),
        Some((SecretsScopeTag::Role("alpha".to_owned()), None))
    );
    assert_eq!(
        secret_unmask_target_for_row(Some(&workspace), |_, _| true),
        Some((SecretsScopeTag::Workspace, "TOKEN".to_owned()))
    );
    assert_eq!(
        secret_unmask_target_for_row(Some(&workspace), |_, _| false),
        None
    );
}

#[test]
fn secret_enter_plan_handles_values_and_headers() {
    let key = SecretsRow::RoleKeyRow {
        role: "alpha".to_owned(),
        key: "TOKEN".to_owned(),
    };
    let collapsed = SecretsRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: false,
    };
    let expanded = SecretsRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        secret_enter_plan_for_row(Some(&key), |_, _| true),
        SecretsEnterPlan::EditValue {
            scope: SecretsScopeTag::Role("alpha".to_owned()),
            key: "TOKEN".to_owned()
        }
    );
    assert_eq!(
        secret_enter_plan_for_row(Some(&key), |_, _| false),
        SecretsEnterPlan::Noop
    );
    assert_eq!(
        secret_enter_plan_for_row(Some(&collapsed), |_, _| true),
        SecretsEnterPlan::ExpandRole("alpha".to_owned())
    );
    assert_eq!(
        secret_enter_plan_for_row(Some(&expanded), |_, _| true),
        SecretsEnterPlan::Noop
    );
}

#[test]
fn cycle_mount_isolation_at_rotates_selected_mount_only() {
    let mut mounts = vec![
        MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
        MountConfig {
            src: "/b".into(),
            dst: "/b".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        },
    ];

    cycle_mount_isolation_at(&mut mounts, 0);
    assert_eq!(mounts[0].isolation, MountIsolation::Worktree);
    assert_eq!(mounts[1].isolation, MountIsolation::Worktree);

    cycle_mount_isolation_at(&mut mounts, 0);
    assert_eq!(mounts[0].isolation, MountIsolation::Clone);

    cycle_mount_isolation_at(&mut mounts, 0);
    assert_eq!(mounts[0].isolation, MountIsolation::Shared);

    cycle_mount_isolation_at(&mut mounts, 99);
    assert_eq!(mounts[0].isolation, MountIsolation::Shared);
}

#[test]
fn editor_mount_index_at_visual_row_maps_header_rows_and_add_sentinel() {
    let mounts = vec![
        MountConfig {
            src: "/a".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
        MountConfig {
            src: "/host/b".into(),
            dst: "/work/b".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
    ];

    assert_eq!(editor_mount_index_at_visual_row(&mounts, 0), None);
    assert_eq!(editor_mount_index_at_visual_row(&mounts, 1), Some(0));
    assert_eq!(editor_mount_index_at_visual_row(&mounts, 2), Some(1));
    assert_eq!(editor_mount_index_at_visual_row(&mounts, 3), Some(1));
    assert_eq!(editor_mount_index_at_visual_row(&mounts, 4), None);
    assert_eq!(editor_mount_index_at_visual_row(&mounts, 5), Some(2));
}

#[test]
fn toggle_allowed_role_demotes_all_and_clears_default() {
    let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
    let mut allowed_roles = Vec::new();
    let mut default_role = Some("alpha".to_owned());

    toggle_allowed_role_at(&mut allowed_roles, &mut default_role, &role_names, 0);

    assert_eq!(allowed_roles, vec!["beta".to_owned()]);
    assert_eq!(default_role, None);
}

#[test]
fn toggle_allowed_role_collapses_full_roster_to_all() {
    let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
    let mut allowed_roles = vec!["alpha".to_owned()];
    let mut default_role = None;

    toggle_allowed_role_at(&mut allowed_roles, &mut default_role, &role_names, 1);

    assert!(allowed_roles.is_empty());
}

#[test]
fn add_role_to_workspace_editor_adds_missing_role_only_in_filtered_mode() {
    let role_names = ["alpha".to_owned(), "beta".to_owned()];
    let mut all_allowed = Vec::new();

    assert_eq!(
        add_role_to_workspace_editor(&mut all_allowed, role_names.iter(), "beta"),
        Some(1)
    );
    assert!(all_allowed.is_empty());

    let mut filtered = vec!["alpha".to_owned()];
    assert_eq!(
        add_role_to_workspace_editor(&mut filtered, role_names.iter(), "beta"),
        Some(1)
    );
    assert_eq!(filtered, vec!["alpha".to_owned(), "beta".to_owned()]);
}

#[test]
fn add_role_to_workspace_editor_returns_none_for_unknown_role() {
    let role_names = ["alpha".to_owned()];
    let mut filtered = vec!["alpha".to_owned()];

    assert_eq!(
        add_role_to_workspace_editor(&mut filtered, role_names.iter(), "ghost"),
        None
    );
    assert_eq!(filtered, vec!["alpha".to_owned(), "ghost".to_owned()]);
}

#[test]
fn toggle_default_role_requires_effective_allowance() {
    let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
    let mut default_role = None;

    toggle_default_role_at(&["alpha".to_owned()], &mut default_role, &role_names, 1);
    assert_eq!(default_role, None);

    toggle_default_role_at(&["alpha".to_owned()], &mut default_role, &role_names, 0);
    assert_eq!(default_role.as_deref(), Some("alpha"));

    toggle_default_role_at(&["alpha".to_owned()], &mut default_role, &role_names, 0);
    assert_eq!(default_role, None);
}

#[test]
fn account_rows_are_focusable_and_github_rows_open_forms() {
    let rows: [AuthRow<TestAuthKind>; 4] = [
        AuthRow::Account {
            id: "personal".into(),
        },
        AuthRow::Binding {
            agent: jackin_core::Agent::Claude,
            role: None,
        },
        AuthRow::WorkspaceMode {
            kind: TestAuthKind::Github,
        },
        AuthRow::RoleMode {
            role: "dev".into(),
            kind: TestAuthKind::Github,
        },
    ];
    assert!(rows.iter().all(auth_row_is_focusable));
    assert_eq!(auth_focusable_index_at_visual_row(&rows, 0), Some(0));
    assert_eq!(auth_focusable_index_at_visual_row(&rows, 4), None);
    assert!(resolve_auth_form_target(&rows, 0).is_none());
    assert!(resolve_auth_form_target(&rows, 1).is_none());
    assert!(matches!(
        resolve_auth_form_target(&rows, 2),
        Some(AuthFormTarget::Workspace {
            kind: TestAuthKind::Github
        })
    ));
    assert!(matches!(
        resolve_auth_form_target(&rows, 3),
        Some(AuthFormTarget::WorkspaceRole { .. })
    ));
}
