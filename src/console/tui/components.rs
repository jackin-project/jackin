//! Root-console local TUI components and adapters.

pub(crate) mod auth_panel;
pub(crate) mod editor;
pub(crate) mod footer;
pub(crate) mod modal;
pub(crate) mod modal_layout;
pub(crate) mod mount_display;
pub(crate) mod op_picker;
pub(crate) mod save_preview;
pub(crate) mod settings;
pub(crate) mod workspace_list;

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
