//! Editor-stage rendering.
//!
//! Full-screen editor with header, tab bar, per-tab body renderers
//! (General / Mounts / Roles / Secrets), and the contextual footer
//! composition that varies with the active tab + cursor.

use crate::config::AppConfig;
use crate::console::tui::components::editor::{
    render_auth_tab, render_general_tab, render_mounts_tab, render_roles_tab, render_secrets_tab,
};
pub use crate::console::tui::state::AuthRow;
#[cfg(test)]
pub(crate) use crate::console::tui::state::SecretsRow;
#[cfg(test)]
pub(crate) use crate::console::tui::state::resolve_auth_row_target;
use crate::console::tui::state::{EditorState, EditorTab};
use jackin_console::tui::components::editor_rows::render_tab_strip;
use jackin_console::tui::screens::editor::view::{
    editor_frame_areas, editor_header_title, tab_labels,
};
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
    let provisional_body = editor_frame_areas(area, state.cached_footer_h.max(1)).body;
    let items = crate::console::tui::components::footer::editor::editor_footer_items(
        state,
        config,
        op_available,
        provisional_body,
    );
    let mut footer_h = footer_height(&items, area.width).max(1);
    let mut areas = editor_frame_areas(area, footer_h);
    let mut items = crate::console::tui::components::footer::editor::editor_footer_items(
        state,
        config,
        op_available,
        areas.body,
    );
    let exact_footer_h = footer_height(&items, area.width).max(1);
    if exact_footer_h != footer_h {
        footer_h = exact_footer_h;
        areas = editor_frame_areas(area, footer_h);
        items = crate::console::tui::components::footer::editor::editor_footer_items(
            state,
            config,
            op_available,
            areas.body,
        );
    }

    let title = editor_header_title(&state.mode);
    render_header(frame, areas.header, &title);
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
        state.tab_bar_focused(),
        state.hovered_tab(),
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
mod tests;
