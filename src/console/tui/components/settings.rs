//! Root-console settings display adapters.

use ratatui::text::Line;

use crate::console::tui::components::env_value_secret_display;
use crate::console::tui::state::{
    SettingsEnvScope, SettingsState, settings_env_flat_rows,
};
use jackin_console::tui::screens::settings::view::{
    env_lines as settings_env_lines, trust_lines as settings_trust_lines,
};

pub(crate) fn settings_env_lines_for_state(
    state: &SettingsState<'_>,
    area_width: u16,
) -> Vec<Line<'static>> {
    let rows = settings_env_flat_rows(state);
    let show_cursor =
        !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
    settings_env_lines(
        &rows,
        state.env.selected,
        show_cursor,
        area_width,
        |scope, key| settings_env_value(state, scope, key).map(env_value_secret_display),
        |scope, key| state.env.unmasked_rows.contains(&(scope.clone(), key.to_string())),
        |role| state.env.pending.roles.get(role).map_or(0, std::collections::BTreeMap::len),
    )
}

pub(crate) fn settings_trust_lines_for_state(
    state: &SettingsState<'_>,
) -> Vec<Line<'static>> {
    let show_cursor = !state.tab_bar_focused
        && state.trust.scroll_focused
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    settings_trust_lines(
        &state.trust.pending,
        state.trust.selected,
        state.trust.hovered,
        show_cursor,
    )
}

fn settings_env_value<'a>(
    state: &'a SettingsState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a crate::operator_env::EnvValue> {
    match scope {
        SettingsEnvScope::Global => state.env.pending.env.get(key),
        SettingsEnvScope::Role(role) => state
            .env
            .pending
            .roles
            .get(role)
            .and_then(|env| env.get(key)),
    }
}
