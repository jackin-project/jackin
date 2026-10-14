// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
