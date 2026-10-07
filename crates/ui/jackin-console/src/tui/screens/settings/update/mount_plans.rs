// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Global mount plan types.

use super::super::model::{GlobalMountDraft, GlobalMountTextTarget};

use jackin_core::RoleSelector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountTextCommitPlan {
    AddScope(Option<String>),
    AddName(String),
    AddSource(String),
    AddDestination(String),
    SetSource(String),
    SetDestination(String),
    SetScope(Option<String>),
    Rename(String),
    EmptyName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountEditTextApplyPlan {
    MissingRow,
    EmptyName,
    Applied,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RolePickerOpenPlan {
    NoRoles,
    Open(Vec<RoleSelector>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountGithubOpenPlan {
    NoSelection,
    NoGithubUrl,
    Open(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountAddFinalizePlan {
    EmptyDestination(GlobalMountDraft),
    Add {
        row: jackin_config::GlobalMountRow,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountAddFinalizeApplyPlan {
    MissingDraft,
    EmptyDestination,
    Add {
        row: jackin_config::GlobalMountRow,
        selected: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountRolePickerCommitPlan {
    MissingDraft,
    OpenFileBrowser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountAddTextApplyPlan {
    MissingDraft,
    OpenFileBrowser,
    OpenAddSource,
    OpenAddDestination,
    Finalize,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountScopePickerCommitPlan {
    ApplyAllAgentsScope,
    OpenRolePicker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsGlobalMountsKeyPlan {
    ConfirmSensitiveSave,
    OpenSavePreview,
    ScrollHorizontal { delta: i16 },
    MoveSelection { delta: isize },
    ToggleReadonly,
    ConfirmDiscard,
    ReturnToList,
    OpenAdd,
    ConfirmRemove,
    OpenGithub,
    OpenEdit(GlobalMountTextTarget),
    Noop,
}
