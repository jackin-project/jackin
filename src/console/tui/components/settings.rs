//! Root-console settings display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::console::tui::components::auth_panel::settings_auth_lines_for_state;
use crate::console::tui::components::env_value_secret_display;
use crate::console::tui::components::mount_display::format_mount_rows_with_cache;
use crate::console::tui::state::{
    MountInfoCache, SettingsEnvScope, SettingsState, settings_env_flat_rows,
};
use jackin_console::tui::screens::settings::view::{
    env_lines as settings_env_lines, global_mount_lines as settings_global_mount_lines,
    general_lines as settings_general_lines, trust_lines as settings_trust_lines,
};

pub(crate) fn render_general_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let focused = !state.tab_bar_focused && state.error_popup.is_none();
    let lines = settings_general_lines(
        state.general.selected,
        state.general.pending_coauthor_trailer,
        state.general.pending_dco,
        focused,
    );
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame, area, lines, 0, 0, focused, None,
    );
}

pub(crate) fn render_mounts_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let focused =
        !state.tab_bar_focused && state.mounts.scroll_focused && state.mounts.modal.is_none();
    let selected = if focused {
        Some(state.mounts.selected)
    } else {
        None
    };
    let lines = global_mount_lines_for_rows(
        &state.mounts.pending,
        selected,
        true,
        &state.mounts.mount_info_cache,
    );
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.mounts.scroll_x,
        state.mounts.scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_env_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let lines = settings_env_lines_for_state(state, area.width);
    let focused = !state.tab_bar_focused && state.env.scroll_focused && state.env.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.env.scroll_y,
        focused,
        None,
    );
}

pub(crate) fn render_auth_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let title = state.auth.selected_kind.map(|k| format!(" {} ", k.label()));
    let lines = settings_auth_lines_for_state(state);
    let focused = !state.tab_bar_focused && state.auth.scroll_focused && state.auth.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.auth.scroll_y,
        focused,
        title.as_deref(),
    );
}

pub(crate) fn render_trust_tab(frame: &mut Frame, state: &SettingsState<'_>, area: Rect) {
    let lines = settings_trust_lines_for_state(state);
    let focused = !state.tab_bar_focused
        && state.trust.scroll_focused
        && state.auth.modal.is_none()
        && state.env.modal.is_none()
        && state.mounts.modal.is_none();
    jackin_tui::components::scrollable_panel::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.trust.scroll_x,
        state.trust.scroll_y,
        focused,
        None,
    );
}

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

pub(crate) fn global_mount_lines_for_rows(
    rows: &[crate::config::GlobalMountRow],
    selected: Option<usize>,
    include_sentinel: bool,
    cache: &MountInfoCache,
) -> Vec<Line<'static>> {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    settings_global_mount_lines(&display_rows, selected, include_sentinel)
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
