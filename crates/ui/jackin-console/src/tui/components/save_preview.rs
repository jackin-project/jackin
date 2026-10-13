// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Save-confirm preview line builders for console-local dialogs.
mod diff_lines;
mod mount_diff;
mod settings_lines;
mod settings_types;
#[cfg(test)]
mod tests;
mod workspace_lines;
mod workspace_types;
pub use diff_lines::append_env_map_diff_lines;
pub(crate) use diff_lines::{
    mount_map, settings_auth_diff_lines, settings_default_account_lines, settings_env_diff_lines,
    settings_mount_diff_lines, settings_trust_diff_lines,
};
pub(crate) use mount_diff::workspace_mount_diffs_preview;
pub use mount_diff::{
    WorkspaceAuthChange, WorkspaceMountDiff, WorkspaceMountPreviewRow, collapse_removal_lines,
    collapse_section_lines, workspace_auth_change, workspace_auth_changes, workspace_env_preview,
    workspace_mount_preview_row,
};
pub(crate) use settings_lines::enabled_label;
pub use settings_lines::settings_save_lines;
pub use settings_types::{
    ConsoleSettingsState, MountPreviewRow, SettingsEnvPreview, SettingsGeneralPreview,
    SettingsGeneralToggles, SettingsSavePreview, TrustPreviewRow, build_settings_save_lines,
    global_mount_preview_row, settings_env_preview, settings_save_preview,
};
pub use workspace_lines::workspace_save_lines;
pub use workspace_types::{
    WorkspaceSaveMode, WorkspaceSavePreview, WorkspaceToggleSet, build_workspace_save_lines,
    workspace_create_display_name, workspace_save_preview,
};
