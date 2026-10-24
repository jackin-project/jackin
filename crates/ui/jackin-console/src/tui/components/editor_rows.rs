// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod auth_lines;
mod displays;
mod primitives;
mod secrets;
#[cfg(test)]
mod tests;
pub use auth_lines::{auth_line_width, auth_lines};
pub use displays::{
    AuthLineRow, AuthSourceDisplay, AuthSourceFolderDisplay, AuthSourceFolderKind, AuthSourceValue,
    SecretEnvLineFrame, SecretLineRow, SecretValueDisplay, action_row_style, auth_source_display,
    auth_source_display_for_required_env, disclosure_style,
};
pub(crate) use primitives::tab_strip_hit_regions;
pub use primitives::{
    AUTH_LABEL_COL_WIDTH, FieldEmphasis, SECRET_LABEL_COL_WIDTH, cursor_gutter, cursor_span,
    labeled_field_line, render_tab_strip,
};
pub use secrets::{render_secret_key_line, secret_env_lines};

pub(crate) use primitives::{padded_width, padded_width_cols, text_width};
