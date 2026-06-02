//! Console-local error popup state construction.

use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

pub fn error_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> jackin_tui::components::ErrorPopupState {
    jackin_tui::components::ErrorPopupState::new(title, message)
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
    format!("Repository is not a valid jackin' role: {detail}.")
}

pub fn generic_role_repository_error_message() -> &'static str {
    "Repository could not be used as a jackin' role."
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
mod tests {
    use super::*;

    #[test]
    fn role_load_error_popup_uses_standard_title() {
        let state = role_load_error_popup_state("bad role");

        assert_eq!(state.title, "Load role failed");
        assert_eq!(state.message, "bad role");
    }

    #[test]
    fn role_load_error_messages_name_configured_or_repository_source() {
        assert_eq!(
            configured_role_load_error_message("bad-role"),
            "Could not load role \"bad-role\".\n\nUse a configured role such as \"agent-smith\" or a GitHub selector like \"owner/agent-name\"."
        );
        assert_eq!(
            repository_role_load_error_message(
                "bad-role",
                "https://example.test/repo.git",
                "not valid"
            ),
            "Could not load role \"bad-role\".\n\nLooked for repository:\nhttps://example.test/repo.git\n\nnot valid"
        );
        assert_eq!(
            internal_role_load_error_message("bad-role", "panic payload"),
            "Could not load role \"bad-role\".\n\nThe role loader hit an internal error while registering the repository.\n\npanic payload"
        );
        assert_eq!(
            role_repository_unavailable_message(),
            "Repository is not available, or you do not have access.",
        );
        assert_eq!(
            role_repository_remote_mismatch_message(),
            "A cached copy already exists for this role, but it points at a different repository.",
        );
        assert_eq!(
            invalid_role_repository_message("missing jackin.role.toml"),
            "Repository is not a valid jackin' role: missing jackin.role.toml.",
        );
        assert_eq!(
            generic_role_repository_error_message(),
            "Repository could not be used as a jackin' role.",
        );
        assert_eq!(
            missing_role_repository_file_message("jackin.role.toml"),
            "missing jackin.role.toml",
        );
        assert_eq!(
            role_input_misroute_error_message(),
            "Role input was routed through the generic text-input handler."
        );
    }

    #[test]
    fn editor_action_error_popup_names_failed_change() {
        let state = editor_action_error_popup_state("disk full");

        assert_eq!(state.title, "Could not apply change");
        assert!(state.message.contains("disk full"));
    }

    #[test]
    fn no_github_url_error_popup_explains_missing_remote() {
        let state = no_github_url_error_popup_state();

        assert_eq!(state.title, "No GitHub URL");
        assert!(state.message.contains("GitHub origin"));
    }

    #[test]
    fn save_failed_error_popup_uses_standard_title() {
        let state = save_failed_error_popup_state("bad config");

        assert_eq!(state.title, "Save failed");
        assert_eq!(state.message, "bad config");
    }

    #[test]
    fn effect_error_popup_helpers_use_standard_titles() {
        let token = token_generation_failed_error_popup_state("op failed");
        let url = failed_to_open_url_error_popup_state("browser failed");

        assert_eq!(token.title, token_generation_failed_error_title());
        assert_eq!(token.message, "op failed");
        assert_eq!(url.title, failed_to_open_url_error_title());
        assert_eq!(url.message, "browser failed");
        assert_eq!(delete_failed_error_title(), "Delete failed");
        assert_eq!(file_browser_failed_error_title(), "File browser failed");
    }

    #[test]
    fn op_read_failed_error_popup_uses_standard_title() {
        let state = op_read_failed_error_popup_state("Touch ID rejected");

        assert_eq!(state.title, "1Password read failed");
        assert_eq!(state.message, "Touch ID rejected");
    }

    #[test]
    fn role_resolution_error_names_role() {
        assert_eq!(role_resolution_error_title(), "Role resolution failed");
        assert_eq!(
            role_resolution_error_message("agent-smith", "not found"),
            "Could not resolve agent-smith.\n\nnot found"
        );
    }

    #[test]
    fn no_eligible_roles_error_names_workspace() {
        assert_eq!(no_eligible_roles_error_title(), "No eligible roles");
        assert_eq!(
            no_eligible_roles_error_message("demo"),
            "Workspace \"demo\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
        );
    }

    #[test]
    fn list_instance_error_helpers_use_standard_wording() {
        assert_eq!(instance_unavailable_error_title(), "Instance unavailable");
        assert_eq!(
            instance_unavailable_error_message(),
            "Instance no longer active; list refreshes automatically."
        );
        assert_eq!(instance_index_error_title(), "Instance index error");
        assert_eq!(
            instance_index_error_message("missing record"),
            "instance index error: missing record"
        );
        assert_eq!(no_instance_error_title(), "No instance");
        assert_eq!(
            no_recoverable_instance_selected_message(),
            "No recoverable instance selected."
        );
        assert_eq!(
            no_recoverable_instance_for_workspace_message(),
            "No recoverable instance for this workspace."
        );
        assert_eq!(
            no_running_instance_for_workspace_message(),
            "No running instance for this workspace."
        );
        assert_eq!(
            no_instance_state_for_workspace_message(),
            "No instance state for this workspace."
        );
        assert_eq!(
            no_running_instance_to_stop_message(),
            "No running instance to stop."
        );
        assert_eq!(
            no_purgeable_instance_for_workspace_message(),
            "No purgeable instance for this workspace."
        );
    }

    #[test]
    fn instance_action_failed_title_names_in_place_actions() {
        assert_eq!(
            instance_action_failed_error_title(WorkspaceInstanceAction::Stop),
            "Stop failed"
        );
        assert_eq!(
            instance_action_failed_error_title(WorkspaceInstanceAction::Purge),
            "Purge failed"
        );
        assert_eq!(
            instance_action_failed_error_title(WorkspaceInstanceAction::Inspect),
            "Action failed"
        );
        assert_eq!(
            instance_action_failed_error_message(anyhow::anyhow!("docker failed")),
            "docker failed"
        );
    }
}
