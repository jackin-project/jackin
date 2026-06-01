//! Workspace manager TUI — list, create, edit, delete workspaces from
//! the operator console. Reached via `m` from the Workspace picker stage.

pub(crate) mod effects;

pub(crate) use effects::poll_background_messages;

impl jackin_console::github_mounts::WorkspaceMounts for crate::workspace::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|mount| mount.src.as_str())
    }
}
