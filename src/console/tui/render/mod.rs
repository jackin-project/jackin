//! Render functions for the workspace manager TUI.

pub mod editor;
mod frame;
#[cfg(test)]
mod consistency_tests;
#[cfg(test)]
mod frame_tests;
pub(crate) mod list;
pub(crate) mod modal;
pub(crate) mod settings;
#[cfg(test)]
mod snapshot_tests;

pub use frame::render;

pub(super) use jackin_console::tui::layout::centered_rect_fixed;

pub(crate) use jackin_tui::components::scrollable_panel::{
    viewport_width as scroll_viewport_width,
};
pub(super) use jackin_tui::components::scrollable_panel::render_scrollable_block_at;

pub(crate) fn env_value_secret_display(
    value: &crate::operator_env::EnvValue,
) -> jackin_console::tui::components::editor_rows::SecretValueDisplay<'_> {
    match value {
        crate::operator_env::EnvValue::Plain(value) => {
            jackin_console::tui::components::editor_rows::SecretValueDisplay::Plain(value)
        }
        crate::operator_env::EnvValue::OpRef(op_ref) => {
            jackin_console::tui::components::editor_rows::SecretValueDisplay::OpRefPath(
                &op_ref.path,
            )
        }
    }
}
