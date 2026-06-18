#![allow(clippy::redundant_pub_crate)]

use crate::console::tui::state::SettingsState;
use jackin_console::tui::screens::settings::view::render_settings_screen;
use ratatui::{Frame, layout::Rect};

pub(super) fn render_settings(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &SettingsState<'_>,
    op_available: bool,
) {
    render_settings_screen(frame, area, state, |state, body| {
        crate::console::tui::components::footer::settings::settings_footer_items(
            state,
            op_available,
            body,
        )
    });
}

#[cfg(test)]
mod tests;
