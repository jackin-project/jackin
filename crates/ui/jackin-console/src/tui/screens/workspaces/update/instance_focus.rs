// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::error_popup::{
    no_instance_state_for_workspace_message, no_purgeable_instance_for_workspace_message,
    no_recoverable_instance_for_workspace_message, no_running_instance_for_workspace_message,
    no_running_instance_to_stop_message,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListFocusOwner {
    ListNames,

    Preview,
}

#[must_use]
pub const fn workspace_list_focus_order() -> [WorkspaceListFocusOwner; 2] {
    [
        WorkspaceListFocusOwner::ListNames,
        WorkspaceListFocusOwner::Preview,
    ]
}

#[must_use]
pub const fn workspace_list_focus_head() -> WorkspaceListFocusOwner {
    let [head, ..] = workspace_list_focus_order();
    head
}

#[must_use]
pub const fn workspace_list_focus_next(owner: WorkspaceListFocusOwner) -> WorkspaceListFocusOwner {
    let [head, second] = workspace_list_focus_order();
    match owner {
        WorkspaceListFocusOwner::ListNames => second,
        WorkspaceListFocusOwner::Preview => head,
    }
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
