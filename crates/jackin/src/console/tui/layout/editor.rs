//! Editor geometry and scroll preparation owned by the manager update layer.

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::tui::state::EditorState;

pub(crate) fn prepare_editor_for_render(
    area: Rect,
    state: &mut EditorState<'_>,
    config: &AppConfig,
) {
    jackin_console::tui::screens::editor::view::prepare_editor_for_render(area, state, config);
}

#[cfg(test)]
pub(crate) fn prepare_editor_tab_for_area(
    body: Rect,
    state: &mut EditorState<'_>,
    config: &AppConfig,
) {
    jackin_console::tui::screens::editor::view::prepare_editor_tab_for_area(body, state, config);
}
