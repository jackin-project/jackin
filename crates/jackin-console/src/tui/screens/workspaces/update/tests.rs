//! Tests for `update`.
use super::*;
use ratatui::layout::Rect;

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
            clear_inline_provider_picker: false,
            clear_launch_provider_picker: false,
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
            clear_inline_provider_picker: true,
            clear_launch_provider_picker: true,
        }
    );
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

#[test]
fn selected_instance_container_for_action_rejects_disallowed_status() {
    let stopped = WorkspaceInstanceLookupEntry {
        container: "stopped",
        workspace_name: None,
        workspace_label: "/work",
        workdir: "/work",
        status: WorkspaceInstanceStatus::CleanExited,
    };

    assert_eq!(
        selected_instance_container_for_action(
            ManagerListRow::CurrentDirectoryInstance(0),
            WorkspaceInstanceAction::Stop,
            |workspace_idx, instance_idx| {
                (workspace_idx.is_none() && instance_idx == 0).then_some(stopped)
            },
            |_| None,
            [],
        ),
        None
    );
}

#[test]
fn workspace_list_new_session_plan_preserves_existing_instance_only_route() {
    assert_eq!(
        workspace_list_new_session_plan(ManagerListRow::WorkspaceInstance(2, 5)),
        WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
            workspace_idx: 2,
            instance_idx: 5,
        }
    );
    assert_eq!(
        workspace_list_new_session_plan(ManagerListRow::CurrentDirectoryInstance(1)),
        WorkspaceListNewSessionPlan::CreateWorkspace
    );
    assert_eq!(
        workspace_list_new_session_plan(ManagerListRow::SavedWorkspace(3)),
        WorkspaceListNewSessionPlan::CreateWorkspace
    );
    assert_eq!(
        workspace_list_new_session_plan(ManagerListRow::NewWorkspace),
        WorkspaceListNewSessionPlan::CreateWorkspace
    );
}

#[test]
fn workspace_list_scroll_focus_plan_routes_mouse_regions() {
    assert_eq!(
        workspace_list_scroll_focus_plan(true, true, true, true, true, true),
        WorkspaceListScrollFocusPlan {
            list_names_focused: true,
            scroll_focus: None,
        }
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, false, true, false, false, false),
        WorkspaceListScrollFocusPlan {
            list_names_focused: false,
            scroll_focus: None,
        }
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, true, false, false).scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::Global)
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, false, true, false).scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::RoleGlobal)
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, false, false, true).scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::Roles)
    );
}

#[test]
fn workspace_list_hover_row_at_position_skips_seam_spacers_and_unselectable_rows() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        None,
        Some(ManagerListRow::SavedWorkspace(0)),
        Some(ManagerListRow::NewWorkspace),
    ];
    let term = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 12,
    };

    assert_eq!(
        workspace_list_hover_row_at_position(&rows, 1, 3, term, 30, |_| true),
        Some(ManagerListRow::CurrentDirectory)
    );
    assert_eq!(
        workspace_list_hover_row_at_position(&rows, 1, 4, term, 30, |_| true),
        None
    );
    assert_eq!(
        workspace_list_hover_row_at_position(&rows, 1, 5, term, 30, |row| {
            !matches!(row, ManagerListRow::SavedWorkspace(_))
        }),
        None
    );
    assert_eq!(
        workspace_list_hover_row_at_position(&rows, 30, 3, term, 30, |_| true),
        None
    );
}

#[test]
fn workspace_visual_selected_index_skips_spacers() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        None,
        Some(ManagerListRow::SavedWorkspace(0)),
        Some(ManagerListRow::NewWorkspace),
    ];

    assert_eq!(
        workspace_visual_selected_index(&rows, ManagerListRow::SavedWorkspace(0)),
        Some(2)
    );
    assert_eq!(
        workspace_visual_selected_index(&rows, ManagerListRow::WorkspaceInstance(0, 0)),
        None
    );
}

#[test]
fn workspace_row_lookup_helpers_handle_selectable_and_visual_rows() {
    let rows = [
        ManagerListRow::CurrentDirectory,
        ManagerListRow::SavedWorkspace(0),
        ManagerListRow::NewWorkspace,
    ];
    let visual_rows = [
        Some(ManagerListRow::CurrentDirectory),
        None,
        Some(ManagerListRow::SavedWorkspace(0)),
        Some(ManagerListRow::NewWorkspace),
    ];

    assert_eq!(
        workspace_row_index(&rows, ManagerListRow::SavedWorkspace(0)),
        Some(1)
    );
    assert_eq!(
        workspace_row_at(&rows, 2),
        Some(ManagerListRow::NewWorkspace)
    );
    assert_eq!(workspace_row_at(&rows, 9), None);
    assert_eq!(
        workspace_selected_row(&rows, 9),
        ManagerListRow::CurrentDirectory
    );
    assert_eq!(workspace_row_at_visual_index(&visual_rows, 1), None);
    assert_eq!(
        workspace_row_at_visual_index(&visual_rows, 2),
        Some(ManagerListRow::SavedWorkspace(0))
    );
    assert_eq!(workspace_last_selectable_index(rows.len()), 2);
    assert_eq!(workspace_last_selectable_index(0), 0);
    assert_eq!(selected_index(9, rows.len()), 2);
    assert_eq!(selected_index(9, 0), 0);
}

#[test]
fn destructive_confirm_states_name_targets() {
    let delete = workspace_delete_confirm_plan("alpha".to_owned());
    let delete_debug = format!("{:?}", delete.state);
    assert_eq!(delete.name, "alpha");
    assert!(delete_debug.contains("Delete"));
    assert!(delete_debug.contains("alpha"));

    let purge = instance_purge_confirm_plan("abc123".to_owned(), "role/dev".to_owned());
    let purge_debug = format!("{:?}", purge.state);
    assert_eq!(purge.container, "abc123");
    assert_eq!(purge.label, "role/dev");
    assert!(purge_debug.contains("Purge"));
    assert!(purge_debug.contains("role/dev"));
    assert!(purge_debug.contains("Cannot be undone"));
}

#[test]
fn tree_disclosure_plans_map_rows_to_actions() {
    assert_eq!(
        collapse_selected_tree_plan(ManagerListRow::WorkspaceInstance(2, 0)),
        WorkspaceTreeDisclosurePlan::CollapseWorkspace(2)
    );
    assert_eq!(
        collapse_selected_tree_plan(ManagerListRow::CurrentDirectoryInstance(0)),
        WorkspaceTreeDisclosurePlan::CollapseCurrentDir
    );
    assert_eq!(
        expand_selected_tree_plan(ManagerListRow::SavedWorkspace(1)),
        WorkspaceTreeDisclosurePlan::ExpandWorkspace(1)
    );
    assert_eq!(
        expand_selected_tree_plan(ManagerListRow::NewWorkspace),
        WorkspaceTreeDisclosurePlan::None
    );
}

#[test]
fn collapse_selection_plans_route_child_rows_to_parent() {
    assert_eq!(
        collapse_current_dir_selection_plan(ManagerListRow::CurrentDirectoryInstance(2)),
        WorkspaceCollapseSelectionPlan::Parent
    );
    assert_eq!(
        collapsed_current_dir_selected_index(ManagerListRow::CurrentDirectoryInstance(2)),
        Some(0)
    );
    assert_eq!(
        collapse_current_dir_selection_plan(ManagerListRow::SavedWorkspace(1)),
        WorkspaceCollapseSelectionPlan::Clamp
    );
    assert_eq!(
        collapsed_current_dir_selected_index(ManagerListRow::SavedWorkspace(1)),
        None
    );
    assert_eq!(
        collapse_workspace_selection_plan(ManagerListRow::WorkspaceInstance(3, 1), 3),
        WorkspaceCollapseSelectionPlan::Parent
    );
    assert_eq!(
        collapse_workspace_selection_plan(ManagerListRow::WorkspaceInstance(4, 1), 3),
        WorkspaceCollapseSelectionPlan::Clamp
    );
    assert_eq!(
        collapse_workspace_selection_plan(ManagerListRow::SavedWorkspace(3), 3),
        WorkspaceCollapseSelectionPlan::Clamp
    );
    let rows = [
        ManagerListRow::CurrentDirectory,
        ManagerListRow::SavedWorkspace(3),
        ManagerListRow::WorkspaceInstance(3, 0),
        ManagerListRow::NewWorkspace,
    ];
    assert_eq!(
        collapsed_workspace_selected_index(&rows, 2, ManagerListRow::WorkspaceInstance(3, 0), 3),
        Some(1)
    );
    assert_eq!(
        collapsed_workspace_selected_index(&rows, 99, ManagerListRow::SavedWorkspace(3), 3),
        Some(3)
    );
}

#[test]
fn workspace_row_ownership_routes_tree_arrows() {
    assert!(workspace_row_owns_left(
        ManagerListRow::CurrentDirectory,
        true,
        true,
        |_| false
    ));
    assert!(!workspace_row_owns_left(
        ManagerListRow::CurrentDirectory,
        true,
        false,
        |_| false
    ));
    assert!(workspace_row_owns_left(
        ManagerListRow::SavedWorkspace(1),
        false,
        false,
        |idx| idx == 1
    ));
    assert!(workspace_row_owns_right(
        ManagerListRow::CurrentDirectory,
        false,
        true,
        |_| false,
        |_| false
    ));
    assert!(workspace_row_owns_right(
        ManagerListRow::SavedWorkspace(1),
        false,
        false,
        |_| false,
        |idx| idx == 1
    ));
    assert!(!workspace_row_owns_right(
        ManagerListRow::WorkspaceInstance(1, 0),
        false,
        true,
        |_| false,
        |_| true
    ));
}

#[test]
fn workspace_list_horizontal_plan_routes_tree_or_scroll() {
    assert_eq!(
        workspace_list_horizontal_plan(
            ManagerListRow::CurrentDirectory,
            -8,
            true,
            true,
            |_| false,
            |_| false,
        ),
        WorkspaceListHorizontalPlan::CollapseTree
    );
    assert_eq!(
        workspace_list_horizontal_plan(
            ManagerListRow::SavedWorkspace(2),
            8,
            false,
            false,
            |_| false,
            |idx| idx == 2,
        ),
        WorkspaceListHorizontalPlan::ExpandTree
    );
    assert_eq!(
        workspace_list_horizontal_plan(
            ManagerListRow::NewWorkspace,
            8,
            false,
            false,
            |_| false,
            |_| false,
        ),
        WorkspaceListHorizontalPlan::Scroll(8)
    );
}

#[test]
fn preview_focus_plans_set_focus_state() {
    assert_eq!(
        enter_preview_focus_plan(),
        PreviewFocusPlan { focused: true }
    );
    assert_eq!(
        exit_preview_focus_plan(),
        PreviewFocusPlan { focused: false }
    );
}

#[test]
fn instance_action_accepts_status_grid_smoke() {
    use WorkspaceInstanceAction as A;
    use WorkspaceInstanceStatus as S;

    assert!(instance_action_accepts_status(A::Stop, S::Running));
    assert!(!instance_action_accepts_status(A::Stop, S::CleanExited));
    assert!(!instance_action_accepts_status(A::Stop, S::Purged));
    assert!(instance_action_accepts_status(A::Purge, S::Running));
    assert!(instance_action_accepts_status(A::Purge, S::PreservedDirty));
    assert!(!instance_action_accepts_status(A::Purge, S::Purged));
    assert!(instance_action_accepts_status(A::Reconnect, S::Crashed));
    assert!(!instance_action_accepts_status(A::Reconnect, S::Purged));
}

#[test]
fn preview_pane_key_plan_routes_navigation() {
    assert_eq!(
        preview_pane_key_plan(KeyCode::Esc, 2),
        PreviewPaneKeyPlan::ExitPreview
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Char('K'), 2),
        PreviewPaneKeyPlan::Move { delta: -1 }
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Down, 2),
        PreviewPaneKeyPlan::Move { delta: 1 }
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Enter, 2),
        PreviewPaneKeyPlan::ReconnectSelected
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Tab, 2),
        PreviewPaneKeyPlan::Continue
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Enter, 0),
        PreviewPaneKeyPlan::ExitPreview
    );
}

#[test]
fn preview_pane_cursor_plan_clamps_current_and_delta() {
    assert_eq!(preview_pane_selected_index(0, Some(4)), None);
    assert_eq!(preview_pane_selected_index(3, Some(9)), Some(2));
    assert_eq!(preview_pane_cursor_plan(0, Some(4), 1), None);
    assert_eq!(preview_pane_cursor_plan(3, None, 1), Some(1));
    assert_eq!(preview_pane_cursor_plan(3, Some(9), 1), Some(2));
    assert_eq!(preview_pane_cursor_plan(3, Some(0), -9), Some(0));
}

#[test]
fn should_enter_preview_pane_requires_instance_row_key_and_panes() {
    assert!(should_enter_preview_pane(
        KeyCode::Tab,
        ManagerListRow::WorkspaceInstance(1, 0),
        2
    ));
    assert!(should_enter_preview_pane(
        KeyCode::Right,
        ManagerListRow::CurrentDirectoryInstance(0),
        1
    ));
    assert!(!should_enter_preview_pane(
        KeyCode::Tab,
        ManagerListRow::SavedWorkspace(1),
        2
    ));
    assert!(!should_enter_preview_pane(
        KeyCode::Down,
        ManagerListRow::WorkspaceInstance(1, 0),
        2
    ));
    assert!(!should_enter_preview_pane(
        KeyCode::Tab,
        ManagerListRow::WorkspaceInstance(1, 0),
        0
    ));
}

#[test]
fn destructive_confirm_plan_routes_commit_cancel_and_continue() {
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Commit(true)),
        DestructiveConfirmPlan::Commit
    );
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Commit(false)),
        DestructiveConfirmPlan::ReturnToList
    );
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Cancel),
        DestructiveConfirmPlan::ReturnToList
    );
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Continue),
        DestructiveConfirmPlan::Continue
    );
}

#[test]
fn workspace_delete_key_plan_carries_remove_payload() {
    assert_eq!(
        workspace_delete_key_plan(ModalOutcome::Commit(true), "alpha".to_owned()),
        WorkspaceDeleteKeyPlan::RemoveWorkspace {
            name: "alpha".to_owned()
        }
    );
    assert_eq!(
        workspace_delete_key_plan(ModalOutcome::Commit(false), "alpha".to_owned()),
        WorkspaceDeleteKeyPlan::ReturnToList
    );
    assert_eq!(
        workspace_delete_key_plan(ModalOutcome::Continue, "alpha".to_owned()),
        WorkspaceDeleteKeyPlan::Continue
    );
}

#[test]
fn instance_purge_key_plan_carries_purge_payload() {
    assert_eq!(
        instance_purge_key_plan(ModalOutcome::Commit(true), "jackin-role-1".to_owned()),
        InstancePurgeKeyPlan::Purge {
            container: "jackin-role-1".to_owned()
        }
    );
    assert_eq!(
        instance_purge_key_plan(ModalOutcome::Cancel, "jackin-role-1".to_owned()),
        InstancePurgeKeyPlan::ReturnToList
    );
    assert_eq!(
        instance_purge_key_plan(ModalOutcome::Continue, "jackin-role-1".to_owned()),
        InstancePurgeKeyPlan::Continue
    );
}

#[test]
fn selected_instance_action_plan_routes_missing_or_found_container() {
    assert_eq!(
        selected_instance_action_plan(Some("jackin-role-1".to_owned())),
        SelectedInstanceActionPlan::Start {
            container: "jackin-role-1".to_owned()
        }
    );
    assert_eq!(
        selected_instance_action_plan(None),
        SelectedInstanceActionPlan::OpenError
    );
}

#[test]
fn selected_instance_purge_confirm_plan_builds_confirm_payload() {
    assert_eq!(
        selected_instance_purge_confirm_plan(Some("jackin-role-1".to_owned()), |container| {
            format!("{container} label")
        }),
        SelectedInstancePurgeConfirmPlan::OpenConfirm {
            container: "jackin-role-1".to_owned(),
            label: "jackin-role-1 label".to_owned()
        }
    );
    assert_eq!(
        selected_instance_purge_confirm_plan(None, |_| "unused".to_owned()),
        SelectedInstancePurgeConfirmPlan::OpenError
    );
}
