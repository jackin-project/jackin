//! Root bindings for the console-local auth panel component.

use crate::console::tui::state::{SettingsState, SettingsTab};
use crate::operator_env::EnvValue;
use jackin_console::tui::screens::settings::view::auth_state_lines as settings_auth_state_lines;

pub(crate) type AuthForm = jackin_console::tui::components::auth_panel::AuthForm<EnvValue>;

pub(crate) use jackin_console::tui::components::auth_panel::render_form;

pub(crate) fn settings_auth_lines_for_state(
    state: &SettingsState<'_>,
) -> Vec<ratatui::text::Line<'static>> {
    let show_cursor = state.content_focused(SettingsTab::Auth) && state.auth.modal.is_none();
    settings_auth_state_lines(&state.auth, &state.env, show_cursor)
}
