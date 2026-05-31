pub use jackin_console::workspaces::state::{WorkspaceSummary, WorkspaceSummarySource};

impl WorkspaceSummarySource for crate::workspace::WorkspaceConfig {
    fn workdir(&self) -> &str {
        &self.workdir
    }

    fn mount_count(&self) -> usize {
        self.mounts.len()
    }

    fn readonly_mount_count(&self) -> usize {
        self.mounts.iter().filter(|mount| mount.readonly).count()
    }

    fn allowed_role_count(&self) -> usize {
        self.allowed_roles.len()
    }

    fn default_role(&self) -> Option<&str> {
        self.default_role.as_deref()
    }

    fn last_role(&self) -> Option<&str> {
        self.last_role.as_deref()
    }
}

pub(crate) fn workspace_summary_from_config(
    name: &str,
    ws: &crate::workspace::WorkspaceConfig,
) -> WorkspaceSummary {
    WorkspaceSummary::from_source(name, ws)
}
