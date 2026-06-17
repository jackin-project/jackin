//! Root modal/footer adapter for the settings screen.

use crate::console::tui::state::SettingsState;
use jackin_tui::HintSpan;
use ratatui::layout::Rect;

pub(crate) fn settings_footer_items(
    state: &SettingsState<'_>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if let Some(modal) = &state.auth.modal {
        modal.footer_items(
            crate::console::tui::input::global_mounts::settings_auth_can_generate_token(
                &state.auth,
            ),
        )
    } else if let Some(modal) = &state.env.modal {
        modal.footer_items()
    } else if let Some(modal) = &state.mounts.modal {
        modal.footer_items()
    } else {
        jackin_console::tui::screens::settings::view::settings_footer_items(
            state,
            op_available,
            body_area,
        )
    }
}
