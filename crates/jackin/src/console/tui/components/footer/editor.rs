//! Root modal/footer adapter for the workspace editor screen.

use jackin_tui::HintSpan;
use ratatui::layout::Rect;

use crate::console::tui::state::{EditorState, EditorTab};
use jackin_config::AppConfig;
use jackin_console::tui::components::footer_hints::{
    EditorScreenFooterFacts, editor_save_footer_label, editor_screen_footer_items,
};

pub(crate) fn editor_footer_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if let Some(modal) = &state.modal {
        return editor_screen_footer_items(EditorScreenFooterFacts::Modal {
            items: modal.footer_items(state.auth_form_can_generate_token()),
        });
    }
    if state.tab_bar_focused() {
        return editor_screen_footer_items(EditorScreenFooterFacts::TabBar {
            save_label: editor_save_footer_label(),
            enter_content: state.active_tab != EditorTab::General,
            dirty_change_count: state.is_dirty().then(|| state.change_count()),
        });
    }
    let row_items = jackin_console::tui::screens::editor::view::editor_contextual_footer_items(
        state,
        config,
        op_available,
        body_area,
    );
    editor_screen_footer_items(EditorScreenFooterFacts::Content {
        save_label: editor_save_footer_label(),
        row_items,
        dirty_change_count: state.is_dirty().then(|| state.change_count()),
    })
}
