// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Concrete console manager state and type bindings.
//!
//! `ManagerState` is the single central struct that the host console TUI
//! owns across its entire lifetime. All field types are lower-crate types
//! (from `jackin-core`, `jackin-config`, `jackin-env`, `jackin-protocol`,
//! `TermRock`, and this crate) so the root binary can depend on this
//! module without creating a circular dependency.
mod aliases;
mod helpers;
mod manager;
mod manager_state;
#[cfg(test)]
mod tests;
pub mod update;
pub use crate::mount_info_cache::MountInfoCache;
pub use crate::tui::focus::MountScrollFocus;
pub use crate::tui::model::SecretsPickerTarget;
pub use crate::tui::screens::editor::model::{
    AuthRow as GenericAuthRow, EditorHoverTarget, EditorMode, EditorTab, ExitIntent, FieldFocus,
    FileBrowserTarget, SecretsEnterPlan, SecretsRow, SecretsScopeTag, TextInputTarget,
};
pub use crate::tui::screens::settings::model::{
    AuthFormFocus, GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, SettingsEnvConfirm,
    SettingsEnvEnterPlan, SettingsEnvOpPickerTarget, SettingsEnvRow, SettingsEnvScope,
    SettingsEnvTextTarget, SettingsGeneralState, SettingsHoverTarget, SettingsTab,
    SettingsTrustRow, SettingsTrustState,
};
pub use crate::tui::screens::usage::{UsageAccount, UsageScreenState};
pub use crate::tui::screens::workspaces::model::{
    ManagerHoverTarget, ManagerListRow, WorkspaceSummary,
};
pub use crate::tui::split::{
    DEFAULT_SPLIT_PCT, DragState, MAX_SPLIT_PCT, MIN_SPLIT_PCT, clamp_split,
};
pub use aliases::{
    AccountPickerState, AgentChoiceState, AuthForm, AuthFormTarget, AuthRow, ConfirmTarget,
    CreatePreludeState, EditorSaveFlow, EditorState, GlobalMountsState, ManagerConfigSaveResult,
    ManagerEffect, ManagerInstanceRefreshSnapshot, ManagerStage, Modal, MountInfoRefreshTarget,
    PendingDriftCheck, PendingFileBrowserCommit, PendingFileBrowserListing,
    PendingIsolationCleanup, PendingMountInfoRefresh, PendingOpCommit, PendingRoleLoad,
    PendingSaveCommit, RolePickerState, SettingsAuthState, SettingsEnvConfig, SettingsEnvState,
    SettingsModal, SettingsState, UsageRouteState, WorkspaceSaveEffect,
};
pub(crate) use helpers::record_console_error;
pub use helpers::{
    active_instances_matching, add_role_to_workspace_editor, open_editor_action_error,
    open_role_input_error, open_role_trust_confirm, visible_instances_matching,
};
pub use manager_state::ManagerState;
