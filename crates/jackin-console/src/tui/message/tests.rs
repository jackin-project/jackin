use super::{
    AgentPickerChoices, ConsoleInstanceAction, agent_picker_choices_for_workspace,
    launch_prompt_should_probe_agents,
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
