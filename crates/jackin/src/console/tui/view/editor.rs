pub use crate::console::tui::state::AuthRow;
use crate::console::tui::state::EditorState;
#[cfg(test)]
pub(crate) use crate::console::tui::state::SecretsRow;
use jackin_config::AppConfig;
use jackin_console::tui::screens::editor::view::render_editor_screen;
#[cfg(test)]
pub(crate) use jackin_console::tui::screens::editor::view::{
    render_general_tab, render_roles_tab, render_secrets_tab,
};
use ratatui::{Frame, layout::Rect};

// ── Editor stage ────────────────────────────────────────────────────

pub(super) fn render_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) {
    render_editor_screen(frame, area, state, config, |state, config, body| {
        crate::console::tui::components::footer::editor::editor_footer_items(
            state,
            config,
            op_available,
            body,
        )
    });
}

#[cfg(test)]
mod tests;
