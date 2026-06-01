use std::collections::BTreeSet;

use super::state::ManagerListRow;

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
