//! Footer hint items for manager modal states.

use jackin_tui::HintSpan;

use crate::console::tui::state::{
    GlobalMountModal, Modal, SettingsAuthModal, SettingsAuthState, SettingsEnvModal,
};
use jackin_console::tui::components::footer_hints::{
    ModalFooterMode, modal_footer_items as shared_modal_footer_items, op_picker_modal_footer_mode,
    pick_list_confirm_footer_label, pick_list_select_footer_label,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn modal_footer_items(
    modal: &Modal<'_>,
    can_generate_token: bool,
) -> Vec<HintSpan<'static>> {
    match modal {
        Modal::AuthForm { state, focus, .. } => shared_modal_footer_items(ModalFooterMode::AuthForm {
            focus: *focus,
            shows_credential_block: state.shows_credential_block(),
            can_generate_token,
        }),
        Modal::TextInput { .. } => shared_modal_footer_items(ModalFooterMode::ConfirmDismiss),
        Modal::FileBrowser { state, .. } => state.footer_items(),
        Modal::MountDstChoice { .. } => {
            shared_modal_footer_items(ModalFooterMode::MountDestination)
        }
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } | Modal::ScopePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::SegmentedChoice)
        }
        Modal::WorkdirPick { .. } => {
            shared_modal_footer_items(ModalFooterMode::PickList {
                commit_label: pick_list_select_footer_label(),
            })
        }
        Modal::GithubPicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::PickList {
                commit_label: pick_list_confirm_footer_label(),
            })
        }
        Modal::ConfirmSave { state } => shared_modal_footer_items(ModalFooterMode::ConfirmSave {
            scrollable: !state.lines.is_empty(),
        }),
        Modal::SaveDiscardCancel { .. } => {
            shared_modal_footer_items(ModalFooterMode::SaveDiscardCancel)
        }
        Modal::ErrorPopup { .. } => shared_modal_footer_items(ModalFooterMode::ErrorPopup),
        Modal::ContainerInfo { .. } => shared_modal_footer_items(ModalFooterMode::ContainerInfo),
        Modal::StatusPopup { .. } => shared_modal_footer_items(ModalFooterMode::StatusPopup),
        Modal::OpPicker { state } => shared_modal_footer_items(op_picker_modal_footer_mode(
            state.stage,
            state.naming_stage_input().is_some(),
            true,
        )),
        Modal::RolePicker { .. }
        | Modal::RoleOverridePicker { .. }
        | Modal::AuthRolePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::FilteredPicker {
                include_refresh: false,
            })
        }
        Modal::Confirm { .. } => shared_modal_footer_items(ModalFooterMode::YesNo),
    }
}

pub(crate) fn settings_mounts_modal_footer_items(
    modal: &GlobalMountModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        GlobalMountModal::Text { .. } => {
            shared_modal_footer_items(ModalFooterMode::ConfirmDismiss)
        }
        GlobalMountModal::FileBrowser { state } => state.footer_items(),
        GlobalMountModal::MountDstChoice { .. } => {
            shared_modal_footer_items(ModalFooterMode::MountDestination)
        }
        GlobalMountModal::ScopePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::SegmentedChoice)
        }
        GlobalMountModal::RolePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::FilteredPicker {
                include_refresh: false,
            })
        }
        GlobalMountModal::Confirm { .. } => shared_modal_footer_items(ModalFooterMode::YesNo),
        GlobalMountModal::PreviewSave { state } => {
            shared_modal_footer_items(ModalFooterMode::ConfirmSave {
                scrollable: !state.lines.is_empty(),
            })
        }
    }
}

pub(crate) fn settings_env_modal_footer_items(
    modal: &SettingsEnvModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        SettingsEnvModal::Text { .. } => {
            shared_modal_footer_items(ModalFooterMode::ConfirmDismiss)
        }
        SettingsEnvModal::SourcePicker { .. } | SettingsEnvModal::ScopePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::SegmentedChoice)
        }
        SettingsEnvModal::OpPicker { .. } | SettingsEnvModal::RolePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::FilteredPicker {
                include_refresh: false,
            })
        }
        SettingsEnvModal::Confirm { .. } => shared_modal_footer_items(ModalFooterMode::YesNo),
    }
}

pub(crate) fn settings_auth_modal_footer_items(auth: &SettingsAuthState) -> Vec<HintSpan<'static>> {
    let Some(modal) = auth.modal.as_ref() else {
        return Vec::new();
    };
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => shared_modal_footer_items(
            ModalFooterMode::AuthForm {
                focus: *focus,
                shows_credential_block: state.shows_credential_block(),
                can_generate_token:
                    crate::console::tui::input::global_mounts::settings_auth_can_generate_token(
                        auth,
                    ),
            },
        ),
        SettingsAuthModal::TextInput { .. } => {
            shared_modal_footer_items(ModalFooterMode::ConfirmDismiss)
        }
        SettingsAuthModal::SourcePicker { .. } => {
            shared_modal_footer_items(ModalFooterMode::SegmentedChoice)
        }
        SettingsAuthModal::OpPicker { state } => {
            shared_modal_footer_items(op_picker_modal_footer_mode(
                state.stage,
                state.naming_stage_input().is_some(),
                false,
            ))
        }
    }
}
