// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_preview_focus_plan_updates_state() {
    let mut state = TestPreviewFocus::default();

    apply_preview_focus_plan(&mut state, enter_preview_focus_plan());
    assert!(state.focused);

    apply_preview_focus_plan(&mut state, exit_preview_focus_plan());
    assert!(!state.focused);
}

#[test]
fn apply_preview_pane_cursor_plan_updates_cursor_or_clears_focus() {
    let mut state = TestPreviewFocus {
        focused: true,
        cursor: None,
    };

    apply_preview_pane_cursor_plan(&mut state, "container-a", Some(2));
    assert_eq!(state.cursor, Some(("container-a".to_owned(), 2)));
    assert!(state.focused);

    apply_preview_pane_cursor_plan(&mut state, "container-a", None);
    assert!(!state.focused);
}

#[test]
fn apply_workspace_list_scroll_plans_update_targeted_offsets() {
    let mut state = TestWorkspaceListScroll {
        list_names_x: 4,
        workspace_x: 10,
        workspace_y: 8,
    };

    apply_workspace_list_horizontal_scroll_plan(
        &mut state,
        WorkspaceListScrollTargetPlan::ListNames,
        3,
    );
    assert_eq!(state.list_names_x, 7);

    apply_workspace_list_horizontal_scroll_plan(
        &mut state,
        WorkspaceListScrollTargetPlan::FocusedBlock(MountScrollFocus::Workspace),
        -4,
    );
    assert_eq!(state.workspace_x, 6);

    apply_workspace_list_vertical_scroll_plan(
        &mut state,
        WorkspaceListScrollTargetPlan::FocusedBlock(MountScrollFocus::Workspace),
        -99,
    );
    assert_eq!(state.workspace_y, 0);
}

#[test]
fn workspace_unclamped_scroll_plan_updates_offset() {
    assert_eq!(workspace_unclamped_scroll_plan(4, 3), 7);
    assert_eq!(workspace_unclamped_scroll_plan(4, -99), 0);
}

#[test]
fn workspace_list_selection_plans_clear_expected_pickers() {
    assert_eq!(
        workspace_list_move_selection_plan(0, 3, 1),
        WorkspaceListSelectionPlan {
            selected: 1,
            changed: true,
            clear_inline_role_picker: true,
            clear_inline_agent_picker: true,
            clear_inline_new_session_picker: true,
            clear_inline_account_picker: false,
            clear_launch_account_picker: false,
        }
    );
    assert_eq!(
        workspace_list_select_row_plan(0, 2, 3),
        WorkspaceListSelectionPlan {
            selected: 2,
            changed: true,
            clear_inline_role_picker: true,
            clear_inline_agent_picker: true,
            clear_inline_new_session_picker: true,
            clear_inline_account_picker: true,
            clear_launch_account_picker: true,
        }
    );
}

#[test]
fn apply_workspace_tree_disclosure_plan_routes_mutations() {
    let mut state = TestTreeDisclosure::default();

    apply_workspace_tree_disclosure_plan(&mut state, WorkspaceTreeDisclosurePlan::None);
    apply_workspace_tree_disclosure_plan(
        &mut state,
        WorkspaceTreeDisclosurePlan::CollapseWorkspace(2),
    );
    apply_workspace_tree_disclosure_plan(
        &mut state,
        WorkspaceTreeDisclosurePlan::CollapseCurrentDir,
    );
    apply_workspace_tree_disclosure_plan(
        &mut state,
        WorkspaceTreeDisclosurePlan::ExpandWorkspace(3),
    );
    apply_workspace_tree_disclosure_plan(&mut state, WorkspaceTreeDisclosurePlan::ExpandCurrentDir);

    assert_eq!(
        state.calls,
        [
            "collapse-workspace:2",
            "collapse-current-dir",
            "expand-workspace:3",
            "expand-current-dir",
        ]
    );
}

#[test]
fn apply_workspace_list_selection_plan_clears_and_selects() {
    let mut state = TestListSelection::default();

    apply_workspace_list_selection_plan(
        &mut state,
        WorkspaceListSelectionPlan {
            selected: 4,
            changed: true,
            clear_inline_role_picker: true,
            clear_inline_agent_picker: true,
            clear_inline_new_session_picker: true,
            clear_inline_account_picker: true,
            clear_launch_account_picker: true,
        },
    );

    assert!(state.cleared.role);
    assert!(state.cleared.agent);
    assert!(state.cleared.new_session);
    assert!(state.cleared.provider);
    assert!(state.cleared.launch_account);
    assert!(state.reset_scroll);
    assert_eq!(state.selected, Some(4));
}

#[test]
fn apply_workspace_list_selection_plan_keeps_selection_when_unchanged() {
    let mut state = TestListSelection::default();

    apply_workspace_list_selection_plan(
        &mut state,
        WorkspaceListSelectionPlan {
            selected: 7,
            changed: false,
            clear_inline_role_picker: true,
            clear_inline_agent_picker: false,
            clear_inline_new_session_picker: false,
            clear_inline_account_picker: false,
            clear_launch_account_picker: false,
        },
    );

    assert!(state.cleared.role);
    assert!(!state.reset_scroll);
    assert_eq!(state.selected, None);
}

#[test]
fn apply_workspace_list_hover_target_updates_storage() {
    let mut state = TestListHover::default();
    let target = Some(ManagerHoverTarget::ListRow(ManagerListRow::SavedWorkspace(
        2,
    )));

    apply_workspace_list_hover_target(&mut state, target);
    assert_eq!(state.target, target);

    apply_workspace_list_hover_target(&mut state, None);
    assert_eq!(state.target, None);
}

#[test]
fn initial_workspace_selected_index_prefers_matching_saved_workspace() {
    assert_eq!(initial_workspace_selected_index(3, Some(1)), 2);
    assert_eq!(initial_workspace_selected_index(3, None), 0);
    assert_eq!(initial_workspace_selected_index(0, None), 0);
    assert_eq!(saved_workspace_selected_index(3, 1), 2);
}

#[test]
fn workspace_list_row_action_policies_route_by_row_kind() {
    assert_eq!(
        workspace_list_enter_plan(ManagerListRow::CurrentDirectory),
        WorkspaceListEnterPlan::LaunchCurrentDir
    );
    assert_eq!(
        workspace_list_enter_plan(ManagerListRow::NewWorkspace),
        WorkspaceListEnterPlan::CreateNewWorkspace
    );
    assert_eq!(
        workspace_list_enter_plan(ManagerListRow::SavedWorkspace(3)),
        WorkspaceListEnterPlan::LaunchSavedWorkspace(3)
    );
    assert_eq!(
        workspace_list_enter_plan(ManagerListRow::WorkspaceInstance(1, 2)),
        WorkspaceListEnterPlan::InstanceAction
    );
    assert_eq!(
        workspace_list_saved_workspace_index(ManagerListRow::SavedWorkspace(4)),
        Some(4)
    );
    assert_eq!(
        workspace_list_saved_workspace_index(ManagerListRow::CurrentDirectory),
        None
    );
    assert_eq!(
        workspace_list_edit_plan(ManagerListRow::SavedWorkspace(4)),
        WorkspaceListEditPlan::OpenEditor { workspace_idx: 4 }
    );
    assert_eq!(
        workspace_list_edit_plan(ManagerListRow::CurrentDirectory),
        WorkspaceListEditPlan::Noop
    );
    assert_eq!(
        workspace_list_delete_plan(ManagerListRow::SavedWorkspace(4)),
        WorkspaceListDeletePlan::ConfirmDelete { workspace_idx: 4 }
    );
    assert_eq!(
        workspace_list_delete_plan(ManagerListRow::WorkspaceInstance(4, 0)),
        WorkspaceListDeletePlan::Noop
    );
    assert_eq!(
        workspace_list_settings_plan(ManagerListRow::CurrentDirectory),
        WorkspaceListSettingsPlan::OpenSettings
    );
    assert_eq!(
        workspace_list_settings_plan(ManagerListRow::SavedWorkspace(4)),
        WorkspaceListSettingsPlan::OpenSettings
    );
    assert_eq!(
        workspace_list_settings_plan(ManagerListRow::CurrentDirectoryInstance(0)),
        WorkspaceListSettingsPlan::Noop
    );
    assert!(workspace_list_settings_available(
        ManagerListRow::CurrentDirectory
    ));
    assert!(!workspace_list_settings_available(
        ManagerListRow::CurrentDirectoryInstance(0)
    ));
    assert!(workspace_list_current_directory_selected(
        ManagerListRow::CurrentDirectory
    ));
    assert!(!workspace_list_current_directory_selected(
        ManagerListRow::SavedWorkspace(0)
    ));
    assert!(workspace_list_new_workspace_selected(
        ManagerListRow::NewWorkspace
    ));
    assert!(!workspace_list_new_workspace_selected(
        ManagerListRow::CurrentDirectory
    ));
}

#[test]
fn selected_instance_scope_plan_routes_workspace_contexts() {
    assert_eq!(
        selected_instance_scope_plan(ManagerListRow::CurrentDirectory),
        WorkspaceInstanceScopePlan::CurrentDirectory
    );
    assert_eq!(
        selected_instance_scope_plan(ManagerListRow::CurrentDirectoryInstance(2)),
        WorkspaceInstanceScopePlan::CurrentDirectory
    );
    assert_eq!(
        selected_instance_scope_plan(ManagerListRow::SavedWorkspace(3)),
        WorkspaceInstanceScopePlan::SavedWorkspace(3)
    );
    assert_eq!(
        selected_instance_scope_plan(ManagerListRow::WorkspaceInstance(4, 1)),
        WorkspaceInstanceScopePlan::WorkspaceInstance(4)
    );
    assert_eq!(
        selected_instance_scope_plan(ManagerListRow::NewWorkspace),
        WorkspaceInstanceScopePlan::None
    );
}

#[test]
fn selected_instance_plan_routes_direct_scope_and_empty_rows() {
    assert_eq!(
        selected_instance_plan(ManagerListRow::CurrentDirectoryInstance(2)),
        WorkspaceListSelectedInstancePlan::Direct {
            workspace_idx: None,
            instance_idx: 2,
        }
    );
    assert_eq!(
        selected_instance_plan(ManagerListRow::WorkspaceInstance(3, 4)),
        WorkspaceListSelectedInstancePlan::Direct {
            workspace_idx: Some(3),
            instance_idx: 4,
        }
    );
    assert_eq!(
        selected_instance_plan(ManagerListRow::SavedWorkspace(1)),
        WorkspaceListSelectedInstancePlan::Scope
    );
    assert_eq!(
        selected_instance_plan(ManagerListRow::CurrentDirectory),
        WorkspaceListSelectedInstancePlan::Scope
    );
    assert_eq!(
        selected_instance_plan(ManagerListRow::NewWorkspace),
        WorkspaceListSelectedInstancePlan::None
    );
}

#[test]
fn selected_instance_container_for_action_routes_direct_rows() {
    let direct = WorkspaceInstanceLookupEntry {
        container: "direct-container",
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/work",
        status: WorkspaceInstanceStatus::Running,
    };

    assert_eq!(
        selected_instance_container_for_action(
            ManagerListRow::WorkspaceInstance(3, 4),
            WorkspaceInstanceAction::Reconnect,
            |workspace_idx, instance_idx| {
                (workspace_idx == Some(3) && instance_idx == 4).then_some(direct)
            },
            |_| None,
            [],
        ),
        Some("direct-container")
    );
}

#[test]
fn selected_instance_container_for_action_routes_scope_rows() {
    let instances = [
        WorkspaceInstanceLookupEntry {
            container: "other",
            workspace_name: Some("other"),
            workspace_label: "other",
            workdir: "/other",
            status: WorkspaceInstanceStatus::Running,
        },
        WorkspaceInstanceLookupEntry {
            container: "target",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/work",
            status: WorkspaceInstanceStatus::CleanExited,
        },
    ];

    assert_eq!(
        selected_instance_container_for_action(
            ManagerListRow::SavedWorkspace(1),
            WorkspaceInstanceAction::Inspect,
            |_, _| None,
            |scope| {
                (scope == WorkspaceInstanceScopePlan::SavedWorkspace(1)).then_some(
                    WorkspaceInstanceLookupScope {
                        workspace_name: Some("workspace"),
                        workspace_label: "workspace",
                        workdir: "/work",
                    },
                )
            },
            instances,
        ),
        Some("target")
    );
}
