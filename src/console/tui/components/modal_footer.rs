//! Footer hint items for manager modal states.

use jackin_tui::HintSpan;
use jackin_tui::components::hint_bar::CONFIRM_DISMISS_HINT;

use crate::console::manager::state::{
    AuthFormFocus, GlobalMountModal, Modal, SettingsAuthModal, SettingsAuthState, SettingsEnvModal,
};
use jackin_console::tui::components::footer_hints::{
    auth_form_footer_items as shared_auth_form_footer_items, confirm_save_footer_items,
    container_info_footer_items, error_popup_footer_items, filtered_picker_footer_items,
    mount_destination_footer_items, op_section_footer_items, pick_list_footer_items,
    save_discard_cancel_footer_items, segmented_choice_footer_items, status_popup_footer_items,
    yes_no_footer_items,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn modal_footer_items(modal: &Modal<'_>) -> Vec<HintSpan<'static>> {
    match modal {
        Modal::AuthForm { state, focus, .. } => auth_form_footer_items(state.as_ref(), *focus),
        Modal::TextInput { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        Modal::FileBrowser { state, .. } => state.footer_items(),
        Modal::MountDstChoice { .. } => mount_destination_footer_items(),
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } | Modal::ScopePicker { .. } => {
            segmented_choice_footer_items()
        }
        Modal::WorkdirPick { .. } => pick_list_footer_items("select"),
        Modal::GithubPicker { .. } => pick_list_footer_items("confirm"),
        Modal::ConfirmSave { state } => confirm_save_footer_items(!state.lines.is_empty()),
        Modal::SaveDiscardCancel { .. } => save_discard_cancel_footer_items(),
        Modal::ErrorPopup { .. } => error_popup_footer_items(),
        Modal::ContainerInfo { .. } => container_info_footer_items(),
        Modal::StatusPopup { .. } => status_popup_footer_items(),
        Modal::OpPicker { state } if state.naming_stage_input().is_some() => {
            CONFIRM_DISMISS_HINT.to_vec()
        }
        Modal::OpPicker { state }
            if state.stage == crate::console::tui::components::op_picker::OpPickerStage::Section =>
        {
            op_section_footer_items()
        }
        Modal::OpPicker { .. } => filtered_picker_footer_items(true),
        Modal::RolePicker { .. }
        | Modal::RoleOverridePicker { .. }
        | Modal::AuthRolePicker { .. } => filtered_picker_footer_items(false),
        Modal::Confirm { .. } => yes_no_footer_items(),
    }
}

pub(crate) fn settings_mounts_modal_footer_items(
    modal: &GlobalMountModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        GlobalMountModal::Text { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        GlobalMountModal::FileBrowser { state } => state.footer_items(),
        GlobalMountModal::MountDstChoice { .. } => mount_destination_footer_items(),
        GlobalMountModal::ScopePicker { .. } => segmented_choice_footer_items(),
        GlobalMountModal::RolePicker { .. } => filtered_picker_footer_items(false),
        GlobalMountModal::Confirm { .. } => yes_no_footer_items(),
        GlobalMountModal::PreviewSave { state } => {
            confirm_save_footer_items(!state.lines.is_empty())
        }
    }
}

pub(crate) fn settings_env_modal_footer_items(
    modal: &SettingsEnvModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        SettingsEnvModal::Text { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        SettingsEnvModal::SourcePicker { .. } | SettingsEnvModal::ScopePicker { .. } => {
            segmented_choice_footer_items()
        }
        SettingsEnvModal::OpPicker { .. } | SettingsEnvModal::RolePicker { .. } => {
            filtered_picker_footer_items(false)
        }
        SettingsEnvModal::Confirm { .. } => yes_no_footer_items(),
    }
}

pub(crate) fn settings_auth_modal_footer_items(auth: &SettingsAuthState) -> Vec<HintSpan<'static>> {
    let Some(modal) = auth.modal.as_ref() else {
        return Vec::new();
    };
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let mut items = auth_form_footer_items(state.as_ref(), *focus);
            if crate::console::manager::input::global_mounts::settings_auth_can_generate_token(auth)
            {
                items.extend([
                    HintSpan::GroupSep,
                    HintSpan::Key("G"),
                    HintSpan::Text("generate"),
                ]);
            }
            items
        }
        SettingsAuthModal::TextInput { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        SettingsAuthModal::SourcePicker { .. } => segmented_choice_footer_items(),
        SettingsAuthModal::OpPicker { state } if state.naming_stage_input().is_some() => {
            CONFIRM_DISMISS_HINT.to_vec()
        }
        SettingsAuthModal::OpPicker { state }
            if state.stage == crate::console::tui::components::op_picker::OpPickerStage::Section =>
        {
            op_section_footer_items()
        }
        SettingsAuthModal::OpPicker { .. } => filtered_picker_footer_items(false),
    }
}

fn auth_form_footer_items(
    form: &crate::console::tui::components::auth_panel::AuthForm,
    focus: AuthFormFocus,
) -> Vec<HintSpan<'static>> {
    shared_auth_form_footer_items(focus, form.shows_credential_block())
}
