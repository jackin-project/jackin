//! Root-console instance action adapters.

use jackin_console::tui::screens::workspaces::update::WorkspaceInstanceAction;

use crate::console::ConsoleInstanceAction;

pub(crate) const fn workspace_instance_action_fact(
    action: ConsoleInstanceAction,
) -> WorkspaceInstanceAction {
    match action {
        ConsoleInstanceAction::Reconnect | ConsoleInstanceAction::ReconnectFocus(_) => {
            WorkspaceInstanceAction::Reconnect
        }
        ConsoleInstanceAction::NewSession | ConsoleInstanceAction::NewSessionWithAgent(_) => {
            WorkspaceInstanceAction::NewSession
        }
        ConsoleInstanceAction::Shell => WorkspaceInstanceAction::Shell,
        ConsoleInstanceAction::Inspect => WorkspaceInstanceAction::Inspect,
        ConsoleInstanceAction::Stop => WorkspaceInstanceAction::Stop,
        ConsoleInstanceAction::Purge => WorkspaceInstanceAction::Purge,
    }
}
