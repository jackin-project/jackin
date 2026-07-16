// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console-local reusable TUI components.

pub mod agent_choice;
pub mod auth_panel;
pub mod brand_header;
pub mod confirm_save;
pub mod container_info;
pub use jackin_ui::operator_info as container_info_surface;
pub mod dialogs;
pub mod editor_rows;
pub mod env_value;
pub mod error_popup;
pub mod file_browser;
pub mod footer_hints;
pub mod github_picker;
pub mod modal_rects;
pub mod mount_dst_choice;
pub mod mount_rows;
pub mod op_breadcrumb;
pub mod op_picker;
pub mod provider_picker;
pub mod role_choice;
pub mod role_picker;
pub mod save_discard;
pub mod save_preview;
pub mod scope_picker;
pub mod source_picker;
pub mod spinner;
pub mod status_popup;
pub mod workdir_pick;

pub use dialogs::{
    ConfirmKind, ConfirmState, ErrorPopupState, SaveDiscardChoice, SaveDiscardState,
    StatusPopupState, TextInputState, confirm_hint_spans, error_popup_hint_spans,
    render_confirm_dialog, render_error_dialog, render_save_discard_dialog, render_status_popup,
    render_text_input, save_discard_hint_spans, text_input_hint_spans,
};
