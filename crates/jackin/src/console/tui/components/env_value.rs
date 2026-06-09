//! Root-console `EnvValue` display adapters.

pub(crate) fn secret_display(
    value: &crate::operator_env::EnvValue,
) -> jackin_console::tui::components::editor_rows::SecretValueDisplay<'_> {
    match value {
        crate::operator_env::EnvValue::Plain(value) => {
            jackin_console::tui::components::editor_rows::SecretValueDisplay::Plain(value)
        }
        crate::operator_env::EnvValue::Extended(value) => {
            jackin_console::tui::components::editor_rows::SecretValueDisplay::Plain(&value.value)
        }
        crate::operator_env::EnvValue::OpRef(op_ref) => {
            jackin_console::tui::components::editor_rows::SecretValueDisplay::OpRefPath(
                &op_ref.path,
            )
        }
    }
}
