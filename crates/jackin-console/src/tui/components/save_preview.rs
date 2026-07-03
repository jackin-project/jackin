//! Save-confirm preview line builders for console-local dialogs.
//!
//! This module deliberately does not use `jackin_tui::components::diff_view`:
//! that component renders text hunks, while save previews render semantic
//! config-change rows with domain labels, summaries, and secret redaction.

mod settings;
mod settings_lines;
mod workspace;
mod workspace_lines;

pub use settings::{
    AuthPreviewRow, ConsoleSettingsState, MountPreviewRow, SettingsEnvPreview,
    SettingsGeneralPreview, SettingsGeneralToggles, SettingsSavePreview, TrustPreviewRow,
    build_settings_save_lines, global_mount_preview_row, settings_env_preview,
    settings_save_preview,
};
pub use settings_lines::{append_env_map_diff_lines, settings_save_lines};
pub use workspace::{
    WorkspaceAuthChange, WorkspaceMountDiff, WorkspaceMountPreviewRow, WorkspaceSaveMode,
    WorkspaceSavePreview, WorkspaceToggleSet, build_workspace_save_lines, credential_label,
    credential_presence, role_auth_relevant, source_folder_text, workspace_auth_change,
    workspace_auth_changes, workspace_create_display_name, workspace_env_preview,
    workspace_mount_preview_row, workspace_save_preview,
};
pub use workspace_lines::{collapse_removal_lines, collapse_section_lines, workspace_save_lines};

#[cfg(test)]
mod tests;
