//! `EnvValue` display adapters for console rows.

pub fn secret_display(
    value: &jackin_core::EnvValue,
) -> crate::tui::components::editor_rows::SecretValueDisplay<'_> {
    match value {
        jackin_core::EnvValue::Plain(value) => {
            crate::tui::components::editor_rows::SecretValueDisplay::Plain(value)
        }
        jackin_core::EnvValue::Extended(e) => {
            crate::tui::components::editor_rows::SecretValueDisplay::Plain(&e.value)
        }
        jackin_core::EnvValue::OpRef(op_ref) => {
            crate::tui::components::editor_rows::SecretValueDisplay::OpRefPath(&op_ref.path)
        }
    }
}

#[cfg(test)]
mod tests;
