//! Console-local error popup state construction.

use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

pub fn error_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> jackin_tui::components::ErrorPopupState {
    jackin_tui::components::ErrorPopupState::new(title, message)
}

/// Shared error dialog for a rejected auth source folder. Used by both the
/// workspace-editor and global-settings source-folder pickers so the
/// rejection looks and reads identically on either surface.
pub fn invalid_source_folder_error_popup_state(
    reason: impl Into<String>,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state("Invalid source folder", reason)
}

pub fn role_load_error_popup_state(
    message: impl Into<String>,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state("Load role failed", message)
}

pub fn configured_role_load_error_message(raw: impl std::fmt::Debug) -> String {
    format!(
        "Could not load role {raw:?}.\n\nUse a configured role such as \
         \"agent-smith\" or a GitHub selector like \"owner/agent-name\"."
    )
}

pub fn repository_role_load_error_message(
    raw: impl std::fmt::Debug,
    source_url: impl std::fmt::Display,
    detail: impl std::fmt::Display,
) -> String {
    format!("Could not load role {raw:?}.\n\nLooked for repository:\n{source_url}\n\n{detail}")
}

pub fn role_repository_unavailable_message() -> &'static str {
    "Repository is not available, or you do not have access."
}

pub fn role_repository_remote_mismatch_message() -> &'static str {
    "A cached copy already exists for this role, but it points at a different repository."
}

pub fn invalid_role_repository_message(detail: impl std::fmt::Display) -> String {
    format!("Repository is not a valid jackin❯ role: {detail}.")
}

pub fn generic_role_repository_error_message() -> &'static str {
    "Repository could not be used as a jackin❯ role."
}

pub fn missing_role_repository_file_message(file: impl std::fmt::Display) -> String {
    format!("missing {file}")
}

pub fn internal_role_load_error_message(
    raw: impl std::fmt::Debug,
    detail: impl std::fmt::Display,
) -> String {
    format!(
        "Could not load role {raw:?}.\n\nThe role loader hit an internal \
         error while registering the repository.\n\n{detail}"
    )
}

pub fn role_input_misroute_error_message() -> &'static str {
    "Role input was routed through the generic text-input handler."
}

pub fn editor_action_error_popup_state(
    err: impl std::fmt::Display,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state(
        "Could not apply change",
        format!("The change could not be saved.\n\n{err}"),
    )
}

pub fn no_github_url_error_popup_state() -> jackin_tui::components::ErrorPopupState {
    error_popup_state(
        "No GitHub URL",
        "This mount has no GitHub remote URL.\n\nOnly git repositories with a GitHub origin support browser preview.",
    )
}

pub fn save_failed_error_popup_state(
    message: impl Into<String>,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state("Save failed", message)
}

pub fn token_generation_failed_error_title() -> &'static str {
    "Token generation failed"
}

pub fn token_generation_failed_error_popup_state(
    error: impl std::fmt::Display,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state(token_generation_failed_error_title(), error.to_string())
}

pub fn failed_to_open_url_error_title() -> &'static str {
    "Failed to open URL"
}

pub fn failed_to_open_url_error_popup_state(
    error: impl std::fmt::Display,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state(failed_to_open_url_error_title(), error.to_string())
}

pub fn delete_failed_error_title() -> &'static str {
    "Delete failed"
}

pub fn file_browser_failed_error_title() -> &'static str {
    "File browser failed"
}

pub fn op_read_failed_error_popup_state(
    error: impl std::fmt::Display,
) -> jackin_tui::components::ErrorPopupState {
    error_popup_state("1Password read failed", error.to_string())
}

pub fn role_resolution_error_title() -> &'static str {
    "Role resolution failed"
}

pub fn role_resolution_error_message(
    role_key: impl std::fmt::Display,
    error: impl std::fmt::Display,
) -> String {
    format!("Could not resolve {role_key}.\n\n{error:#}")
}

pub fn no_eligible_roles_error_title() -> &'static str {
    "No eligible roles"
}

pub fn no_eligible_roles_error_message(workspace_name: impl std::fmt::Display) -> String {
    format!(
        "Workspace \"{workspace_name}\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
    )
}

pub fn instance_unavailable_error_title() -> &'static str {
    "Instance unavailable"
}

pub fn instance_unavailable_error_message() -> &'static str {
    "Instance no longer active; list refreshes automatically."
}

pub fn instance_index_error_title() -> &'static str {
    "Instance index error"
}

pub fn instance_index_error_message(error: impl std::fmt::Display) -> String {
    format!("instance index error: {error}")
}

pub fn no_instance_error_title() -> &'static str {
    "No instance"
}

pub fn no_recoverable_instance_selected_message() -> &'static str {
    "No recoverable instance selected."
}

pub fn no_recoverable_instance_for_workspace_message() -> &'static str {
    "No recoverable instance for this workspace."
}

pub fn no_running_instance_for_workspace_message() -> &'static str {
    "No running instance for this workspace."
}

pub fn no_instance_state_for_workspace_message() -> &'static str {
    "No instance state for this workspace."
}

pub fn no_running_instance_to_stop_message() -> &'static str {
    "No running instance to stop."
}

pub fn no_purgeable_instance_for_workspace_message() -> &'static str {
    "No purgeable instance for this workspace."
}

pub fn instance_action_failed_error_title(action: WorkspaceInstanceAction) -> &'static str {
    match action {
        WorkspaceInstanceAction::Stop => "Stop failed",
        WorkspaceInstanceAction::Purge => "Purge failed",
        _ => "Action failed",
    }
}

pub fn instance_action_failed_error_message(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

#[cfg(test)]
mod tests;
