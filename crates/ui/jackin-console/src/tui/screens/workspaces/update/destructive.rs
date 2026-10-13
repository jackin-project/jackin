// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Destructive and disclosure plans.

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
