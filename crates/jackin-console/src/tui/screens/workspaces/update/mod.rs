//! Workspace list screen update logic: handle keyboard events and produce
//! effects for launch, reconnect, stop, purge, and navigation actions.
//!
//! Not responsible for: rendering (see `view`) or state definitions (see
//! `model`).

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
pub struct PreviewFocusPlan {
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveConfirmPlan {
    Continue,
    ReturnToList,
    Commit,
}

#[derive(Debug, Clone)]
pub struct WorkspaceDeleteConfirmPlan {
    pub name: String,
    pub state: jackin_tui::components::ConfirmState,
}

#[derive(Debug, Clone)]
pub struct InstancePurgeConfirmPlan {
    pub container: String,
    pub label: String,
    pub state: jackin_tui::components::ConfirmState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTreeDisclosurePlan {
    None,
    CollapseWorkspace(usize),
    CollapseCurrentDir,
    ExpandWorkspace(usize),
    ExpandCurrentDir,
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
    crate::tui::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub fn selected_index(selected: usize, row_count: usize) -> usize {
    crate::tui::focus::selected_index(selected, row_count)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListSelectionPlan {
    pub selected: usize,
    pub changed: bool,
    pub clear_inline_role_picker: bool,
    pub clear_inline_agent_picker: bool,
    pub clear_inline_new_session_picker: bool,
    pub clear_inline_provider_picker: bool,
    pub clear_launch_provider_picker: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListScrollFocusPlan {
    pub list_names_focused: bool,
    pub scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
}

#[must_use]
pub const fn workspace_list_scroll_focus_plan(
    in_left_pane: bool,
    has_scroll_areas: bool,
    in_workspace_mounts: bool,
    in_global_mounts: bool,
    in_role_global_mounts: bool,
    in_roles: bool,
) -> WorkspaceListScrollFocusPlan {
    if in_left_pane {
        return WorkspaceListScrollFocusPlan {
            list_names_focused: true,
            scroll_focus: None,
        };
    }
    let scroll_focus = if !has_scroll_areas {
        None
    } else if in_workspace_mounts {
        Some(crate::tui::focus::MountScrollFocus::Workspace)
    } else if in_global_mounts {
        Some(crate::tui::focus::MountScrollFocus::Global)
    } else if in_role_global_mounts {
        Some(crate::tui::focus::MountScrollFocus::RoleGlobal)
    } else if in_roles {
        Some(crate::tui::focus::MountScrollFocus::Roles)
    } else {
        None
    };
    WorkspaceListScrollFocusPlan {
        list_names_focused: false,
        scroll_focus,
    }
}

#[must_use]
pub fn workspace_list_move_selection_plan(
    selected: usize,
    row_count: usize,
    delta: isize,
) -> WorkspaceListSelectionPlan {
    let next = crate::tui::focus::moved_selection(selected, row_count, delta);
    WorkspaceListSelectionPlan {
        selected: next,
        changed: next != selected,
        clear_inline_role_picker: true,
        clear_inline_agent_picker: true,
        clear_inline_new_session_picker: true,
        clear_inline_provider_picker: false,
        clear_launch_provider_picker: false,
    }
}

#[must_use]
pub fn workspace_list_select_row_plan(
    current_selected: usize,
    selected: usize,
    row_count: usize,
) -> WorkspaceListSelectionPlan {
    let next = crate::tui::focus::selected_index(selected, row_count);
    let changed = next != current_selected;
    WorkspaceListSelectionPlan {
        selected: next,
        changed,
        clear_inline_role_picker: true,
        clear_inline_agent_picker: changed,
        clear_inline_new_session_picker: changed,
        clear_inline_provider_picker: changed,
        clear_launch_provider_picker: changed,
    }
}

#[must_use]
pub fn workspace_delete_confirm_state(name: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!("Delete \"{name}\"?"))
}

#[must_use]
pub fn instance_purge_confirm_state(label: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!(
        "Purge \"{label}\"?\nThis removes the role container, DinD sidecar, volume, network, AND local recovery state. Cannot be undone."
    ))
}

#[must_use]
pub fn workspace_delete_confirm_plan(name: String) -> WorkspaceDeleteConfirmPlan {
    WorkspaceDeleteConfirmPlan {
        state: workspace_delete_confirm_state(&name),
        name,
    }
}

#[must_use]
pub fn instance_purge_confirm_plan(container: String, label: String) -> InstancePurgeConfirmPlan {
    InstancePurgeConfirmPlan {
        state: instance_purge_confirm_state(&label),
        container,
        label,
    }
}

#[must_use]
pub const fn collapse_selected_tree_plan(row: ManagerListRow) -> WorkspaceTreeDisclosurePlan {
    match row {
        ManagerListRow::SavedWorkspace(i) | ManagerListRow::WorkspaceInstance(i, _) => {
            WorkspaceTreeDisclosurePlan::CollapseWorkspace(i)
        }
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceTreeDisclosurePlan::CollapseCurrentDir
        }
        ManagerListRow::NewWorkspace => WorkspaceTreeDisclosurePlan::None,
    }
}

#[must_use]
pub const fn expand_selected_tree_plan(row: ManagerListRow) -> WorkspaceTreeDisclosurePlan {
    match row {
        ManagerListRow::SavedWorkspace(i) => WorkspaceTreeDisclosurePlan::ExpandWorkspace(i),
        ManagerListRow::CurrentDirectory => WorkspaceTreeDisclosurePlan::ExpandCurrentDir,
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceTreeDisclosurePlan::None,
    }
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

#[must_use]
pub const fn enter_preview_focus_plan() -> PreviewFocusPlan {
    PreviewFocusPlan { focused: true }
}

#[must_use]
pub const fn exit_preview_focus_plan() -> PreviewFocusPlan {
    PreviewFocusPlan { focused: false }
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
    Some(crate::tui::focus::moved_selection(
        cursor, pane_count, delta,
    ))
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
mod tests;
