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
