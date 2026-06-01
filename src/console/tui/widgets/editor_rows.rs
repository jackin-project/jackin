//! Compatibility shim for extracted editor/settings row render helpers.

use crate::operator_env::EnvValue;
use ratatui::text::Line;

pub(crate) use jackin_console::widgets::editor_rows::{
    action_row_style, disclosure_style, render_tab_strip,
};

pub(crate) fn render_secret_key_line(
    selected: bool,
    cursor_col: &str,
    key: &str,
    value: &EnvValue,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> Line<'static> {
    let value = match value {
        EnvValue::Plain(value) => {
            jackin_console::widgets::editor_rows::SecretValueDisplay::Plain(value)
        }
        EnvValue::OpRef(op_ref) => {
            jackin_console::widgets::editor_rows::SecretValueDisplay::OpRefPath(&op_ref.path)
        }
    };
    jackin_console::widgets::editor_rows::render_secret_key_line(
        selected,
        cursor_col,
        key,
        value,
        masked,
        area_width,
        label_width,
    )
}
