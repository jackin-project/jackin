// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use super::WorkspaceSelection;
use crate::tui::screens::workspaces::model::ManagerListRow;
use crate::tui::screens::workspaces::update::WorkspaceRowLayout;

/// Mixed expansion fixture: current directory expanded with two
/// instances, two saved workspaces (first expanded with one instance),
/// and the `NewWorkspace` tail row.
fn mixed_projection() -> Vec<ManagerListRow> {
    WorkspaceSelection::projection(WorkspaceRowLayout {
        current_dir_expanded: true,
        current_dir_instance_count: 2,
        workspace_instance_counts: &[1, 3],
        expanded_workspaces: &BTreeSet::from([0]),
    })
}

#[test]
fn wrapper_projection_round_trips_every_row_variant() {
    let rows = mixed_projection();
    assert_eq!(
        rows,
        vec![
            ManagerListRow::CurrentDirectory,
            ManagerListRow::CurrentDirectoryInstance(0),
            ManagerListRow::CurrentDirectoryInstance(1),
            ManagerListRow::SavedWorkspace(0),
            ManagerListRow::WorkspaceInstance(0, 0),
            ManagerListRow::SavedWorkspace(1),
            ManagerListRow::NewWorkspace,
        ]
    );
    for (index, row) in rows.iter().copied().enumerate() {
        assert_eq!(WorkspaceSelection::index_of(&rows, row), Some(index));
        assert_eq!(WorkspaceSelection::row_at(&rows, index), Some(row));
    }
    assert_eq!(WorkspaceSelection::row_at(&rows, rows.len()), None);
}

#[test]
fn wrapper_projection_round_trips_collapsed_workspaces() {
    let rows = WorkspaceSelection::projection(WorkspaceRowLayout {
        current_dir_expanded: false,
        current_dir_instance_count: 2,
        workspace_instance_counts: &[1, 3],
        expanded_workspaces: &BTreeSet::new(),
    });
    assert_eq!(
        rows,
        vec![
            ManagerListRow::CurrentDirectory,
            ManagerListRow::SavedWorkspace(0),
            ManagerListRow::SavedWorkspace(1),
            ManagerListRow::NewWorkspace,
        ]
    );
    for (index, row) in rows.iter().copied().enumerate() {
        assert_eq!(WorkspaceSelection::index_of(&rows, row), Some(index));
        assert_eq!(WorkspaceSelection::row_at(&rows, index), Some(row));
    }
}

/// Wrap-parity gate, corrected against observed pre-cutover behavior: the
/// retired helper SATURATED at both ends (planning text claimed wrap —
/// falsified by the code and by
/// `update::tests::selection_move_plan_clamps_to_rows`), so parity means
/// saturating here.
#[test]
fn wrapper_move_saturates_at_both_ends_like_pre_cutover() {
    let rows = mixed_projection();
    let last = rows.len() - 1;
    assert_eq!(WorkspaceSelection::move_index(0, rows.len(), -1), 0);
    assert_eq!(WorkspaceSelection::move_index(last, rows.len(), 1), last);
    assert_eq!(WorkspaceSelection::move_index(0, rows.len(), -99), 0);
    assert_eq!(WorkspaceSelection::move_index(last, rows.len(), 99), last);
    assert_eq!(WorkspaceSelection::move_index(2, rows.len(), 1), 3);
    assert_eq!(WorkspaceSelection::move_index(2, rows.len(), -1), 1);
}

#[test]
fn wrapper_move_to_clamps_into_projection() {
    let rows = mixed_projection();
    assert_eq!(WorkspaceSelection::move_to(2, rows.len()), 2);
    assert_eq!(WorkspaceSelection::move_to(99, rows.len()), rows.len() - 1);
}
