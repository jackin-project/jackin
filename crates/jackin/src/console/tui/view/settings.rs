//! Settings screen render: tabs (Auth, Env, Global Mounts) and their current focus state.
//!
//! Not responsible for: settings state mutations or keyboard event handling —
//! those live in the settings event-loop in `src/console/tui/`.

#![allow(clippy::redundant_pub_crate)]

use crate::console::tui::components::settings::{
    render_auth_tab, render_env_tab, render_general_tab, render_mounts_tab, render_trust_tab,
};
use crate::console::tui::state::{SettingsState, SettingsTab};
use jackin_console::tui::components::editor_rows::render_tab_strip;
use jackin_console::tui::screens::settings::view::{
    settings_frame_areas, settings_header_title, tab_labels,
};
use jackin_console::tui::view::{footer_height, render_footer, render_header};
use ratatui::{Frame, layout::Rect};

pub(super) fn render_settings(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &SettingsState<'_>,
    op_available: bool,
) {
    let provisional_body = settings_frame_areas(area, state.cached_footer_h.max(1)).body;
    let footer = crate::console::tui::components::footer::settings::settings_footer_items(
        state,
        op_available,
        provisional_body,
    );
    let mut footer_h = footer_height(&footer, area.width).max(1);
    let mut areas = settings_frame_areas(area, footer_h);
    let mut footer = crate::console::tui::components::footer::settings::settings_footer_items(
        state,
        op_available,
        areas.body,
    );
    let exact_footer_h = footer_height(&footer, area.width).max(1);
    if exact_footer_h != footer_h {
        footer_h = exact_footer_h;
        areas = settings_frame_areas(area, footer_h);
        footer = crate::console::tui::components::footer::settings::settings_footer_items(
            state,
            op_available,
            areas.body,
        );
    }
    render_header(frame, areas.header, settings_header_title());
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
        state.tab_bar_focused(),
        state.hovered_tab(),
    );

    match state.active_tab {
        SettingsTab::General => render_general_tab(frame, state, areas.body),
        SettingsTab::Mounts => render_mounts_tab(frame, state, areas.body),
        SettingsTab::Environments => render_env_tab(frame, state, areas.body),
        SettingsTab::Auth => render_auth_tab(frame, state, areas.body),
        SettingsTab::Trust => render_trust_tab(frame, state, areas.body),
    }

    render_footer(frame, areas.footer, &footer);
}

#[cfg(test)]
mod tests;
