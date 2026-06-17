//! Debug-log naming helpers for the root console TUI.

use crate::console::{ConsoleStage, ConsoleState};
use jackin_console::tui::debug::{
    ConsoleLocationDebug, ConsoleStageDebug, ModalDebugKind, SettingsMountModalDebugKind,
    console_location_debug_name,
};

const fn modal_debug_kind(modal: &crate::console::tui::state::Modal<'_>) -> ModalDebugKind {
    use crate::console::tui::state::Modal;
    match modal {
        Modal::TextInput { .. } => ModalDebugKind::TextInput,
        Modal::FileBrowser { .. } => ModalDebugKind::FileBrowser,
        Modal::MountDstChoice { .. } => ModalDebugKind::MountDstChoice,
        Modal::WorkdirPick { .. } => ModalDebugKind::WorkdirPick,
        Modal::Confirm { .. } => ModalDebugKind::Confirm,
        Modal::SaveDiscardCancel { .. } => ModalDebugKind::SaveDiscardCancel,
        Modal::GithubPicker { .. } => ModalDebugKind::GithubPicker,
        Modal::ConfirmSave { .. } => ModalDebugKind::ConfirmSave,
        Modal::ErrorPopup { .. } => ModalDebugKind::ErrorPopup,
        Modal::StatusPopup { .. } => ModalDebugKind::StatusPopup,
        Modal::ContainerInfo { .. } => ModalDebugKind::ContainerInfo,
        Modal::OpPicker { .. } => ModalDebugKind::OpPicker,
        Modal::RolePicker { .. } => ModalDebugKind::RolePicker,
        Modal::RoleOverridePicker { .. } => ModalDebugKind::RoleOverridePicker,
        Modal::SourcePicker { .. } => ModalDebugKind::SourcePicker,
        Modal::AuthSourcePicker { .. } => ModalDebugKind::AuthSourcePicker,
        Modal::ScopePicker { .. } => ModalDebugKind::ScopePicker,
        Modal::AuthForm { .. } => ModalDebugKind::AuthForm,
        Modal::AuthRolePicker { .. } => ModalDebugKind::AuthRolePicker,
    }
}

const fn settings_mount_modal_debug_kind(
    modal: &crate::console::tui::state::GlobalMountModal<'_>,
) -> SettingsMountModalDebugKind {
    use crate::console::tui::state::{GlobalMountConfirm, GlobalMountModal};
    match modal {
        GlobalMountModal::Text { .. } => SettingsMountModalDebugKind::TextInput,
        GlobalMountModal::FileBrowser { .. } => SettingsMountModalDebugKind::FileBrowser,
        GlobalMountModal::MountDstChoice { .. } => SettingsMountModalDebugKind::MountDstChoice,
        GlobalMountModal::ScopePicker { .. } => SettingsMountModalDebugKind::ScopePicker,
        GlobalMountModal::RolePicker { .. } => SettingsMountModalDebugKind::RolePicker,
        GlobalMountModal::Confirm { action, .. } => match action {
            GlobalMountConfirm::Remove => SettingsMountModalDebugKind::ConfirmRemove,
            GlobalMountConfirm::Save => SettingsMountModalDebugKind::ConfirmSave,
            GlobalMountConfirm::Sensitive => SettingsMountModalDebugKind::ConfirmSensitive,
            GlobalMountConfirm::Discard => SettingsMountModalDebugKind::ConfirmDiscard,
        },
        GlobalMountModal::PreviewSave { .. } => SettingsMountModalDebugKind::PreviewSave,
    }
}

pub(crate) fn console_location_debug(console_state: &ConsoleState) -> String {
    let ConsoleStage::Manager(ms) = &console_state.stage;
    let stage = match &ms.stage {
        crate::console::tui::state::ManagerStage::List => ConsoleStageDebug::List,
        crate::console::tui::state::ManagerStage::Editor(editor) => ConsoleStageDebug::Editor {
            mode: format!("{:?}", editor.mode),
            tab: format!("{:?}", editor.active_tab),
            field: format!("{:?}", editor.active_field),
            modal: editor.modal.as_ref().map(modal_debug_kind),
        },
        crate::console::tui::state::ManagerStage::CreatePrelude(prelude) => {
            ConsoleStageDebug::CreatePrelude {
                step: format!("{:?}", prelude.step),
                modal: prelude.modal.as_ref().map(modal_debug_kind),
            }
        }
        crate::console::tui::state::ManagerStage::ConfirmDelete { .. } => {
            ConsoleStageDebug::ConfirmDelete
        }
        crate::console::tui::state::ManagerStage::ConfirmInstancePurge { .. } => {
            ConsoleStageDebug::ConfirmInstancePurge
        }
        crate::console::tui::state::ManagerStage::Settings(settings) => {
            ConsoleStageDebug::Settings {
                tab: format!("{:?}", settings.active_tab),
                selected: settings.mounts.selected,
                modal: settings
                    .mounts
                    .modal
                    .as_ref()
                    .map(settings_mount_modal_debug_kind),
            }
        }
    };
    console_location_debug_name(&ConsoleLocationDebug {
        quit_confirm: console_state.quit_confirm.is_some(),
        stage,
        list_modal: ms.list_modal.as_ref().map(modal_debug_kind),
    })
}
