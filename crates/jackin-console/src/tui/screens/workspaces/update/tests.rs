//! Tests for `update`.
use super::*;
use crate::tui::components::error_popup::{
    no_instance_state_for_workspace_message, no_purgeable_instance_for_workspace_message,
    no_recoverable_instance_for_workspace_message, no_running_instance_for_workspace_message,
    no_running_instance_to_stop_message,
};
use crate::tui::components::github_picker::GithubOpenPlan;
use crate::tui::focus::MountScrollFocus;
use jackin_config::{MountConfig, WorkspaceConfig};
use ratatui::layout::Rect;

#[derive(Default)]
struct TestPreviewFocus {
    focused: bool,
    cursor: Option<(String, usize)>,
}

impl PreviewFocusState for TestPreviewFocus {
    fn set_preview_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl PreviewPaneCursorState for TestPreviewFocus {
    fn set_preview_pane_cursor(&mut self, container: &str, cursor: usize) {
        self.cursor = Some((container.to_owned(), cursor));
    }
}

#[derive(Default)]
struct TestWorkspaceListScroll {
    list_names_x: u16,
    workspace_x: u16,
    workspace_y: u16,
}

impl WorkspaceListScrollState for TestWorkspaceListScroll {
    fn list_names_scroll_x(&self) -> u16 {
        self.list_names_x
    }

    fn set_list_names_scroll_x(&mut self, value: u16) {
        self.list_names_x = value;
    }

    fn block_scroll_x(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.workspace_x,
            MountScrollFocus::Global | MountScrollFocus::RoleGlobal | MountScrollFocus::Roles => 0,
        }
    }

    fn set_block_scroll_x(&mut self, focus: MountScrollFocus, value: u16) {
        if matches!(focus, MountScrollFocus::Workspace) {
            self.workspace_x = value;
        }
    }

    fn block_scroll_y(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.workspace_y,
            MountScrollFocus::Global | MountScrollFocus::RoleGlobal | MountScrollFocus::Roles => 0,
        }
    }

    fn set_block_scroll_y(&mut self, focus: MountScrollFocus, value: u16) {
        if matches!(focus, MountScrollFocus::Workspace) {
            self.workspace_y = value;
        }
    }
}

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

#[derive(Default)]
struct TestTreeDisclosure {
    calls: Vec<String>,
}

impl WorkspaceTreeDisclosureState for TestTreeDisclosure {
    fn collapse_workspace(&mut self, index: usize) {
        self.calls.push(format!("collapse-workspace:{index}"));
    }

    fn collapse_current_dir(&mut self) {
        self.calls.push("collapse-current-dir".to_owned());
    }

    fn expand_workspace(&mut self, index: usize) {
        self.calls.push(format!("expand-workspace:{index}"));
    }

    fn expand_current_dir(&mut self) {
        self.calls.push("expand-current-dir".to_owned());
    }
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

#[expect(
    clippy::struct_excessive_bools,
    reason = "tracked in codebase-health-enforcement"
)]
#[derive(Default)]
struct TestListSelection {
    cleared_role: bool,
    cleared_agent: bool,
    cleared_new_session: bool,
    cleared_provider: bool,
    cleared_launch_provider: bool,
    reset_scroll: bool,
    selected: Option<usize>,
}

impl WorkspaceListSelectionState for TestListSelection {
    fn clear_inline_role_picker(&mut self) {
        self.cleared_role = true;
    }

    fn clear_inline_agent_picker(&mut self) {
        self.cleared_agent = true;
    }

    fn clear_inline_new_session_picker(&mut self) {
        self.cleared_new_session = true;
    }

    fn clear_inline_provider_picker(&mut self) {
        self.cleared_provider = true;
    }

    fn clear_launch_provider_picker(&mut self) {
        self.cleared_launch_provider = true;
    }

    fn reset_list_scroll(&mut self) {
        self.reset_scroll = true;
    }

    fn set_selected(&mut self, selected: usize) {
        self.selected = Some(selected);
    }
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
            clear_inline_provider_picker: true,
            clear_launch_provider_picker: true,
        },
    );

    assert!(state.cleared_role);
    assert!(state.cleared_agent);
    assert!(state.cleared_new_session);
    assert!(state.cleared_provider);
    assert!(state.cleared_launch_provider);
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
            clear_inline_provider_picker: false,
            clear_launch_provider_picker: false,
        },
    );

    assert!(state.cleared_role);
    assert!(!state.reset_scroll);
    assert_eq!(state.selected, None);
}

#[derive(Default)]
struct TestListHover {
    target: Option<ManagerHoverTarget>,
}

impl WorkspaceListHoverState for TestListHover {
    fn set_workspace_list_hover_target(&mut self, target: Option<ManagerHoverTarget>) {
        self.target = target;
    }
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

fn mount(src: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: "/work".to_owned(),
        readonly: false,
        isolation: jackin_config::MountIsolation::default(),
    }
}

fn workspace_with_mounts(mounts: Vec<MountConfig>) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: "/work".to_owned(),
        mounts,
        ..WorkspaceConfig::default()
    }
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

fn mouse(kind: crossterm::event::MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::empty(),
    }
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

#[test]
fn workspace_list_mouse_plan_routes_drag_update_end_and_modal_gate() {
    let rows = [Some(ManagerListRow::CurrentDirectory)];
    let term = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 20,
    };
    let drag = crate::tui::split::DragState {
        anchor_pct: 30,
        anchor_x: 30,
    };

    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
                55,
                4,
            ),
            term,
            30,
            Some(drag),
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::UpdateSplit(55)
    );
    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                55,
                4,
            ),
            term,
            30,
            Some(drag),
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::EndDrag
    );
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
            true,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::Continue
    );
}

#[test]
fn workspace_list_clickable_at_position_excludes_seam_spacers_and_modal() {
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

    assert!(!workspace_list_clickable_at_position(
        30,
        4,
        term,
        30,
        false,
        &rows,
        |_| true,
    ));
    assert!(workspace_list_clickable_at_position(
        10,
        4,
        term,
        30,
        false,
        &rows,
        |_| true,
    ));
    assert!(!workspace_list_clickable_at_position(
        10,
        5,
        term,
        30,
        false,
        &rows,
        |_| true,
    ));
    assert!(!workspace_list_clickable_at_position(
        10,
        4,
        term,
        30,
        true,
        &rows,
        |_| true,
    ));
}

#[test]
fn workspace_list_clickable_at_position_respects_selectable_rows() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        Some(ManagerListRow::SavedWorkspace(0)),
    ];
    let term = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 20,
    };

    assert!(!workspace_list_clickable_at_position(
        10,
        4,
        term,
        30,
        false,
        &rows,
        |row| row != ManagerListRow::SavedWorkspace(0),
    ));
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
    assert!(purge_debug.contains(
        "Removes the role container, DinD sidecar, volume, network, and local recovery state."
    ));
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
