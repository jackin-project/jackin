/// Logical row in the manager list. Prefer over the raw `selected:
/// usize` when reasoning about what the operator is pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerListRow {
    CurrentDirectory,
    /// An active instance under the synthetic "Current directory" row.
    /// `instance_idx` is the position within the current-directory
    /// active-instance list.
    CurrentDirectoryInstance(usize),
    SavedWorkspace(usize),
    /// An active instance under a saved workspace. `(workspace_idx,
    /// instance_idx)` where `instance_idx` is the position within that
    /// workspace's active-instance list.
    WorkspaceInstance(usize, usize),
    NewWorkspace,
}

impl ManagerListRow {
    /// Screen index in the selectable row list. Returns `None` for
    /// instance rows because they are injected mid-list when their parent
    /// is expanded, so they have no fixed position.
    #[must_use]
    pub const fn to_screen_index(self, saved_count: usize) -> Option<usize> {
        match self {
            Self::CurrentDirectory => Some(0),
            Self::SavedWorkspace(i) => Some(i + 1),
            Self::NewWorkspace => Some(saved_count + 1),
            Self::WorkspaceInstance(_, _) | Self::CurrentDirectoryInstance(_) => None,
        }
    }

    /// Visual-list position including the blank spacer before `NewWorkspace`.
    /// Returns `None` for instance rows for the same reason as
    /// `to_screen_index`.
    #[must_use]
    pub const fn to_visual_index(self, saved_count: usize) -> Option<usize> {
        match self {
            Self::CurrentDirectory => Some(0),
            Self::SavedWorkspace(i) => Some(i + 1),
            Self::NewWorkspace => {
                if saved_count > 0 {
                    Some(saved_count + 2)
                } else {
                    Some(saved_count + 1)
                }
            }
            Self::WorkspaceInstance(_, _) | Self::CurrentDirectoryInstance(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub name: String,
    pub workdir: String,
    pub mount_count: usize,
    pub readonly_mount_count: usize,
    pub allowed_role_count: usize,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
}

pub trait WorkspaceSummarySource {
    fn workdir(&self) -> &str;
    fn mount_count(&self) -> usize;
    fn readonly_mount_count(&self) -> usize;
    fn allowed_role_count(&self) -> usize;
    fn default_role(&self) -> Option<&str>;
    fn last_role(&self) -> Option<&str>;
}

impl WorkspaceSummary {
    pub fn from_source(name: &str, source: &impl WorkspaceSummarySource) -> Self {
        Self {
            name: name.to_string(),
            workdir: source.workdir().to_string(),
            mount_count: source.mount_count(),
            readonly_mount_count: source.readonly_mount_count(),
            allowed_role_count: source.allowed_role_count(),
            default_role: source.default_role().map(str::to_string),
            last_role: source.last_role().map(str::to_string),
        }
    }
}
