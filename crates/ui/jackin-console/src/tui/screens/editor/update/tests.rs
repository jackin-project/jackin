// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.

use super::*;
use jackin_config::{MountConfig, MountIsolation};

#[test]
fn editor_tab_move_plan_resets_local_view_state() {
    assert_eq!(
        editor_tab_move_plan(EditorTab::Secrets, 1, true),
        EditorTabMovePlan {
            active_tab: EditorTab::Auth,
            tab_bar_focused: true,
            active_row: 0,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            clear_secret_view_state: true,
        }
    );
}

#[test]
fn editor_tab_move_plan_clears_auth_kind_when_leaving_auth() {
    assert_eq!(
        editor_tab_move_plan(EditorTab::Auth, 1, false),
        EditorTabMovePlan {
            active_tab: EditorTab::General,
            tab_bar_focused: false,
            active_row: 0,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            clear_secret_view_state: false,
        }
    );
}

#[test]
fn editor_tab_select_plan_focuses_tab_and_clears_departed_state() {
    assert_eq!(
        editor_tab_select_plan(EditorTab::Secrets, EditorTab::Mounts),
        EditorTabSelectPlan {
            active_tab: EditorTab::Mounts,
            tab_bar_focused: true,
            active_row: 0,
            workspace_mounts_scroll_focused: false,
            clear_secret_view_state: true,
        }
    );
}

#[test]
fn editor_tab_bar_focus_plan_returns_requested_focus() {
    assert!(editor_tab_bar_focus_plan(true));
    assert!(!editor_tab_bar_focus_plan(false));
}

#[test]
fn editor_tab_at_position_maps_tab_strip_cells() {
    assert_eq!(
        editor_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(EditorTab::General)
    );
    assert_eq!(
        editor_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(EditorTab::Mounts)
    );
    assert_eq!(
        editor_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn editor_tab_hover_plan_maps_strip() {
    assert_eq!(
        editor_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(0)
    );
    assert_eq!(
        editor_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(1)
    );
    assert_eq!(
        editor_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn editor_tab_hover_target_plan_maps_strip_when_no_modal() {
    assert_eq!(
        editor_tab_hover_target_plan(false, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(EditorHoverTarget::Tab(0))
    );
    assert_eq!(
        editor_tab_hover_target_plan(true, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        None
    );
}

#[test]
fn editor_mount_hover_target_at_position_maps_mount_rows() {
    let mounts = vec![
        MountConfig {
            src: "/src".into(),
            dst: "/dst".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
        MountConfig {
            src: "/src2".into(),
            dst: "/dst2".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
    ];
    let area = ratatui::layout::Rect::new(0, 4, 80, 8);
    assert_eq!(
        editor_mount_index_at_position(EditorTab::Mounts, false, area, 1, 6, 0, &mounts),
        Some(0)
    );
    assert_eq!(
        editor_mount_hover_target_at_position(EditorTab::Mounts, false, area, 1, 6, 0, &mounts),
        Some(EditorHoverTarget::MountRow(0))
    );
    assert_eq!(
        editor_mount_hover_target_at_position(EditorTab::General, false, area, 1, 6, 0, &mounts),
        None
    );
    assert_eq!(
        editor_mount_hover_target_at_position(EditorTab::Mounts, true, area, 1, 6, 0, &mounts),
        None
    );
}

#[test]
fn editor_general_field_modal_plan_routes_editable_rows() {
    assert_eq!(
        editor_general_field_modal_plan(EditorTab::General, 0, false),
        EditorGeneralFieldModalPlan::RenameWorkspace
    );
    assert_eq!(
        editor_general_field_modal_plan(EditorTab::General, 1, true),
        EditorGeneralFieldModalPlan::PickWorkdir
    );
    assert_eq!(
        editor_general_field_modal_plan(EditorTab::General, 1, false),
        EditorGeneralFieldModalPlan::None
    );
    assert_eq!(
        editor_general_field_modal_plan(EditorTab::Mounts, 0, true),
        EditorGeneralFieldModalPlan::None
    );
}

#[test]
fn editor_field_selection_plan_skips_rows_and_updates_scroll() {
    let plan = editor_field_selection_plan(1, 1, 4, &[2], 0, 8, 0);
    assert_eq!(plan.active_row, 3);
    assert!(plan.tab_scroll_y > 0);
}

#[test]
fn skipped_row_helpers_mark_inert_editor_rows() {
    let secrets = [
        SecretsRow::WorkspaceAddSentinel,
        SecretsRow::SectionSpacer,
        SecretsRow::RoleHeader {
            role: "alpha".to_owned(),
            expanded: false,
        },
    ];
    assert_eq!(secrets_skipped_rows(&secrets), vec![1]);
    assert_eq!(editor_secrets_selection_bounds(&secrets), (2, vec![1]));

    let auth: [AuthRow<TestAuthKind>; 3] = [
        AuthRow::WorkspaceMode {
            kind: TestAuthKind::Claude,
        },
        AuthRow::Account {
            id: "personal".into(),
        },
        AuthRow::Binding {
            agent: jackin_core::Agent::Claude,
            role: None,
        },
    ];
    assert!(auth_skipped_rows(&auth).is_empty());
}

#[test]
fn editor_selection_bounds_routes_tab_specific_rows() {
    let secrets = [
        SecretsRow::WorkspaceAddSentinel,
        SecretsRow::SectionSpacer,
        SecretsRow::RoleHeader {
            role: "alpha".to_owned(),
            expanded: false,
        },
    ];
    let auth: [AuthRow<TestAuthKind>; 3] = [
        AuthRow::WorkspaceMode {
            kind: TestAuthKind::Claude,
        },
        AuthRow::Account {
            id: "personal".into(),
        },
        AuthRow::Binding {
            agent: jackin_core::Agent::Claude,
            role: None,
        },
    ];

    assert_eq!(
        editor_selection_bounds(EditorTab::Secrets, 9, 8, &secrets, &auth),
        (2, vec![1])
    );
    assert_eq!(
        editor_selection_bounds(EditorTab::Auth, 9, 8, &secrets, &auth),
        (2, vec![])
    );
    assert_eq!(
        editor_selection_bounds(EditorTab::Mounts, 9, 8, &secrets, &auth),
        (9, Vec::new())
    );
}

#[test]
fn editor_max_row_for_tab_uses_tab_specific_counts() {
    assert_eq!(editor_max_row_for_tab(EditorTab::General, 9, 8, 7, 6), 3);
    assert_eq!(editor_max_row_for_tab(EditorTab::Mounts, 9, 8, 7, 6), 9);
    assert_eq!(editor_max_row_for_tab(EditorTab::Roles, 9, 8, 7, 6), 8);
    assert_eq!(editor_max_row_for_tab(EditorTab::Secrets, 9, 8, 7, 6), 6);
    assert_eq!(editor_max_row_for_tab(EditorTab::Auth, 9, 8, 7, 6), 5);
    assert!(editor_mount_add_row_selected(9, 9));
    assert!(!editor_mount_add_row_selected(8, 9));
    assert!(editor_role_add_row_selected(8, 8));
    assert!(!editor_role_add_row_selected(7, 8));
}

#[test]
fn editor_mount_row_select_plan_focuses_workspace_mounts() {
    assert_eq!(
        editor_mount_row_select_plan(4),
        EditorMountRowSelectPlan {
            active_row: 4,
            workspace_mounts_scroll_focused: true,
        }
    );
}

#[test]
fn editor_scroll_focus_plan_routes_by_tab_and_modal() {
    assert_eq!(
        editor_scroll_focus_plan(EditorTab::Mounts, false, true, true),
        EditorScrollFocusPlan {
            workspace_mounts_scroll_focused: true,
            tab_content_scroll_focused: false,
        }
    );
    assert_eq!(
        editor_scroll_focus_plan(EditorTab::Secrets, false, true, true),
        EditorScrollFocusPlan {
            workspace_mounts_scroll_focused: false,
            tab_content_scroll_focused: true,
        }
    );
    assert_eq!(
        editor_scroll_focus_plan(EditorTab::Mounts, true, true, true),
        EditorScrollFocusPlan {
            workspace_mounts_scroll_focused: false,
            tab_content_scroll_focused: false,
        }
    );
}

#[test]
fn editor_horizontal_scroll_plans_update_offset_and_focus() {
    assert_eq!(
        editor_tab_horizontal_scroll_plan(0, 5, 10, 30),
        EditorHorizontalScrollPlan {
            scroll_x: 5,
            workspace_mounts_scroll_focused: false,
            tab_content_scroll_focused: true,
        }
    );
    assert_eq!(
        editor_workspace_mounts_horizontal_scroll_plan(0, 5, 10, 30),
        EditorHorizontalScrollPlan {
            scroll_x: 5,
            workspace_mounts_scroll_focused: true,
            tab_content_scroll_focused: false,
        }
    );
}

#[derive(Default)]
struct RoleEnv {
    env: BTreeMap<String, &'static str>,
}

#[test]
fn secrets_flat_rows_include_expanded_role_keys() {
    let workspace_env = BTreeMap::from([("GLOBAL".to_owned(), "x")]);
    let roles = BTreeMap::from([(
        "alpha".to_owned(),
        RoleEnv {
            env: BTreeMap::from([("ROLE_KEY".to_owned(), "x")]),
        },
    )]);
    let rows = secrets_flat_rows(
        &workspace_env,
        &roles,
        &BTreeSet::from(["alpha".to_owned()]),
        |role| &role.env,
    );

    assert!(matches!(rows[0], SecretsRow::WorkspaceKeyRow(_)));
    assert!(rows.iter().any(
        |row| matches!(row, SecretsRow::RoleHeader { role, expanded: true } if role == "alpha")
    ));
    assert!(
            rows.iter()
                .any(|row| matches!(row, SecretsRow::RoleKeyRow { role, key } if role == "alpha" && key == "ROLE_KEY"))
        );
    assert!(
        rows.iter()
            .any(|row| matches!(row, SecretsRow::RoleAddSentinel(role) if role == "alpha"))
    );
}

#[test]
fn secrets_flat_rows_collapse_role_keys() {
    let workspace_env = BTreeMap::new();
    let roles = BTreeMap::from([(
        "alpha".to_owned(),
        RoleEnv {
            env: BTreeMap::from([("ROLE_KEY".to_owned(), "x")]),
        },
    )]);
    let rows = secrets_flat_rows(&workspace_env, &roles, &BTreeSet::new(), |role| &role.env);

    assert!(matches!(rows[0], SecretsRow::WorkspaceAddSentinel));
    assert!(rows.iter().any(
        |row| matches!(row, SecretsRow::RoleHeader { role, expanded: false } if role == "alpha")
    ));
    assert!(!rows.iter().any(
            |row| matches!(row, SecretsRow::RoleKeyRow { role, key } if role == "alpha" && key == "ROLE_KEY")
        ));
}

#[test]
fn forbidden_secret_keys_follow_scope() {
    let workspace_env = BTreeMap::from([("GLOBAL".to_owned(), "x")]);
    let roles = BTreeMap::from([(
        "alpha".to_owned(),
        RoleEnv {
            env: BTreeMap::from([("ROLE_KEY".to_owned(), "x")]),
        },
    )]);

    assert_eq!(
        forbidden_secret_keys(
            &workspace_env,
            &roles,
            &SecretsScopeTag::Workspace,
            |role| { &role.env }
        ),
        vec!["GLOBAL".to_owned()]
    );
    assert_eq!(
        forbidden_secret_keys(
            &workspace_env,
            &roles,
            &SecretsScopeTag::Role("alpha".into()),
            |role| &role.env
        ),
        vec!["ROLE_KEY".to_owned()]
    );
}

#[test]
fn set_secret_value_creates_and_expands_role_scope() {
    let mut workspace_env = BTreeMap::new();
    let mut roles = BTreeMap::<String, RoleEnv>::new();
    let mut expanded = BTreeSet::new();

    set_secret_value(
        &mut workspace_env,
        &mut roles,
        &mut expanded,
        &SecretsScopeTag::Role("alpha".into()),
        "TOKEN",
        "secret",
        |roles, role| {
            roles.entry(role.to_owned()).or_default();
        },
        |role| &mut role.env,
    );

    assert_eq!(roles["alpha"].env.get("TOKEN"), Some(&"secret"));
    assert!(expanded.contains("alpha"));
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAuthKind {
    Claude,
    Github,
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
