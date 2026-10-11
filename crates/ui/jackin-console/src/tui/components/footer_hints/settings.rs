// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings + mount + secret + auth-form hint-span builders for the
//! settings screen's per-row contextual footer.
mod pickers;
mod rows;
mod secrets;
pub use pickers::{
    filtered_picker_footer_items, mount_destination_footer_items, op_section_footer_items,
    pick_list_footer_items, segmented_choice_footer_items,
};
pub use rows::{
    SettingsContextFooterMode, add_row_footer_items, append_generate_token_footer_item,
    settings_contextual_row_footer_items, settings_general_row_footer_items,
    settings_trust_row_footer_items,
};
pub use secrets::{
    global_mount_row_footer_items, secret_add_row_footer_items, secret_op_ref_row_footer_items,
    secret_plain_row_footer_items, secret_role_header_footer_items,
    workspace_mount_row_footer_items,
};
