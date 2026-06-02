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
}
