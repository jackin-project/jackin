// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console-owned config save diff planning.
//!
//! The root binary still applies these operations through `ConfigEditor`, but
//! the rules for what changed between the original and pending console models
//! live with the console crate.
mod diff;
mod edits;
mod preview;
mod settings;
#[cfg(test)]
mod tests;
pub(crate) use diff::{push_auth_forward_diff, push_env_diff, validate_settings_env_keys};
pub use edits::{
    WorkspaceSaveDiffOp, build_workspace_edit, validate_settings_env, workspace_save_diff_plan,
};
pub use preview::{
    EditorSavePreviewError, EditorSavePreviewInput, EditorSavePreviewPlan,
    plan_editor_save_preview, pre_existing_redundant_mounts_message,
};
pub use settings::{SettingsSaveInput, save_settings};
