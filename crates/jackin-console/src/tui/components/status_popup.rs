//! Console-local status popup state construction.

pub fn status_popup_state(
    title: impl Into<String>,
    message: impl Into<String>,
) -> jackin_tui::components::StatusPopupState {
    jackin_tui::components::StatusPopupState::new(title, message)
}
