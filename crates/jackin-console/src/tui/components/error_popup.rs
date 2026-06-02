//! Console-local error popup state construction.

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
}
