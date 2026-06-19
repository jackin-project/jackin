//! Auth-tab input handling: thin adapter shell.

pub(in crate::console) use jackin_console::tui::input::auth::{
    apply_op_picker_to_auth_form_committed, apply_plain_source_picker_to_auth_form,
    apply_plain_text_to_auth_form, apply_source_folder_to_auth_form, handle_auth_form_key,
    handle_d_on_auth_row, open_auth_form_modal, open_auth_role_picker,
    open_op_picker_from_auth_source, restore_auth_form_after_op_picker_cancel, toggle_role_expand,
};
