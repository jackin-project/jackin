// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn workspace_list_key_plan_routes_navigation_and_actions() {
    assert_eq!(
        workspace_list_key_plan(KeyCode::Esc, false),
        WorkspaceListKeyPlan::Exit
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Left, false),
        WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta: -8 }
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Char('l'), false),
        WorkspaceListKeyPlan::ScrollHorizontal { delta: 8 }
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Up, false),
        WorkspaceListKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Char('J'), true),
        WorkspaceListKeyPlan::ScrollFocusedVertical { delta: 3 }
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Enter, false),
        WorkspaceListKeyPlan::Enter
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Char('r'), false),
        WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Reconnect)
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Char('A'), false),
        WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::NewSession)
    );
    assert_eq!(
        workspace_list_key_plan(KeyCode::Char('p'), false),
        WorkspaceListKeyPlan::ConfirmPurge
    );
    // `?` is intercepted by the dispatcher before this planner runs (proof:
    // tui::input::dispatch::tests); the planner itself maps it to Continue.
    assert_eq!(
        workspace_list_key_plan(KeyCode::Char('?'), false),
        WorkspaceListKeyPlan::Continue
    );
}

#[test]
fn workspace_instance_empty_message_routes_action_messages() {
    assert_eq!(
        workspace_instance_empty_message(WorkspaceInstanceAction::Reconnect),
        no_recoverable_instance_for_workspace_message()
    );
    assert_eq!(
        workspace_instance_empty_message(WorkspaceInstanceAction::NewSession),
        no_running_instance_for_workspace_message()
    );
    assert_eq!(
        workspace_instance_empty_message(WorkspaceInstanceAction::Shell),
        no_running_instance_for_workspace_message()
    );
    assert_eq!(
        workspace_instance_empty_message(WorkspaceInstanceAction::Inspect),
        no_instance_state_for_workspace_message()
    );
    assert_eq!(
        workspace_instance_empty_message(WorkspaceInstanceAction::Stop),
        no_running_instance_to_stop_message()
    );
    assert_eq!(
        workspace_instance_empty_message(WorkspaceInstanceAction::Purge),
        no_purgeable_instance_for_workspace_message()
    );
}

#[test]
fn workspace_list_github_open_plan_routes_workspace_choices() {
    let cache = MountInfoCache::default();
    cache.store_entries([
        (
            "/repo-one".to_owned(),
            crate::mount_info::MountKind::Git {
                branch: crate::mount_info::GitBranch::Named("main".to_owned()),
                origin: Some(crate::mount_info::GitOrigin::Github {
                    remote_url: "git@github.com:owner/one.git".to_owned(),
                    web_url: "https://github.com/owner/one/tree/main".to_owned(),
                }),
            },
        ),
        (
            "/repo-two".to_owned(),
            crate::mount_info::MountKind::Git {
                branch: crate::mount_info::GitBranch::Named("dev".to_owned()),
                origin: Some(crate::mount_info::GitOrigin::Github {
                    remote_url: "git@github.com:owner/two.git".to_owned(),
                    web_url: "https://github.com/owner/two/tree/dev".to_owned(),
                }),
            },
        ),
        ("/plain".to_owned(), crate::mount_info::MountKind::Folder),
    ]);
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
        "one".to_owned(),
        workspace_with_mounts(vec![mount("/repo-one")]),
    );
    config.workspaces.insert(
        "many".to_owned(),
        workspace_with_mounts(vec![
            mount("/repo-one"),
            mount("/repo-two"),
            mount("/plain"),
        ]),
    );

    assert!(matches!(
        workspace_list_github_open_plan(None, &config, &cache),
        GithubOpenPlan::Continue
    ));
    assert!(matches!(
        workspace_list_github_open_plan(Some("missing"), &config, &cache),
        GithubOpenPlan::Continue
    ));
    assert!(matches!(
        workspace_list_github_open_plan(Some("one"), &config, &cache),
        GithubOpenPlan::OpenUrl(url) if url == "https://github.com/owner/one/tree/main"
    ));
    assert!(matches!(
        workspace_list_github_open_plan(Some("many"), &config, &cache),
        GithubOpenPlan::Pick(picker) if picker.choices.len() == 2
    ));
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
fn workspace_list_new_session_open_plan_routes_lookup_results() {
    assert_eq!(
        workspace_list_new_session_open_plan(
            WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
                workspace_idx: 2,
                instance_idx: 5,
            },
            |workspace_idx, instance_idx| {
                (workspace_idx == 2 && instance_idx == 5).then(|| "abc123".to_owned())
            },
        ),
        WorkspaceListNewSessionOpenPlan::OpenPicker {
            container: "abc123".to_owned(),
        }
    );

    assert_eq!(
        workspace_list_new_session_open_plan(
            WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
                workspace_idx: 9,
                instance_idx: 1,
            },
            |_, _| None,
        ),
        WorkspaceListNewSessionOpenPlan::OpenInstanceUnavailableError
    );

    assert_eq!(
        workspace_list_new_session_open_plan(
            WorkspaceListNewSessionPlan::CreateWorkspace,
            |_, _| Some("unused".to_owned()),
        ),
        WorkspaceListNewSessionOpenPlan::OpenCreateWorkspace
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
        Some(MountScrollFocus::Global)
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, false, true, false).scroll_focus,
        Some(MountScrollFocus::RoleGlobal)
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, false, false, true).scroll_focus,
        Some(MountScrollFocus::Roles)
    );
}

#[test]
fn workspace_list_scroll_target_plans_route_list_names_and_blocks() {
    use crate::tui::focus::MountScrollFocus;

    assert_eq!(
        workspace_list_horizontal_scroll_target_plan(true, Some(MountScrollFocus::Workspace)),
        WorkspaceListScrollTargetPlan::ListNames
    );
    assert_eq!(
        workspace_list_horizontal_scroll_target_plan(false, Some(MountScrollFocus::Global)),
        WorkspaceListScrollTargetPlan::FocusedBlock(MountScrollFocus::Global)
    );
    assert_eq!(
        workspace_list_horizontal_scroll_target_plan(false, None),
        WorkspaceListScrollTargetPlan::None
    );
    assert_eq!(
        workspace_list_vertical_scroll_target_plan(Some(MountScrollFocus::Roles)),
        WorkspaceListScrollTargetPlan::FocusedBlock(MountScrollFocus::Roles)
    );
    assert_eq!(
        workspace_list_vertical_scroll_target_plan(None),
        WorkspaceListScrollTargetPlan::None
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
fn workspace_list_mouse_plan_routes_seam_drag_and_row_selection() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        Some(ManagerListRow::SavedWorkspace(0)),
        None,
        Some(ManagerListRow::NewWorkspace),
    ];
    let term = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 20,
    };

    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                30,
                4,
            ),
            term,
            30,
            None,
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::StartDrag(crate::tui::split::DragState {
            anchor_pct: 30,
            anchor_x: 30,
        })
    );
    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                10,
                4,
            ),
            term,
            30,
            None,
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::SelectRow(ManagerListRow::SavedWorkspace(0))
    );
    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                10,
                5,
            ),
            term,
            30,
            None,
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::Continue
    );
}
