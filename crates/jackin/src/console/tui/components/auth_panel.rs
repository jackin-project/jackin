//! Root bindings for the console-local auth panel component.

use crate::config::AppConfig;
use crate::console::tui::state::{EditorState, SettingsState, SettingsTab};
use crate::operator_env::EnvValue;
use jackin_console::tui::screens::editor::view::auth_state_lines as editor_auth_state_lines;
use jackin_console::tui::screens::settings::view::auth_state_lines as settings_auth_state_lines;

pub(crate) type AuthForm = jackin_console::tui::components::auth_panel::AuthForm<EnvValue>;

pub(crate) use jackin_console::tui::components::auth_panel::render_form;

pub(crate) fn editor_auth_lines_for_state(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<ratatui::text::Line<'static>> {
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();
    editor_auth_state_lines(state, config, show_cursor)
}

pub(crate) fn settings_auth_lines_for_state(
    state: &SettingsState<'_>,
) -> Vec<ratatui::text::Line<'static>> {
    let show_cursor = state.content_focused(SettingsTab::Auth) && state.auth.modal.is_none();
    settings_auth_state_lines(&state.auth, &state.env, show_cursor)
}
