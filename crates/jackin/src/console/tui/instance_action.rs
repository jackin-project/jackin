//! Root-console instance action adapters.

pub(crate) fn workspace_instance_action_fact(
    action: crate::console::ConsoleInstanceAction,
) -> jackin_console::tui::screens::workspaces::update::WorkspaceInstanceAction {
    action.workspace_action_fact()
}
