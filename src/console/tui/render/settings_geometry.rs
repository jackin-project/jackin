//! Settings tab geometry used by input/update code.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::console::tui::render::mount_display::{
    settings_global_mounts_content_height, settings_global_mounts_content_width_with_cache,
};
use crate::console::tui::state::{GlobalMountsState, SettingsState};

pub(crate) fn clamp_global_mounts_scroll_for_frame(area: Rect, global: &mut GlobalMountsState<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        settings_global_mounts_content_width_with_cache(&global.pending, &global.mount_info_cache),
        jackin_tui::components::scrollable_panel::viewport_width(chunks[2]),
        &mut global.scroll_x,
    );
}

pub(crate) fn mounts_content_height(state: &SettingsState<'_>) -> usize {
    jackin_console::tui::screens::settings::view::content_height_with_error_rows(
        settings_global_mounts_content_height(&state.mounts.pending),
        state.mounts.error.is_some(),
    )
}

pub(crate) fn env_content_height(state: &SettingsState<'_>) -> usize {
    let height = jackin_console::tui::screens::settings::update::settings_env_flat_rows(
        &state.env.pending,
        &state.env.expanded,
    )
    .len();
    jackin_console::tui::screens::settings::view::content_height_with_error_rows(
        height,
        state.env.error.is_some(),
    )
}

pub(crate) fn auth_content_height(state: &SettingsState<'_>) -> usize {
    jackin_console::tui::screens::settings::view::auth_content_height(
        state.auth.selected_kind,
        &state.auth.pending,
        |kind, mode| kind.required_env_var(*mode).is_some(),
        state.auth.error.is_some(),
    )
}

pub(crate) fn trust_content_height(state: &SettingsState<'_>) -> usize {
    let height = 1 + state.trust.pending.len().max(1);
    jackin_console::tui::screens::settings::view::content_height_with_error_rows(
        height,
        state.trust.error.is_some(),
    )
}
