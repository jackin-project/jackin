// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AgentPickerChoices, AgentPickerResolution, ConsoleInstanceAction, MountInfoRefreshSourceFacts,
    MountInfoRefreshTarget, OnPromptFailure, PromptOutcome, agent_picker_choices_for_workspace,
    launch_agent_prompt_plan, launch_prompt_should_probe_agents, mount_info_refresh_source_plan,
};
use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

#[test]
fn console_instance_actions_map_to_workspace_facts() {
    assert_eq!(
        ConsoleInstanceAction::<()>::Reconnect.workspace_action_fact(),
        WorkspaceInstanceAction::Reconnect
    );
    assert_eq!(
        ConsoleInstanceAction::<()>::ReconnectFocus(7).workspace_action_fact(),
        WorkspaceInstanceAction::Reconnect
    );
    assert_eq!(
        ConsoleInstanceAction::<()>::NewSession.workspace_action_fact(),
        WorkspaceInstanceAction::NewSession
    );
    assert_eq!(
        ConsoleInstanceAction::NewSessionWithAgent(()).workspace_action_fact(),
        WorkspaceInstanceAction::NewSession
    );
    assert_eq!(
        ConsoleInstanceAction::<()>::Shell.workspace_action_fact(),
        WorkspaceInstanceAction::Shell
    );
    assert_eq!(
        ConsoleInstanceAction::<()>::Inspect.workspace_action_fact(),
        WorkspaceInstanceAction::Inspect
    );
    assert_eq!(
        ConsoleInstanceAction::<()>::Stop.workspace_action_fact(),
        WorkspaceInstanceAction::Stop
    );
    assert_eq!(
        ConsoleInstanceAction::<()>::Purge.workspace_action_fact(),
        WorkspaceInstanceAction::Purge
    );
}

#[test]
fn agent_picker_choices_skip_when_workspace_has_default_agent() {
    let choices = agent_picker_choices_for_workspace(true, AgentPickerChoices::Choices(vec![1, 2]));

    assert!(matches!(choices, AgentPickerChoices::NotNeeded));
}

#[test]
fn agent_picker_choices_preserve_probe_result_when_prompt_needed() {
    let choices =
        agent_picker_choices_for_workspace(false, AgentPickerChoices::Choices(vec!["a", "b"]));

    match choices {
        AgentPickerChoices::Choices(choices) => assert_eq!(choices, vec!["a", "b"]),
        AgentPickerChoices::NotNeeded | AgentPickerChoices::Failed(_) => {
            panic!("expected choices to be preserved")
        }
    }
}

#[test]
fn launch_prompt_probe_policy_skips_workspace_default_agent() {
    assert!(!launch_prompt_should_probe_agents(true));
    assert!(launch_prompt_should_probe_agents(false));
}

#[test]
fn launch_agent_prompt_plan_routes_opened_and_not_needed() {
    let opened =
        launch_agent_prompt_plan(AgentPickerResolution::Opened, OnPromptFailure::ClearPending);
    assert_eq!(opened.outcome, PromptOutcome::Defer);
    assert!(opened.store_pending_launch);
    assert!(opened.error.is_none());

    let not_needed = launch_agent_prompt_plan(
        AgentPickerResolution::NotNeeded,
        OnPromptFailure::RestorePending,
    );
    assert_eq!(not_needed.outcome, PromptOutcome::Launch);
    assert!(!not_needed.store_pending_launch);
    assert!(not_needed.error.is_none());
}

#[test]
fn launch_agent_prompt_plan_restores_pending_only_when_requested() {
    let clear = launch_agent_prompt_plan(
        AgentPickerResolution::Failed(anyhow::anyhow!("missing role")),
        OnPromptFailure::ClearPending,
    );
    assert_eq!(clear.outcome, PromptOutcome::Defer);
    assert!(!clear.store_pending_launch);
    assert!(clear.error.is_some());

    let restore = launch_agent_prompt_plan(
        AgentPickerResolution::Failed(anyhow::anyhow!("missing role")),
        OnPromptFailure::RestorePending,
    );
    assert_eq!(restore.outcome, PromptOutcome::Defer);
    assert!(restore.store_pending_launch);
    assert!(restore.error.is_some());
}

#[test]
fn mount_info_refresh_source_plan_dedups_manager_sources() {
    let plan = mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::ManagerList {
        current_dir: "/repo".to_owned(),
        workspace_mount_sources: vec!["/work".to_owned(), "/repo".to_owned()],
        global_mount_sources: vec!["/global".to_owned(), "/work".to_owned()],
    })
    .expect("manager list should refresh cwd and configured mounts");

    assert_eq!(plan.target, MountInfoRefreshTarget::ManagerList);
    assert_eq!(plan.sources, vec!["/global", "/repo", "/work"]);
}

#[test]
fn mount_info_refresh_source_plan_skips_empty_screen_sources() {
    assert!(
        mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::Editor {
            mount_sources: Vec::new(),
        })
        .is_none()
    );
    assert!(
        mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::SettingsMounts {
            mount_sources: Vec::new(),
        })
        .is_none()
    );
    assert!(mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::Inactive).is_none());
}

#[test]
fn mount_info_refresh_source_plan_keeps_screen_target() {
    let editor = mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::Editor {
        mount_sources: vec!["/b".to_owned(), "/a".to_owned(), "/b".to_owned()],
    })
    .expect("editor mounts should refresh");
    assert_eq!(editor.target, MountInfoRefreshTarget::Editor);
    assert_eq!(editor.sources, vec!["/a", "/b"]);

    let settings = mount_info_refresh_source_plan(MountInfoRefreshSourceFacts::SettingsMounts {
        mount_sources: vec!["/settings".to_owned()],
    })
    .expect("settings mounts should refresh");
    assert_eq!(settings.target, MountInfoRefreshTarget::SettingsMounts);
    assert_eq!(settings.sources, vec!["/settings"]);
}
