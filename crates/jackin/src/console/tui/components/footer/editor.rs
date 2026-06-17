//! Root modal/footer adapter for the workspace editor screen.

use jackin_tui::HintSpan;
use ratatui::layout::Rect;

use crate::console::tui::state::{EditorState, EditorTab, Modal};
use jackin_config::AppConfig;
use jackin_console::tui::components::footer_hints::{
    content_footer_items, editor_save_footer_label, tab_bar_footer_items,
};

pub(crate) fn editor_footer_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if let Some(modal) = &state.modal {
        let can_generate_token = matches!(modal, Modal::AuthForm { .. })
            && crate::console::tui::input::auth::auth_form_can_generate_token(state);
        return modal.footer_items(can_generate_token);
    }
    if state.tab_bar_focused() {
        return tab_bar_footer_items(
            editor_save_footer_label(),
            state.active_tab != EditorTab::General,
            state.is_dirty().then(|| state.change_count()),
        );
    }
    let row_items = jackin_console::tui::screens::editor::view::editor_contextual_footer_items(
        state,
        config,
        op_available,
        body_area,
    );
    content_footer_items(
        editor_save_footer_label(),
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}
