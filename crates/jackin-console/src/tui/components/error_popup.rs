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
}
