use std::collections::BTreeSet;

use super::model::ManagerListRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceInstanceAction {
    Reconnect,
    NewSession,
    Shell,
    Inspect,
    Stop,
    Purge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceInstanceStatus {
    Active,
    Running,
    CleanExited,
    Crashed,
    PreservedDirty,
    PreservedUnpushed,
    RestoreAvailable,
    Superseded,
    Purged,
    FailedSetup,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceRowLayout<'a> {
    pub current_dir_expanded: bool,
    pub current_dir_instance_count: usize,
    pub workspace_instance_counts: &'a [usize],
    pub expanded_workspaces: &'a BTreeSet<usize>,
}

#[must_use]
pub fn selectable_rows(layout: WorkspaceRowLayout<'_>) -> Vec<ManagerListRow> {
    let mut rows = vec![ManagerListRow::CurrentDirectory];
    if layout.current_dir_expanded {
        rows.extend(
            (0..layout.current_dir_instance_count).map(ManagerListRow::CurrentDirectoryInstance),
        );
    }
    for (i, count) in layout.workspace_instance_counts.iter().copied().enumerate() {
        rows.push(ManagerListRow::SavedWorkspace(i));
        if layout.expanded_workspaces.contains(&i) {
            rows.extend((0..count).map(|j| ManagerListRow::WorkspaceInstance(i, j)));
        }
    }
    rows.push(ManagerListRow::NewWorkspace);
    rows
}

#[must_use]
pub fn visual_rows(layout: WorkspaceRowLayout<'_>) -> Vec<Option<ManagerListRow>> {
    let mut rows = selectable_rows(layout)
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();
    if !layout.workspace_instance_counts.is_empty() {
        let insert_at = rows.len().saturating_sub(1);
        rows.insert(insert_at, None);
    }
    rows
}

#[must_use]
pub fn moved_selection(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub fn selected_index(selected: usize, row_count: usize) -> usize {
    crate::focus::selected_index(selected, row_count)
}

/// Action x status acceptance grid. Each arm enumerates the exact set
/// of statuses the action runs against. Positive matching keeps future
/// status variants from becoming accepted by accident.
#[must_use]
pub const fn instance_action_accepts_status(
    action: WorkspaceInstanceAction,
    status: WorkspaceInstanceStatus,
) -> bool {
    use WorkspaceInstanceAction as A;
    use WorkspaceInstanceStatus as S;
    match (action, status) {
        // Reconnect / Inspect: anything that still has on-disk state to read.
        (A::Reconnect | A::Inspect, status) => match status {
            S::Active
            | S::Running
            | S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::FailedSetup => true,
            S::Purged => false,
        },
        // NewSession / Shell / Stop: live container required.
        (A::NewSession | A::Shell | A::Stop, status) => match status {
            S::Active | S::Running => true,
            S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::Purged
            | S::FailedSetup => false,
        },
        // Purge: anything that has not already been purged. Crashed /
        // CleanExited / Preserved* rows have local state worth deleting
        // even though their containers are gone.
        (A::Purge, status) => match status {
            S::Active
            | S::Running
            | S::CleanExited
            | S::Crashed
            | S::PreservedDirty
            | S::PreservedUnpushed
            | S::RestoreAvailable
            | S::Superseded
            | S::FailedSetup => true,
            S::Purged => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_action_accepts_status_grid_smoke() {
        use WorkspaceInstanceAction as A;
        use WorkspaceInstanceStatus as S;

        assert!(instance_action_accepts_status(A::Stop, S::Running));
        assert!(!instance_action_accepts_status(A::Stop, S::CleanExited));
        assert!(!instance_action_accepts_status(A::Stop, S::Purged));
        assert!(instance_action_accepts_status(A::Purge, S::Running));
        assert!(instance_action_accepts_status(A::Purge, S::PreservedDirty));
        assert!(!instance_action_accepts_status(A::Purge, S::Purged));
        assert!(instance_action_accepts_status(A::Reconnect, S::Crashed));
        assert!(!instance_action_accepts_status(A::Reconnect, S::Purged));
    }
}
