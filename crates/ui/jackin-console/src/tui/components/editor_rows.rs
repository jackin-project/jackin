// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared row render helpers for editor/settings tabs.
//!
//! Plan 010 step 4 (C10) reviewed the named upstream pairings at rev
//! `29a16b5b` and recorded behavior-preserving non-adoptions (commit-message
//! carve-outs): `FieldRow::paint` inserts a spacing gap after the label band,
//! truncates label and value with `…`, and paints selection fill/tint, while
//! `labeled_field_line` pads the pinned label column with no gap, no
//! truncation, and no row fill; the mount tables are a four-column product
//! layout with continuation rows, not key→value anatomy; and the secret
//! masking here is read-only display rows (fixed 11-glyph mask / clamped
//! 1–12 repeat), not an editor — `PasswordInputState` masks per real secret
//! length with reveal/strength chrome.
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
pub use primitives::{
    AUTH_LABEL_COL_WIDTH, FieldEmphasis, SECRET_LABEL_COL_WIDTH, cursor_gutter, cursor_span,
    labeled_field_line, render_tab_strip,
};
pub use secrets::{render_secret_key_line, secret_env_lines};

pub(crate) use primitives::{padded_width, padded_width_cols, text_width};
