//! Console-local status popup state construction.

pub fn status_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> jackin_tui::components::StatusPopupState {
    jackin_tui::components::StatusPopupState::new(title, message)
}

pub fn role_resolution_status_popup_state(
    role_key: impl std::fmt::Display,
) -> jackin_tui::components::StatusPopupState {
    status_popup_state("Resolving agent role", format!("Loading and resolving {role_key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_resolution_status_popup_names_role() {
        let state = role_resolution_status_popup_state("agent-smith");
        let debug = format!("{state:?}");

        assert!(debug.contains("Resolving agent role"));
        assert!(debug.contains("Loading and resolving agent-smith"));
    }
}
