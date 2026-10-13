// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace list key plans.

use super::{WorkspaceInstanceAction, WorkspaceInstanceStatus};

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
