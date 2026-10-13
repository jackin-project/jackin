// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn focus_chain_walks_list_then_preview_and_wraps() {
    let order = workspace_list_focus_order();
    assert_eq!(
        order,
        [
            WorkspaceListFocusOwner::ListNames,
            WorkspaceListFocusOwner::Preview
        ]
    );
    assert_eq!(
        workspace_list_focus_head(),
        WorkspaceListFocusOwner::ListNames
    );
    assert_eq!(
        workspace_list_focus_next(WorkspaceListFocusOwner::ListNames),
        WorkspaceListFocusOwner::Preview
    );
    assert_eq!(
        workspace_list_focus_next(WorkspaceListFocusOwner::Preview),
        WorkspaceListFocusOwner::ListNames
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
fn preview_pane_action_plan_routes_key_cursor_and_sessions() {
    assert_eq!(
        preview_pane_action_plan(KeyCode::Esc, Some(1), [11, 22]),
        PreviewPaneActionPlan::ExitPreview
    );
    assert_eq!(
        preview_pane_action_plan(KeyCode::Char('j'), Some(1), [11, 22]),
        PreviewPaneActionPlan::Move { delta: 1 }
    );
    assert_eq!(
        preview_pane_action_plan(KeyCode::Enter, Some(1), [11, 22]),
        PreviewPaneActionPlan::ReconnectSelected { session_id: 22 }
    );
    assert_eq!(
        preview_pane_action_plan(KeyCode::Enter, Some(9), [11, 22]),
        PreviewPaneActionPlan::ReconnectSelected { session_id: 22 }
    );
    assert_eq!(
        preview_pane_action_plan(KeyCode::Enter, Some(0), []),
        PreviewPaneActionPlan::ExitPreview
    );
    assert_eq!(
        preview_pane_action_plan(KeyCode::Tab, Some(0), [11]),
        PreviewPaneActionPlan::Continue
    );
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
fn workspace_list_top_level_key_plan_prioritizes_preview_then_list_keys() {
    assert_eq!(
        workspace_list_top_level_key_plan(
            KeyCode::Char('q'),
            true,
            ManagerListRow::SavedWorkspace(0),
            None,
            false,
        ),
        WorkspaceListTopLevelKeyPlan::PreviewFocused
    );
    assert_eq!(
        workspace_list_top_level_key_plan(
            KeyCode::Right,
            false,
            ManagerListRow::WorkspaceInstance(0, 0),
            Some(2),
            false,
        ),
        WorkspaceListTopLevelKeyPlan::EnterPreview
    );
    assert_eq!(
        workspace_list_top_level_key_plan(
            KeyCode::Right,
            false,
            ManagerListRow::WorkspaceInstance(0, 0),
            Some(0),
            false,
        ),
        WorkspaceListTopLevelKeyPlan::ListKey(WorkspaceListKeyPlan::HorizontalTreeOrScroll {
            delta: 8,
        })
    );
    assert_eq!(
        workspace_list_top_level_key_plan(
            KeyCode::Down,
            false,
            ManagerListRow::SavedWorkspace(0),
            None,
            true,
        ),
        WorkspaceListTopLevelKeyPlan::ListKey(WorkspaceListKeyPlan::ScrollFocusedVertical {
            delta: 3,
        })
    );
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
