use std::collections::BTreeSet;

use crossterm::event::KeyCode;
use jackin_tui::ModalOutcome;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewPaneKeyPlan {
    Continue,
    ExitPreview,
    Move { delta: isize },
    ReconnectSelected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveConfirmPlan {
    Continue,
    ReturnToList,
    Commit,
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

#[must_use]
pub const fn workspace_unclamped_scroll_plan(current_scroll: u16, delta: i16) -> u16 {
    crate::tui::update::unclamped_scroll_plan(current_scroll, delta)
}

#[must_use]
pub const fn is_preview_pane_entry_target(key: KeyCode, row: ManagerListRow) -> bool {
    matches!(key, KeyCode::Tab | KeyCode::Right)
        && matches!(
            row,
            ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
        )
}

#[must_use]
pub const fn should_enter_preview_pane(
    key: KeyCode,
    row: ManagerListRow,
    pane_count: usize,
) -> bool {
    is_preview_pane_entry_target(key, row) && pane_count > 0
}

/// Preview-pane navigation mode: Esc / Left / BackTab exits, Up/Down
/// move inside the snapshot, and Enter reconnects to the selected pane.
#[must_use]
pub const fn preview_pane_key_plan(key: KeyCode, pane_count: usize) -> PreviewPaneKeyPlan {
    if pane_count == 0 {
        return PreviewPaneKeyPlan::ExitPreview;
    }
    match key {
        KeyCode::Esc | KeyCode::BackTab | KeyCode::Left => PreviewPaneKeyPlan::ExitPreview,
        KeyCode::Up | KeyCode::Char('k' | 'K') => PreviewPaneKeyPlan::Move { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => PreviewPaneKeyPlan::Move { delta: 1 },
        KeyCode::Enter => PreviewPaneKeyPlan::ReconnectSelected,
        _ => PreviewPaneKeyPlan::Continue,
    }
}

#[must_use]
pub fn preview_pane_cursor_plan(
    pane_count: usize,
    current_cursor: Option<usize>,
    delta: isize,
) -> Option<usize> {
    if pane_count == 0 {
        return None;
    }
    let cursor = current_cursor.unwrap_or(0).min(pane_count - 1);
    Some(crate::focus::moved_selection(cursor, pane_count, delta))
}

#[must_use]
pub const fn destructive_confirm_plan(outcome: ModalOutcome<bool>) -> DestructiveConfirmPlan {
    match outcome {
        ModalOutcome::Commit(true) => DestructiveConfirmPlan::Commit,
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => DestructiveConfirmPlan::ReturnToList,
        ModalOutcome::Continue => DestructiveConfirmPlan::Continue,
    }
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
    fn workspace_unclamped_scroll_plan_updates_offset() {
        assert_eq!(workspace_unclamped_scroll_plan(4, 3), 7);
        assert_eq!(workspace_unclamped_scroll_plan(4, -99), 0);
    }

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

    #[test]
    fn preview_pane_key_plan_routes_navigation() {
        assert_eq!(preview_pane_key_plan(KeyCode::Esc, 2), PreviewPaneKeyPlan::ExitPreview);
        assert_eq!(
            preview_pane_key_plan(KeyCode::Char('K'), 2),
            PreviewPaneKeyPlan::Move { delta: -1 }
        );
        assert_eq!(
            preview_pane_key_plan(KeyCode::Down, 2),
            PreviewPaneKeyPlan::Move { delta: 1 }
        );
        assert_eq!(
            preview_pane_key_plan(KeyCode::Enter, 2),
            PreviewPaneKeyPlan::ReconnectSelected
        );
        assert_eq!(preview_pane_key_plan(KeyCode::Tab, 2), PreviewPaneKeyPlan::Continue);
        assert_eq!(preview_pane_key_plan(KeyCode::Enter, 0), PreviewPaneKeyPlan::ExitPreview);
    }

    #[test]
    fn preview_pane_cursor_plan_clamps_current_and_delta() {
        assert_eq!(preview_pane_cursor_plan(0, Some(4), 1), None);
        assert_eq!(preview_pane_cursor_plan(3, None, 1), Some(1));
        assert_eq!(preview_pane_cursor_plan(3, Some(9), 1), Some(2));
        assert_eq!(preview_pane_cursor_plan(3, Some(0), -9), Some(0));
    }

    #[test]
    fn should_enter_preview_pane_requires_instance_row_key_and_panes() {
        assert!(should_enter_preview_pane(
            KeyCode::Tab,
            ManagerListRow::WorkspaceInstance(1, 0),
            2
        ));
        assert!(should_enter_preview_pane(
            KeyCode::Right,
            ManagerListRow::CurrentDirectoryInstance(0),
            1
        ));
        assert!(!should_enter_preview_pane(
            KeyCode::Tab,
            ManagerListRow::SavedWorkspace(1),
            2
        ));
        assert!(!should_enter_preview_pane(
            KeyCode::Down,
            ManagerListRow::WorkspaceInstance(1, 0),
            2
        ));
        assert!(!should_enter_preview_pane(
            KeyCode::Tab,
            ManagerListRow::WorkspaceInstance(1, 0),
            0
        ));
    }

    #[test]
    fn destructive_confirm_plan_routes_commit_cancel_and_continue() {
        assert_eq!(
            destructive_confirm_plan(ModalOutcome::Commit(true)),
            DestructiveConfirmPlan::Commit
        );
        assert_eq!(
            destructive_confirm_plan(ModalOutcome::Commit(false)),
            DestructiveConfirmPlan::ReturnToList
        );
        assert_eq!(
            destructive_confirm_plan(ModalOutcome::Cancel),
            DestructiveConfirmPlan::ReturnToList
        );
        assert_eq!(
            destructive_confirm_plan(ModalOutcome::Continue),
            DestructiveConfirmPlan::Continue
        );
    }
}
