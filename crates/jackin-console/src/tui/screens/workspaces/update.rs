// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace list screen update logic: handle keyboard events and produce
//! effects for launch, reconnect, stop, purge, and navigation actions.
//!
//! Not responsible for: rendering (see `view`) or state definitions (see
//! `model`).

use std::collections::BTreeSet;

use crossterm::event::{KeyCode, MouseEvent};
use jackin_tui::ModalOutcome;
use ratatui::layout::Rect;

use super::model::{ManagerHoverTarget, ManagerListRow};
use crate::mount_info_cache::MountInfoCache;
use crate::tui::components::error_popup::{
    no_instance_state_for_workspace_message, no_purgeable_instance_for_workspace_message,
    no_recoverable_instance_for_workspace_message, no_running_instance_for_workspace_message,
    no_running_instance_to_stop_message,
};
use crate::tui::components::github_picker::{GithubOpenPlan, github_open_plan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceInstanceAction {
    Reconnect,
    NewSession,
    Shell,
    Inspect,
    Stop,
    Purge,
}

#[must_use]
pub fn workspace_instance_empty_message(action: WorkspaceInstanceAction) -> &'static str {
    match action {
        WorkspaceInstanceAction::Reconnect => no_recoverable_instance_for_workspace_message(),
        WorkspaceInstanceAction::NewSession | WorkspaceInstanceAction::Shell => {
            no_running_instance_for_workspace_message()
        }
        WorkspaceInstanceAction::Inspect => no_instance_state_for_workspace_message(),
        WorkspaceInstanceAction::Stop => no_running_instance_to_stop_message(),
        WorkspaceInstanceAction::Purge => no_purgeable_instance_for_workspace_message(),
    }
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
pub enum PreviewPaneActionPlan {
    Continue,
    ExitPreview,
    Move { delta: isize },
    ReconnectSelected { session_id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewFocusPlan {
    pub focused: bool,
}

pub trait PreviewFocusState {
    fn set_preview_focused(&mut self, focused: bool);
}

pub fn apply_preview_focus_plan(state: &mut impl PreviewFocusState, plan: PreviewFocusPlan) {
    state.set_preview_focused(plan.focused);
}

pub trait PreviewPaneCursorState: PreviewFocusState {
    fn set_preview_pane_cursor(&mut self, container: &str, cursor: usize);
}

pub fn apply_preview_pane_cursor_plan(
    state: &mut impl PreviewPaneCursorState,
    container: &str,
    plan: Option<usize>,
) {
    let Some(cursor) = plan else {
        state.set_preview_focused(false);
        return;
    };
    state.set_preview_pane_cursor(container, cursor);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveConfirmPlan {
    Continue,
    ReturnToList,
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceDeleteKeyPlan {
    Continue,
    ReturnToList,
    RemoveWorkspace { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstancePurgeKeyPlan {
    Continue,
    ReturnToList,
    Purge { container: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedInstanceActionPlan {
    OpenError,
    Start { container: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedInstancePurgeConfirmPlan {
    OpenError,
    OpenConfirm { container: String, label: String },
}

#[derive(Debug, Clone)]
pub struct WorkspaceDeleteConfirmPlan {
    pub name: String,
    pub state: crate::tui::components::ConfirmState,
}

#[derive(Debug, Clone)]
pub struct InstancePurgeConfirmPlan {
    pub container: String,
    pub label: String,
    pub state: crate::tui::components::ConfirmState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTreeDisclosurePlan {
    None,
    CollapseWorkspace(usize),
    CollapseCurrentDir,
    ExpandWorkspace(usize),
    ExpandCurrentDir,
}

pub trait WorkspaceTreeDisclosureState {
    fn collapse_workspace(&mut self, index: usize);
    fn collapse_current_dir(&mut self);
    fn expand_workspace(&mut self, index: usize);
    fn expand_current_dir(&mut self);
}

pub fn apply_workspace_tree_disclosure_plan(
    state: &mut impl WorkspaceTreeDisclosureState,
    plan: WorkspaceTreeDisclosurePlan,
) {
    match plan {
        WorkspaceTreeDisclosurePlan::None => {}
        WorkspaceTreeDisclosurePlan::CollapseWorkspace(index) => state.collapse_workspace(index),
        WorkspaceTreeDisclosurePlan::CollapseCurrentDir => state.collapse_current_dir(),
        WorkspaceTreeDisclosurePlan::ExpandWorkspace(index) => state.expand_workspace(index),
        WorkspaceTreeDisclosurePlan::ExpandCurrentDir => state.expand_current_dir(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCollapseSelectionPlan {
    Parent,
    Clamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListHorizontalPlan {
    CollapseTree,
    ExpandTree,
    Scroll(i16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListEnterPlan {
    LaunchCurrentDir,
    CreateNewWorkspace,
    LaunchSavedWorkspace(usize),
    InstanceAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListKeyPlan {
    Exit,
    HorizontalTreeOrScroll { delta: i16 },
    ScrollHorizontal { delta: i16 },
    MoveSelection { delta: isize },
    ScrollFocusedVertical { delta: i16 },
    Enter,
    Edit,
    NewSession,
    Delete,
    OpenGithub,
    Prewarm,
    InstanceAction(WorkspaceInstanceAction),
    ConfirmPurge,
    Settings,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListTopLevelKeyPlan {
    PreviewFocused,
    EnterPreview,
    ListKey(WorkspaceListKeyPlan),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceInstanceScopePlan {
    CurrentDirectory,
    SavedWorkspace(usize),
    WorkspaceInstance(usize),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListSelectedInstancePlan {
    Direct {
        workspace_idx: Option<usize>,
        instance_idx: usize,
    },
    Scope,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceInstanceLookupEntry<'a> {
    pub container: &'a str,
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
    pub status: WorkspaceInstanceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceInstanceLookupScope<'a> {
    pub workspace_name: Option<&'a str>,
    pub workspace_label: &'a str,
    pub workdir: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListNewSessionPlan {
    ExistingWorkspaceInstance {
        workspace_idx: usize,
        instance_idx: usize,
    },
    CreateWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceListNewSessionOpenPlan {
    OpenPicker { container: String },
    OpenCreateWorkspace,
    OpenInstanceUnavailableError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListEditPlan {
    OpenEditor { workspace_idx: usize },
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListDeletePlan {
    ConfirmDelete { workspace_idx: usize },
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListSettingsPlan {
    OpenSettings,
    Noop,
}

#[must_use]
pub const fn workspace_list_prewarm_plan(row: ManagerListRow) -> Option<usize> {
    match row {
        ManagerListRow::SavedWorkspace(idx) => Some(idx),
        ManagerListRow::CurrentDirectory
        | ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::CurrentDirectoryInstance(_) => None,
    }
}

#[must_use]
pub const fn workspace_list_enter_plan(row: ManagerListRow) -> WorkspaceListEnterPlan {
    match row {
        ManagerListRow::CurrentDirectory => WorkspaceListEnterPlan::LaunchCurrentDir,
        ManagerListRow::NewWorkspace => WorkspaceListEnterPlan::CreateNewWorkspace,
        ManagerListRow::SavedWorkspace(idx) => WorkspaceListEnterPlan::LaunchSavedWorkspace(idx),
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceListEnterPlan::InstanceAction
        }
    }
}

#[must_use]
pub fn workspace_list_key_plan(key: KeyCode, list_scroll_focused: bool) -> WorkspaceListKeyPlan {
    use crate::tui::keymap::{WORKSPACE_LIST_KEYMAP, WorkspaceListAction as A};
    use termrock::keymap::KeyChord;

    let Some(action) =
        WORKSPACE_LIST_KEYMAP.dispatch(KeyChord::from(termrock::input::KeyCode::from(key)))
    else {
        return WorkspaceListKeyPlan::Continue;
    };
    match action {
        A::Exit => WorkspaceListKeyPlan::Exit,
        A::TreeLeft => WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta: -8 },
        A::TreeRight => WorkspaceListKeyPlan::HorizontalTreeOrScroll { delta: 8 },
        A::ScrollLeft => WorkspaceListKeyPlan::ScrollHorizontal { delta: -8 },
        A::ScrollRight => WorkspaceListKeyPlan::ScrollHorizontal { delta: 8 },
        A::NavigateUp => {
            if list_scroll_focused {
                WorkspaceListKeyPlan::ScrollFocusedVertical { delta: -3 }
            } else {
                WorkspaceListKeyPlan::MoveSelection { delta: -1 }
            }
        }
        A::NavigateDown => {
            if list_scroll_focused {
                WorkspaceListKeyPlan::ScrollFocusedVertical { delta: 3 }
            } else {
                WorkspaceListKeyPlan::MoveSelection { delta: 1 }
            }
        }
        A::Enter => WorkspaceListKeyPlan::Enter,
        A::Edit => WorkspaceListKeyPlan::Edit,
        A::NewSession => WorkspaceListKeyPlan::NewSession,
        A::Delete => WorkspaceListKeyPlan::Delete,
        A::OpenGithub => WorkspaceListKeyPlan::OpenGithub,
        A::InstanceReconnect => {
            WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Reconnect)
        }
        A::InstanceNewSession => {
            WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::NewSession)
        }
        A::InstanceShell => WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Shell),
        A::InstanceInspect => {
            WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Inspect)
        }
        A::InstanceStop => WorkspaceListKeyPlan::InstanceAction(WorkspaceInstanceAction::Stop),
        A::ConfirmPurge => WorkspaceListKeyPlan::ConfirmPurge,
        A::Settings => WorkspaceListKeyPlan::Settings,
        A::Prewarm => WorkspaceListKeyPlan::Prewarm,
        // Tab/preview entry is resolved upstream in `workspace_list_top_level_key_plan`;
        // Ctrl-Q never reaches here (intercepted by `should_open_quit_confirm`).
        A::EnterPreview | A::Quit => WorkspaceListKeyPlan::Continue,
    }
}

#[must_use]
pub const fn selected_instance_scope_plan(row: ManagerListRow) -> WorkspaceInstanceScopePlan {
    match row {
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceInstanceScopePlan::CurrentDirectory
        }
        ManagerListRow::SavedWorkspace(idx) => WorkspaceInstanceScopePlan::SavedWorkspace(idx),
        ManagerListRow::WorkspaceInstance(ws_idx, _) => {
            WorkspaceInstanceScopePlan::WorkspaceInstance(ws_idx)
        }
        ManagerListRow::NewWorkspace => WorkspaceInstanceScopePlan::None,
    }
}

#[must_use]
pub const fn selected_instance_plan(row: ManagerListRow) -> WorkspaceListSelectedInstancePlan {
    match row {
        ManagerListRow::CurrentDirectoryInstance(instance_idx) => {
            WorkspaceListSelectedInstancePlan::Direct {
                workspace_idx: None,
                instance_idx,
            }
        }
        ManagerListRow::WorkspaceInstance(workspace_idx, instance_idx) => {
            WorkspaceListSelectedInstancePlan::Direct {
                workspace_idx: Some(workspace_idx),
                instance_idx,
            }
        }
        ManagerListRow::CurrentDirectory | ManagerListRow::SavedWorkspace(_) => {
            WorkspaceListSelectedInstancePlan::Scope
        }
        ManagerListRow::NewWorkspace => WorkspaceListSelectedInstancePlan::None,
    }
}

#[must_use]
pub fn selected_instance_container_for_action<'a>(
    row: ManagerListRow,
    action: WorkspaceInstanceAction,
    mut direct_instance: impl FnMut(Option<usize>, usize) -> Option<WorkspaceInstanceLookupEntry<'a>>,
    mut scope: impl FnMut(WorkspaceInstanceScopePlan) -> Option<WorkspaceInstanceLookupScope<'a>>,
    instances: impl IntoIterator<Item = WorkspaceInstanceLookupEntry<'a>>,
) -> Option<&'a str> {
    match selected_instance_plan(row) {
        WorkspaceListSelectedInstancePlan::Direct {
            workspace_idx,
            instance_idx,
        } => {
            let entry = direct_instance(workspace_idx, instance_idx)?;
            instance_action_accepts_status(action, entry.status).then_some(entry.container)
        }
        WorkspaceListSelectedInstancePlan::Scope => {
            let scope = scope(selected_instance_scope_plan(row))?;
            instances.into_iter().find_map(|entry| {
                (instance_lookup_entry_matches_scope(entry, scope)
                    && instance_action_accepts_status(action, entry.status))
                .then_some(entry.container)
            })
        }
        WorkspaceListSelectedInstancePlan::None => None,
    }
}

#[must_use]
pub fn instance_lookup_entry_matches_scope(
    entry: WorkspaceInstanceLookupEntry<'_>,
    scope: WorkspaceInstanceLookupScope<'_>,
) -> bool {
    entry.workspace_name == scope.workspace_name
        && entry.workspace_label == scope.workspace_label
        && entry.workdir == scope.workdir
}

#[must_use]
pub const fn workspace_list_new_session_plan(row: ManagerListRow) -> WorkspaceListNewSessionPlan {
    match row {
        ManagerListRow::WorkspaceInstance(workspace_idx, instance_idx) => {
            WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
                workspace_idx,
                instance_idx,
            }
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace => WorkspaceListNewSessionPlan::CreateWorkspace,
    }
}

#[must_use]
pub fn workspace_list_new_session_open_plan(
    plan: WorkspaceListNewSessionPlan,
    workspace_instance_container: impl FnOnce(usize, usize) -> Option<String>,
) -> WorkspaceListNewSessionOpenPlan {
    match plan {
        WorkspaceListNewSessionPlan::ExistingWorkspaceInstance {
            workspace_idx,
            instance_idx,
        } => workspace_instance_container(workspace_idx, instance_idx).map_or(
            WorkspaceListNewSessionOpenPlan::OpenInstanceUnavailableError,
            |container| WorkspaceListNewSessionOpenPlan::OpenPicker { container },
        ),
        WorkspaceListNewSessionPlan::CreateWorkspace => {
            WorkspaceListNewSessionOpenPlan::OpenCreateWorkspace
        }
    }
}

#[must_use]
pub const fn workspace_list_edit_plan(row: ManagerListRow) -> WorkspaceListEditPlan {
    match row {
        ManagerListRow::SavedWorkspace(workspace_idx) => {
            WorkspaceListEditPlan::OpenEditor { workspace_idx }
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceListEditPlan::Noop,
    }
}

#[must_use]
pub const fn workspace_list_delete_plan(row: ManagerListRow) -> WorkspaceListDeletePlan {
    match row {
        ManagerListRow::SavedWorkspace(workspace_idx) => {
            WorkspaceListDeletePlan::ConfirmDelete { workspace_idx }
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceListDeletePlan::Noop,
    }
}

#[must_use]
pub const fn workspace_list_settings_plan(row: ManagerListRow) -> WorkspaceListSettingsPlan {
    match row {
        ManagerListRow::CurrentDirectory
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace => WorkspaceListSettingsPlan::OpenSettings,
        ManagerListRow::CurrentDirectoryInstance(_) | ManagerListRow::WorkspaceInstance(_, _) => {
            WorkspaceListSettingsPlan::Noop
        }
    }
}

#[must_use]
pub fn workspace_list_github_open_plan(
    selected_workspace_name: Option<&str>,
    config: &jackin_config::AppConfig,
    mount_info_cache: &MountInfoCache,
) -> GithubOpenPlan {
    let Some(name) = selected_workspace_name else {
        return GithubOpenPlan::Continue;
    };
    let Some(workspace) = config.workspaces.get(name) else {
        return GithubOpenPlan::Continue;
    };
    github_open_plan(crate::github_mounts::resolve_for_workspace_from_cache(
        workspace,
        mount_info_cache,
    ))
}

#[must_use]
pub const fn workspace_list_current_directory_selected(row: ManagerListRow) -> bool {
    matches!(row, ManagerListRow::CurrentDirectory)
}

#[must_use]
pub const fn workspace_list_new_workspace_selected(row: ManagerListRow) -> bool {
    matches!(row, ManagerListRow::NewWorkspace)
}

#[must_use]
pub const fn initial_workspace_selected_index(
    saved_count: usize,
    matching_saved_index: Option<usize>,
) -> usize {
    let selected_row = match matching_saved_index {
        Some(idx) => ManagerListRow::SavedWorkspace(idx),
        None => ManagerListRow::CurrentDirectory,
    };
    match selected_row.to_screen_index(saved_count) {
        Some(idx) => idx,
        None => 0,
    }
}

#[must_use]
pub const fn saved_workspace_selected_index(saved_count: usize, saved_index: usize) -> usize {
    match ManagerListRow::SavedWorkspace(saved_index).to_screen_index(saved_count) {
        Some(idx) => idx,
        None => 0,
    }
}

#[must_use]
pub const fn collapse_current_dir_selection_plan(
    row: ManagerListRow,
) -> WorkspaceCollapseSelectionPlan {
    match row {
        ManagerListRow::CurrentDirectoryInstance(_) => WorkspaceCollapseSelectionPlan::Parent,
        ManagerListRow::CurrentDirectory
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceCollapseSelectionPlan::Clamp,
    }
}

#[must_use]
pub const fn collapsed_current_dir_selected_index(selected_row: ManagerListRow) -> Option<usize> {
    match collapse_current_dir_selection_plan(selected_row) {
        WorkspaceCollapseSelectionPlan::Parent => Some(0),
        WorkspaceCollapseSelectionPlan::Clamp => None,
    }
}

#[must_use]
pub const fn collapse_workspace_selection_plan(
    row: ManagerListRow,
    workspace_idx: usize,
) -> WorkspaceCollapseSelectionPlan {
    match row {
        ManagerListRow::WorkspaceInstance(row_workspace_idx, _)
            if row_workspace_idx == workspace_idx =>
        {
            WorkspaceCollapseSelectionPlan::Parent
        }
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => WorkspaceCollapseSelectionPlan::Clamp,
    }
}

#[must_use]
pub fn collapsed_workspace_selected_index(
    rows: &[ManagerListRow],
    selected: usize,
    selected_row: ManagerListRow,
    workspace_idx: usize,
) -> Option<usize> {
    match collapse_workspace_selection_plan(selected_row, workspace_idx) {
        WorkspaceCollapseSelectionPlan::Parent => {
            workspace_row_index(rows, ManagerListRow::SavedWorkspace(workspace_idx))
        }
        WorkspaceCollapseSelectionPlan::Clamp => {
            Some(selected.min(workspace_last_selectable_index(rows.len())))
        }
    }
}

#[must_use]
pub const fn workspace_list_saved_workspace_index(row: ManagerListRow) -> Option<usize> {
    match row {
        ManagerListRow::SavedWorkspace(idx) => Some(idx),
        ManagerListRow::CurrentDirectory
        | ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::NewWorkspace
        | ManagerListRow::WorkspaceInstance(_, _) => None,
    }
}

#[must_use]
pub const fn workspace_list_settings_available(row: ManagerListRow) -> bool {
    !matches!(
        row,
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
    )
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
pub fn workspace_visual_selected_index(
    visual_rows: &[Option<ManagerListRow>],
    selected: ManagerListRow,
) -> Option<usize> {
    visual_rows
        .iter()
        .position(|row| row.as_ref() == Some(&selected))
}

#[must_use]
pub fn workspace_row_index(rows: &[ManagerListRow], row: ManagerListRow) -> Option<usize> {
    rows.iter().position(|candidate| *candidate == row)
}

#[must_use]
pub fn workspace_row_at(rows: &[ManagerListRow], idx: usize) -> Option<ManagerListRow> {
    rows.get(idx).copied()
}

#[must_use]
pub fn workspace_selected_row(rows: &[ManagerListRow], selected: usize) -> ManagerListRow {
    workspace_row_at(rows, selected).unwrap_or(ManagerListRow::CurrentDirectory)
}

#[must_use]
pub fn workspace_row_at_visual_index(
    visual_rows: &[Option<ManagerListRow>],
    idx: usize,
) -> Option<ManagerListRow> {
    visual_rows.get(idx).copied().flatten()
}

#[must_use]
pub const fn workspace_last_selectable_index(row_count: usize) -> usize {
    row_count.saturating_sub(1)
}

#[must_use]
pub fn workspace_list_hover_row_at_position(
    visual_rows: &[Option<ManagerListRow>],
    col: u16,
    row: u16,
    term_size: Rect,
    seam_x: u16,
    mut selectable: impl FnMut(ManagerListRow) -> bool,
) -> Option<ManagerListRow> {
    if crate::tui::layout::near_seam(col, seam_x) {
        return None;
    }
    let content_top = crate::tui::layout::LIST_HEADER_HEIGHT.saturating_add(1);
    let body_end = term_size
        .height
        .saturating_sub(crate::tui::layout::LIST_FOOTER_HEIGHT);
    let content_bottom = body_end.saturating_sub(1);
    if content_top >= content_bottom {
        return None;
    }

    let mut regions = Vec::new();
    for (visual_idx, row_value) in visual_rows.iter().enumerate() {
        let Some(row_value) = row_value else {
            continue;
        };
        if !selectable(*row_value) {
            continue;
        }
        let Ok(offset) = u16::try_from(visual_idx) else {
            break;
        };
        let y = content_top.saturating_add(offset);
        if y >= content_bottom {
            break;
        }
        regions.push(termrock::interaction::HitRegion {
            area: Rect {
                x: 1,
                y,
                width: seam_x.saturating_sub(1),
                height: 1,
            },
            id: *row_value,
        });
    }
    let position = ratatui::layout::Position::new(col, row);
    regions
        .iter()
        .find(|region| region.area.contains(position))
        .map(|region| region.id)
}

#[must_use]
pub fn moved_selection(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::tui::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub fn selected_index(selected: usize, row_count: usize) -> usize {
    crate::tui::focus::selected_index(selected, row_count)
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal inline-picker clear flags on the list-selection \
              plan (role / agent / new_session / provider / launch_provider) — \
              each tracks an independent clear-mutation the plan applies to the \
              state. Named-field reads match the per-trait-method dispatch this \
              plan parallelizes."
)]
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

pub trait WorkspaceListSelectionState {
    fn clear_inline_role_picker(&mut self);
    fn clear_inline_agent_picker(&mut self);
    fn clear_inline_new_session_picker(&mut self);
    fn clear_inline_provider_picker(&mut self);
    fn clear_launch_provider_picker(&mut self);
    fn reset_list_scroll(&mut self);
    fn set_selected(&mut self, selected: usize);
}

pub fn apply_workspace_list_selection_plan(
    state: &mut impl WorkspaceListSelectionState,
    plan: WorkspaceListSelectionPlan,
) {
    if plan.clear_inline_role_picker {
        state.clear_inline_role_picker();
    }
    if plan.clear_inline_agent_picker {
        state.clear_inline_agent_picker();
    }
    if plan.clear_inline_new_session_picker {
        state.clear_inline_new_session_picker();
    }
    if plan.clear_inline_provider_picker {
        state.clear_inline_provider_picker();
    }
    if plan.clear_launch_provider_picker {
        state.clear_launch_provider_picker();
    }
    if plan.changed {
        state.reset_list_scroll();
        state.set_selected(plan.selected);
    }
}

pub trait WorkspaceListHoverState {
    fn set_workspace_list_hover_target(&mut self, target: Option<ManagerHoverTarget>);
}

pub fn apply_workspace_list_hover_target(
    state: &mut impl WorkspaceListHoverState,
    target: Option<ManagerHoverTarget>,
) {
    state.set_workspace_list_hover_target(target);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListScrollFocusPlan {
    pub list_names_focused: bool,
    pub scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListScrollTargetPlan {
    ListNames,
    FocusedBlock(crate::tui::focus::MountScrollFocus),
    None,
}

pub trait WorkspaceListScrollState {
    fn list_names_scroll_x(&self) -> u16;
    fn set_list_names_scroll_x(&mut self, value: u16);
    fn block_scroll_x(&self, focus: crate::tui::focus::MountScrollFocus) -> u16;
    fn set_block_scroll_x(&mut self, focus: crate::tui::focus::MountScrollFocus, value: u16);
    fn block_scroll_y(&self, focus: crate::tui::focus::MountScrollFocus) -> u16;
    fn set_block_scroll_y(&mut self, focus: crate::tui::focus::MountScrollFocus, value: u16);
}

pub fn apply_workspace_list_horizontal_scroll_plan(
    state: &mut impl WorkspaceListScrollState,
    plan: WorkspaceListScrollTargetPlan,
    delta: i16,
) {
    match plan {
        WorkspaceListScrollTargetPlan::ListNames => {
            state.set_list_names_scroll_x(workspace_unclamped_scroll_plan(
                state.list_names_scroll_x(),
                delta,
            ));
        }
        WorkspaceListScrollTargetPlan::FocusedBlock(focus) => {
            state.set_block_scroll_x(
                focus,
                workspace_unclamped_scroll_plan(state.block_scroll_x(focus), delta),
            );
        }
        WorkspaceListScrollTargetPlan::None => {}
    }
}

pub fn apply_workspace_list_vertical_scroll_plan(
    state: &mut impl WorkspaceListScrollState,
    plan: WorkspaceListScrollTargetPlan,
    delta: i16,
) {
    if let WorkspaceListScrollTargetPlan::FocusedBlock(focus) = plan {
        state.set_block_scroll_y(
            focus,
            workspace_unclamped_scroll_plan(state.block_scroll_y(focus), delta),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListMousePlan {
    StartDrag(crate::tui::split::DragState),
    UpdateSplit(u16),
    EndDrag,
    SelectRow(ManagerListRow),
    Continue,
}

#[must_use]
pub fn workspace_list_mouse_plan(
    mouse: MouseEvent,
    term_size: Rect,
    split_pct: u16,
    drag_state: Option<crate::tui::split::DragState>,
    list_modal_open: bool,
    visual_rows: &[Option<ManagerListRow>],
    selectable: impl FnMut(ManagerListRow) -> bool,
) -> WorkspaceListMousePlan {
    if list_modal_open {
        return WorkspaceListMousePlan::Continue;
    }
    match mouse.kind {
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            let seam_x = crate::tui::layout::split_seam_column(split_pct, term_size.width);
            if crate::tui::layout::near_seam(mouse.column, seam_x) {
                return WorkspaceListMousePlan::StartDrag(crate::tui::split::DragState {
                    anchor_pct: split_pct,
                    anchor_x: mouse.column,
                });
            }
            workspace_list_hover_row_at_position(
                visual_rows,
                mouse.column,
                mouse.row,
                term_size,
                seam_x,
                selectable,
            )
            .map_or(WorkspaceListMousePlan::Continue, |row| {
                WorkspaceListMousePlan::SelectRow(row)
            })
        }
        crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => drag_state
            .map_or(WorkspaceListMousePlan::Continue, |anchor| {
                WorkspaceListMousePlan::UpdateSplit(crate::tui::split::clamp_split(
                    crate::tui::layout::split_pct_from_drag(
                        anchor.anchor_pct,
                        anchor.anchor_x,
                        mouse.column,
                        term_size.width,
                    ),
                ))
            }),
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
            WorkspaceListMousePlan::EndDrag
        }
        _ => WorkspaceListMousePlan::Continue,
    }
}

#[must_use]
pub fn workspace_list_clickable_at_position(
    column: u16,
    row: u16,
    term_size: Rect,
    split_pct: u16,
    list_modal_open: bool,
    visual_rows: &[Option<ManagerListRow>],
    selectable: impl FnMut(ManagerListRow) -> bool,
) -> bool {
    if list_modal_open {
        return false;
    }
    let seam_x = crate::tui::layout::split_seam_column(split_pct, term_size.width);
    if crate::tui::layout::near_seam(column, seam_x) {
        return false;
    }
    workspace_list_hover_row_at_position(visual_rows, column, row, term_size, seam_x, selectable)
        .is_some()
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Six orthogonal workspace-list scroll-focus inputs (in_left_pane, \
              has_scroll_areas, ...) — each is an independent UI signal the \
              scroll-focus planner reads to pick the correct focus target. \
              Named-arg reads match the per-input scroll-focus routing idiom."
)]
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
pub const fn workspace_list_horizontal_scroll_target_plan(
    list_names_focused: bool,
    scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
) -> WorkspaceListScrollTargetPlan {
    if list_names_focused {
        WorkspaceListScrollTargetPlan::ListNames
    } else if let Some(focus) = scroll_focus {
        WorkspaceListScrollTargetPlan::FocusedBlock(focus)
    } else {
        WorkspaceListScrollTargetPlan::None
    }
}

#[must_use]
pub const fn workspace_list_vertical_scroll_target_plan(
    scroll_focus: Option<crate::tui::focus::MountScrollFocus>,
) -> WorkspaceListScrollTargetPlan {
    if let Some(focus) = scroll_focus {
        WorkspaceListScrollTargetPlan::FocusedBlock(focus)
    } else {
        WorkspaceListScrollTargetPlan::None
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
pub fn workspace_delete_confirm_state(name: &str) -> crate::tui::components::ConfirmState {
    crate::tui::components::ConfirmState::new(format!("Delete \"{name}\"?"))
}

#[must_use]
pub fn instance_purge_confirm_state(label: &str) -> crate::tui::components::ConfirmState {
    crate::tui::components::ConfirmState::new(format!(
        "Purge \"{label}\"?\nRemoves the role container, DinD sidecar, volume, network, and local recovery state."
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
pub fn workspace_row_owns_left(
    row: ManagerListRow,
    current_dir_expanded: bool,
    current_dir_has_instances: bool,
    mut workspace_expanded: impl FnMut(usize) -> bool,
) -> bool {
    match row {
        ManagerListRow::CurrentDirectory => current_dir_expanded && current_dir_has_instances,
        ManagerListRow::CurrentDirectoryInstance(_) => current_dir_expanded,
        ManagerListRow::SavedWorkspace(i) | ManagerListRow::WorkspaceInstance(i, _) => {
            workspace_expanded(i)
        }
        ManagerListRow::NewWorkspace => false,
    }
}

#[must_use]
pub fn workspace_row_owns_right(
    row: ManagerListRow,
    current_dir_expanded: bool,
    current_dir_has_instances: bool,
    mut workspace_expanded: impl FnMut(usize) -> bool,
    mut workspace_has_instances: impl FnMut(usize) -> bool,
) -> bool {
    match row {
        ManagerListRow::CurrentDirectory => !current_dir_expanded && current_dir_has_instances,
        ManagerListRow::SavedWorkspace(i) => !workspace_expanded(i) && workspace_has_instances(i),
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => false,
    }
}

#[must_use]
pub fn workspace_list_horizontal_plan(
    row: ManagerListRow,
    horizontal_delta: i16,
    current_dir_expanded: bool,
    current_dir_has_instances: bool,
    workspace_expanded: impl FnMut(usize) -> bool,
    workspace_has_instances: impl FnMut(usize) -> bool,
) -> WorkspaceListHorizontalPlan {
    if horizontal_delta < 0 {
        if workspace_row_owns_left(
            row,
            current_dir_expanded,
            current_dir_has_instances,
            workspace_expanded,
        ) {
            WorkspaceListHorizontalPlan::CollapseTree
        } else {
            WorkspaceListHorizontalPlan::Scroll(horizontal_delta)
        }
    } else if horizontal_delta > 0
        && workspace_row_owns_right(
            row,
            current_dir_expanded,
            current_dir_has_instances,
            workspace_expanded,
            workspace_has_instances,
        )
    {
        WorkspaceListHorizontalPlan::ExpandTree
    } else {
        WorkspaceListHorizontalPlan::Scroll(horizontal_delta)
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
pub fn workspace_list_top_level_key_plan(
    key: KeyCode,
    preview_focused: bool,
    selected_row: ManagerListRow,
    selected_preview_pane_count: Option<usize>,
    list_scroll_focus_active: bool,
) -> WorkspaceListTopLevelKeyPlan {
    if preview_focused {
        return WorkspaceListTopLevelKeyPlan::PreviewFocused;
    }
    if let Some(pane_count) = selected_preview_pane_count
        && should_enter_preview_pane(key, selected_row, pane_count)
    {
        return WorkspaceListTopLevelKeyPlan::EnterPreview;
    }
    WorkspaceListTopLevelKeyPlan::ListKey(workspace_list_key_plan(key, list_scroll_focus_active))
}

#[must_use]
pub const fn enter_preview_focus_plan() -> PreviewFocusPlan {
    PreviewFocusPlan { focused: true }
}

#[must_use]
pub const fn exit_preview_focus_plan() -> PreviewFocusPlan {
    PreviewFocusPlan { focused: false }
}

/// Preview-pane navigation mode: Esc / Left / `BackTab` exits, Up/Down
/// move inside the snapshot, and Enter reconnects to the selected pane.
#[must_use]
pub fn preview_pane_key_plan(key: KeyCode, pane_count: usize) -> PreviewPaneKeyPlan {
    use crate::tui::keymap::{PREVIEW_PANE_KEYMAP, PreviewPaneAction as A};
    use termrock::keymap::KeyChord;

    if pane_count == 0 {
        return PreviewPaneKeyPlan::ExitPreview;
    }
    match PREVIEW_PANE_KEYMAP.dispatch(KeyChord::from(termrock::input::KeyCode::from(key))) {
        Some(A::Back) => PreviewPaneKeyPlan::ExitPreview,
        Some(A::NavigateUp) => PreviewPaneKeyPlan::Move { delta: -1 },
        Some(A::NavigateDown) => PreviewPaneKeyPlan::Move { delta: 1 },
        Some(A::Attach) => PreviewPaneKeyPlan::ReconnectSelected,
        None => PreviewPaneKeyPlan::Continue,
    }
}

#[must_use]
pub fn preview_pane_selected_index(
    pane_count: usize,
    current_cursor: Option<usize>,
) -> Option<usize> {
    if pane_count == 0 {
        return None;
    }
    Some(current_cursor.unwrap_or(0).min(pane_count - 1))
}

#[must_use]
pub fn preview_pane_cursor_plan(
    pane_count: usize,
    current_cursor: Option<usize>,
    delta: isize,
) -> Option<usize> {
    let cursor = preview_pane_selected_index(pane_count, current_cursor)?;
    Some(crate::tui::focus::moved_selection(
        cursor, pane_count, delta,
    ))
}

#[must_use]
pub fn preview_pane_action_plan(
    key: KeyCode,
    current_cursor: Option<usize>,
    session_ids: impl IntoIterator<Item = u64>,
) -> PreviewPaneActionPlan {
    let session_ids: Vec<u64> = session_ids.into_iter().collect();
    match preview_pane_key_plan(key, session_ids.len()) {
        PreviewPaneKeyPlan::ExitPreview => PreviewPaneActionPlan::ExitPreview,
        PreviewPaneKeyPlan::Move { delta } => PreviewPaneActionPlan::Move { delta },
        PreviewPaneKeyPlan::ReconnectSelected => {
            let Some(cursor) = preview_pane_selected_index(session_ids.len(), current_cursor)
            else {
                return PreviewPaneActionPlan::Continue;
            };
            session_ids
                .get(cursor)
                .copied()
                .map_or(PreviewPaneActionPlan::Continue, |session_id| {
                    PreviewPaneActionPlan::ReconnectSelected { session_id }
                })
        }
        PreviewPaneKeyPlan::Continue => PreviewPaneActionPlan::Continue,
    }
}

#[must_use]
pub const fn destructive_confirm_plan(outcome: ModalOutcome<bool>) -> DestructiveConfirmPlan {
    match outcome {
        ModalOutcome::Commit(true) => DestructiveConfirmPlan::Commit,
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => DestructiveConfirmPlan::ReturnToList,
        ModalOutcome::Continue => DestructiveConfirmPlan::Continue,
    }
}

#[must_use]
pub fn workspace_delete_key_plan(
    outcome: ModalOutcome<bool>,
    name: String,
) -> WorkspaceDeleteKeyPlan {
    match destructive_confirm_plan(outcome) {
        DestructiveConfirmPlan::Commit => WorkspaceDeleteKeyPlan::RemoveWorkspace { name },
        DestructiveConfirmPlan::ReturnToList => WorkspaceDeleteKeyPlan::ReturnToList,
        DestructiveConfirmPlan::Continue => WorkspaceDeleteKeyPlan::Continue,
    }
}

#[must_use]
pub fn instance_purge_key_plan(
    outcome: ModalOutcome<bool>,
    container: String,
) -> InstancePurgeKeyPlan {
    match destructive_confirm_plan(outcome) {
        DestructiveConfirmPlan::Commit => InstancePurgeKeyPlan::Purge { container },
        DestructiveConfirmPlan::ReturnToList => InstancePurgeKeyPlan::ReturnToList,
        DestructiveConfirmPlan::Continue => InstancePurgeKeyPlan::Continue,
    }
}

#[must_use]
pub fn selected_instance_action_plan(container: Option<String>) -> SelectedInstanceActionPlan {
    match container {
        Some(container) => SelectedInstanceActionPlan::Start { container },
        None => SelectedInstanceActionPlan::OpenError,
    }
}

#[must_use]
pub fn selected_instance_purge_confirm_plan(
    container: Option<String>,
    label_for_container: impl FnOnce(&str) -> String,
) -> SelectedInstancePurgeConfirmPlan {
    let Some(container) = container else {
        return SelectedInstancePurgeConfirmPlan::OpenError;
    };
    let label = label_for_container(&container);
    SelectedInstancePurgeConfirmPlan::OpenConfirm { container, label }
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
