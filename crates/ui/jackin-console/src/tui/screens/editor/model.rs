// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor screen state: draft workspace config being edited and per-tab/
//! per-field edit state for General, Mounts, Roles, Secrets, and Auth panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).
mod plans_a;
mod plans_b;
mod save;
mod secrets;
mod state;
mod state_impl;
#[cfg(test)]
mod tests;
pub use plans_a::{
    AuthEnterPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan, EditorFieldSelectionKeyPlan,
    EditorFocusTarget, EditorHorizontalScrollKeyPlan, EditorHoverTarget, EditorMountGithubOpenPlan,
    EditorNavigationKeyPlan, EditorRoleHeaderExpansionKeyPlan, EditorSaveKeyPlan, EditorTab,
    RoleHeaderExpansionPlan,
};
pub use plans_b::{
    EditorAuthActionKeyPlan, EditorImmediateActionKeyPlan, EditorMode, EditorMountActionKeyPlan,
    EditorRoleActionKeyPlan, EditorSaveModePlan, EditorSecretsActionKeyPlan,
    EditorTabActionKeyPlan, EditorTopLevelKeyPlan, editor_save_mode_plan,
};
pub use save::{
    ConfirmTarget, EditorSaveFlow, ExitIntent, FileBrowserTarget, PendingSaveCommit,
    TextInputTarget,
};
pub use secrets::{AuthRow, FieldFocus, SecretsEnterPlan, SecretsRow, SecretsScopeTag};
pub use state::{
    EditorErrorPopupModal, EditorRoleOverridePickerModal, EditorSaveDiscardModal, EditorState,
    EditorStatusPopupModal,
};
