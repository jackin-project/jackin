//! Editor-stage rendering.
//!
//! Full-screen editor with header, tab bar, per-tab body renderers
//! (General / Mounts / Roles / Secrets), and the contextual footer
//! composition that varies with the active tab + cursor.

use crate::config::AppConfig;
use crate::console::tui::components::editor::{
    render_auth_tab, render_editor_tab_strip, render_general_tab, render_mounts_tab,
    render_roles_tab, render_secrets_tab,
};
pub use crate::console::tui::state::AuthRow;
#[cfg(test)]
pub(crate) use crate::console::tui::state::SecretsRow;
use crate::console::tui::state::{EditorState, EditorTab};
#[cfg(test)]
pub(crate) use crate::console::tui::state::{
    eligible_agents_for_override, resolve_auth_row_target,
};
use jackin_console::tui::screens::editor::view::{editor_frame_areas, editor_header_title};
use jackin_console::tui::view::{footer_height, render_footer, render_header};
use ratatui::{Frame, layout::Rect};

// ── Editor stage ────────────────────────────────────────────────────

pub(super) fn render_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) {
    let items = crate::console::tui::components::footer::editor::editor_footer_items(
        state,
        config,
        op_available,
    );
    let footer_h = footer_height(&items, area.width).max(1);
    let areas = editor_frame_areas(area, footer_h);

    let title = editor_header_title(&state.mode);
    render_header(frame, areas.header, &title);
    render_editor_tab_strip(
        frame,
        areas.tabs,
        state.active_tab,
        state.tab_bar_focused(),
        state.hovered_tab,
    );

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, areas.body, state),
        EditorTab::Mounts => render_mounts_tab(frame, areas.body, state),
        EditorTab::Roles => render_roles_tab(frame, areas.body, state, config),
        EditorTab::Secrets => render_secrets_tab(frame, areas.body, state, config),
        EditorTab::Auth => render_auth_tab(frame, areas.body, state, config),
    }

    render_footer(frame, areas.footer, &items);
}

#[cfg(test)]
mod contextual_row_items_tests;

#[cfg(test)]
mod general_tab_render_tests;

#[cfg(test)]
mod mounts_tab_render_tests;

#[cfg(test)]
mod agents_tab_render_tests;

#[cfg(test)]
mod secrets_tab_render_tests;

#[cfg(test)]
mod eligible_agents_for_override_tests;

#[cfg(test)]
mod auth_flat_rows_tests;
