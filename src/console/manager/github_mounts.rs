pub(crate) use jackin_console::github_mounts::{WorkspaceMounts, resolve_for_workspace};

impl WorkspaceMounts for crate::workspace::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|mount| mount.src.as_str())
    }
}
