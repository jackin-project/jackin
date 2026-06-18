//! Root modal/footer adapter for the settings screen.

use crate::console::tui::state::{GlobalMountModal, SettingsEnvModal, SettingsState};
use jackin_console::tui::components::footer_hints::{
    SettingsScreenFooterFacts, settings_screen_footer_items,
};
use jackin_tui::HintSpan;
use ratatui::layout::Rect;

pub(crate) fn settings_footer_items(
    state: &SettingsState<'_>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    settings_screen_footer_items(SettingsScreenFooterFacts {
        auth_modal_items: state.auth.modal_ref().map(|modal| {
            modal.footer_items(
                crate::console::tui::input::global_mounts::settings_auth_can_generate_token(
                    &state.auth,
                ),
            )
        }),
        env_modal_items: state.env.modal.as_ref().map(SettingsEnvModal::footer_items),
        mounts_modal_items: state
            .mounts
            .modal
            .as_ref()
            .map(GlobalMountModal::footer_items),
        screen_items: jackin_console::tui::screens::settings::view::settings_footer_items(
            state,
            op_available,
            body_area,
        ),
    })
}
