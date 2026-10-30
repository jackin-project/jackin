// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::github_mounts::GithubChoice;
use crate::tui::components::ConfirmState;
use crate::tui::components::ErrorPopupState;
use crate::tui::components::container_info_surface::{ContainerInfoRow, ContainerInfoState};
use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::role_picker::RolePickerState;
use crate::tui::model::{
    ConsoleManagerInputPresence, ConsoleManagerStageRoute, ConsoleManagerStageState,
};
use crate::tui::screens::workspaces::update::{
    PreviewFocusState, PreviewPaneCursorState, WorkspaceListHoverState, WorkspaceListSelectionState,
};
use crate::tui::state::{ManagerStage, ManagerState, Modal};
use crate::tui::update::{InlinePickerDismissalState, ListModalState};

fn test_manager<'a>(
    config: &'a jackin_config::AppConfig,
    cwd: &'a std::path::Path,
) -> ManagerState<'a> {
    ManagerState::from_config(config, cwd)
}

fn test_role() -> jackin_core::RoleSelector {
    jackin_core::RoleSelector {
        namespace: None,
        name: "deploy".to_owned(),
    }
}

fn test_account_choice() -> crate::services::launch::AccountChoice {
    crate::services::launch::AccountChoice {
        id: "account-1".to_owned(),
        name: "Work".to_owned(),
        provider: jackin_config::AiProvider::OpenAi,
        agents: vec![jackin_core::Agent::Codex],
        configuration_id: None,
        instance_id: None,
    }
}

#[test]
fn manager_state_is_single_stage_owner() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    assert_eq!(manager.stage.route(), ConsoleManagerStageRoute::List);
    assert!(manager.allows_base_surface_input());
    manager.set_manager_stage(ManagerStage::ConfirmDelete {
        name: "web".to_owned(),
        state: ConfirmState::new("Delete web?"),
    });
    assert_eq!(
        manager.stage.route(),
        ConsoleManagerStageRoute::ConfirmDelete
    );
    assert!(!manager.allows_base_surface_input());
    manager.set_manager_stage(ManagerStage::List);
    assert_eq!(manager.stage.route(), ConsoleManagerStageRoute::List);
    assert!(manager.allows_base_surface_input());
}

#[test]
fn list_modals_open_through_list_modal_state() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    assert!(manager.list_modal.is_none());
    manager.open_container_info_modal(ContainerInfoState::new(
        "web",
        vec![ContainerInfoRow::new("Run ID", "run-1")],
    ));
    assert!(matches!(
        manager.list_modal,
        Some(Modal::ContainerInfo { .. })
    ));
    manager.open_error_popup_modal(ErrorPopupState::new("Save failed", "disk full"));
    assert!(matches!(manager.list_modal, Some(Modal::ErrorPopup { .. })));
    manager.open_github_picker_modal(GithubPickerState::new(vec![GithubChoice {
        src: "src".to_owned(),
        branch: "main".to_owned(),
        url: "https://example.invalid/repo".to_owned(),
    }]));
    assert!(matches!(
        manager.list_modal,
        Some(Modal::GithubPicker { .. })
    ));
    manager.dismiss_list_modal();
    assert!(manager.list_modal.is_none());
    manager.dismiss_list_modal();
    assert!(manager.list_modal.is_none());
}

fn populate_inline_pickers(manager: &mut ManagerState<'_>) {
    manager.inline_new_session_picker = Some((
        "web".to_owned(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Codex,
        ]),
        vec![test_account_choice()],
    ));
    manager.inline_role_picker = Some(RolePickerState::launch(vec![test_role()]));
    manager.inline_agent_picker = Some((
        test_role(),
        crate::tui::components::agent_choice::AgentChoiceState::with_choices(vec![
            jackin_core::Agent::Codex,
        ]),
    ));
    manager.inline_account_picker = Some(
        crate::tui::components::account_picker::AccountPickerState::new(
            "web".to_owned(),
            jackin_core::Agent::Codex,
            vec![test_account_choice()],
        ),
    );
    manager.launch_account_picker = Some(
        crate::tui::components::account_picker::AccountPickerState::new(
            test_role(),
            jackin_core::Agent::Codex,
            vec![test_account_choice()],
        ),
    );
}

fn all_inline_pickers_populated(manager: &ManagerState<'_>) -> bool {
    manager.inline_new_session_picker.is_some()
        && manager.inline_role_picker.is_some()
        && manager.inline_agent_picker.is_some()
        && manager.inline_account_picker.is_some()
        && manager.launch_account_picker.is_some()
}

#[test]
fn inline_picker_dismissal_clears_only_targeted_slot() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    InlinePickerDismissalState::clear_inline_new_session_picker(&mut manager);
    InlinePickerDismissalState::clear_inline_role_picker(&mut manager);
    InlinePickerDismissalState::clear_inline_agent_picker(&mut manager);
    InlinePickerDismissalState::clear_inline_account_picker(&mut manager);
    InlinePickerDismissalState::clear_launch_account_picker(&mut manager);
    populate_inline_pickers(&mut manager);
    assert!(all_inline_pickers_populated(&manager));
    InlinePickerDismissalState::clear_inline_role_picker(&mut manager);
    assert!(manager.inline_role_picker.is_none());
    assert!(manager.inline_new_session_picker.is_some());
    assert!(manager.inline_agent_picker.is_some());
    assert!(manager.inline_account_picker.is_some());
    assert!(manager.launch_account_picker.is_some());
    InlinePickerDismissalState::clear_launch_account_picker(&mut manager);
    assert!(manager.launch_account_picker.is_none());
    assert!(manager.inline_new_session_picker.is_some());
    assert!(manager.inline_agent_picker.is_some());
    assert!(manager.inline_account_picker.is_some());
    populate_inline_pickers(&mut manager);
    InlinePickerDismissalState::clear_inline_new_session_picker(&mut manager);
    InlinePickerDismissalState::clear_inline_agent_picker(&mut manager);
    InlinePickerDismissalState::clear_inline_account_picker(&mut manager);
    assert!(manager.inline_new_session_picker.is_none());
    assert!(manager.inline_agent_picker.is_none());
    assert!(manager.inline_account_picker.is_none());
    assert!(manager.inline_role_picker.is_some());
    assert!(manager.launch_account_picker.is_some());
}

#[test]
fn list_stage_poll_pending_handoffs_are_empty() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    assert!(manager.poll_pending_role_load().is_none());
    assert!(manager.poll_pending_drift_check().is_none());
    assert!(manager.poll_pending_isolation_cleanup().is_none());
    assert!(manager.poll_pending_op_commit().is_none());
    assert!(manager.drain_instance_refresh().is_none());
}

fn push_disclosure_instances(manager: &mut ManagerState<'_>) {
    manager.instances.push(jackin_core::InstanceIndexEntry {
        instance_id: "instance-1".to_owned(),
        container_base: "container-1".to_owned(),
        workspace_name: Some("web".to_owned()),
        workspace_label: "web".to_owned(),
        workdir: "/tmp/jackin-console-web".to_owned(),
        role_key: "role-1".to_owned(),
        agent_runtime: "codex".to_owned(),
        status: jackin_core::InstanceStatus::Active,
        updated_at: "now".to_owned(),
    });
    manager.instances.push(jackin_core::InstanceIndexEntry {
        instance_id: "instance-2".to_owned(),
        container_base: "container-2".to_owned(),
        workspace_name: None,
        workspace_label: "/tmp/jackin-console-test".to_owned(),
        workdir: "/tmp/jackin-console-test".to_owned(),
        role_key: "role-2".to_owned(),
        agent_runtime: "codex".to_owned(),
        status: jackin_core::InstanceStatus::Running,
        updated_at: "now".to_owned(),
    });
}

#[test]
fn workspace_list_selection_and_disclosure_live_on_manager() {
    let mut config = jackin_config::AppConfig::default();
    config.workspaces.insert(
        "web".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/tmp/jackin-console-web".to_owned(),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    assert!(!manager.is_workspace_expanded(0));
    assert!(!manager.current_dir_expanded);
    manager.expand_workspace(0);
    assert!(!manager.is_workspace_expanded(0));
    push_disclosure_instances(&mut manager);
    let collapsed_rows = manager.row_count();
    manager.expand_workspace(0);
    manager.expand_current_dir();
    assert!(manager.is_workspace_expanded(0));
    assert!(manager.current_dir_expanded);
    assert!(manager.row_count() > collapsed_rows);
    manager.set_selected(2);
    assert_eq!(manager.selected, 2);
    manager.reset_list_scroll();
    assert_eq!(manager.selected, 2);
    manager.collapse_workspace(0);
    manager.collapse_current_dir();
    assert!(!manager.is_workspace_expanded(0));
    assert!(!manager.current_dir_expanded);
    assert!(manager.selected < manager.row_count());
    assert!(manager.hover_target.is_none());
    manager.set_workspace_list_hover_target(Some(crate::tui::state::ManagerHoverTarget::ListRow(
        crate::tui::state::ManagerListRow::NewWorkspace,
    )));
    assert!(manager.hover_target.is_some());
    assert_eq!(
        manager.hovered_list_row(),
        Some(crate::tui::state::ManagerListRow::NewWorkspace)
    );
}

#[test]
fn preview_focus_and_pane_cursor_live_on_manager() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    assert!(!manager.preview_focused);
    assert!(manager.preview_pane_cursor.is_empty());
    manager.set_preview_focused(true);
    manager.set_preview_pane_cursor("container-1", 3);
    manager.set_preview_pane_cursor("container-2", 0);
    assert!(manager.preview_focused);
    assert_eq!(manager.preview_pane_cursor.get("container-1"), Some(&3));
    assert_eq!(manager.preview_pane_cursor.get("container-2"), Some(&0));
    manager.set_preview_focused(false);
    assert!(!manager.preview_focused);
    assert_eq!(manager.preview_pane_cursor.get("container-1"), Some(&3));
}
