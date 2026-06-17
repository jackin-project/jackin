use super::ConsoleInstanceAction;
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
