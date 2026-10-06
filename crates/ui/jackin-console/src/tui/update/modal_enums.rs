// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Picker and modal plan enumerations.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlinePickerShellPlan {
    ScrollHorizontal(i16),
    Exit,
    Delegate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlinePickerPlan<T> {
    Commit(T),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserModalPlan<T> {
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<T>),
    ResolveGitUrl(std::path::PathBuf),
    OpenUrl(String),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDstChoicePlan {
    CommitSamePath,
    OpenEditInput,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDiscardModalPlan {
    Save,
    Discard,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmSaveModalPlan {
    Commit,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolConfirmModalPlan {
    Confirm,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOpPickerPlan<S> {
    Commit(S),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceFolderPickerPlan<T> {
    Commit(T),
    Close,
    KeepModal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePickerPlan {
    AllAgents,
    SpecificAgent,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePickerPlan {
    Plain,
    Op,
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListGithubPickerPlan {
    OpenUrl(String),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListModalKeyTarget {
    GithubPicker,
    RolePicker,
    ErrorPopup,
    ContainerInfo,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRolePickerPlan<R> {
    Launch(R),
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissibleModalPlan {
    Dismiss,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListModalScrollTarget {
    GithubPicker,
    RolePicker,
    OpPicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedModalScrollTarget {
    WorkdirPick,
    RolePicker,
    OpPicker,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsModalScrollTarget {
    MountRolePicker,
    EnvOpPicker,
    EnvRolePicker,
    AuthOpPicker,
    None,
}
