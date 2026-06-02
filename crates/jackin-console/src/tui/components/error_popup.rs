//! Console-local error popup state construction.

pub fn error_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> jackin_tui::components::ErrorPopupState {
    jackin_tui::components::ErrorPopupState::new(title, message)
}
