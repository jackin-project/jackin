//! Debug-log naming helpers for the root console TUI.

use crate::console::{ConsoleStage, ConsoleState};
use jackin_console::tui::debug::{
    modal_debug_name, settings_mount_modal_debug_name, ModalDebugKind,
    SettingsMountModalDebugKind,
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
    if console_state.quit_confirm.is_some() {
        return "quit-confirm".into();
    }

    let ConsoleStage::Manager(ms) = &console_state.stage;
    let list_modal = ms.list_modal.as_ref().map_or_else(String::new, |modal| {
        format!(" list_modal={}", modal_debug_name(modal_debug_kind(modal)))
    });
    let location = match &ms.stage {
        crate::console::tui::state::ManagerStage::List => "list".to_string(),
        crate::console::tui::state::ManagerStage::Editor(editor) => {
            let modal = editor
                .modal
                .as_ref()
                .map_or("none", |modal| modal_debug_name(modal_debug_kind(modal)));
            format!(
                "editor mode={:?} tab={:?} field={:?} modal={modal}",
                editor.mode, editor.active_tab, editor.active_field
            )
        }
        crate::console::tui::state::ManagerStage::CreatePrelude(prelude) => {
            let modal = prelude
                .modal
                .as_ref()
                .map_or("none", |modal| modal_debug_name(modal_debug_kind(modal)));
            format!("create-prelude step={:?} modal={modal}", prelude.step)
        }
        crate::console::tui::state::ManagerStage::ConfirmDelete { .. } => {
            "confirm-delete".to_string()
        }
        crate::console::tui::state::ManagerStage::ConfirmInstancePurge { .. } => {
            "confirm-instance-purge".to_string()
        }
        crate::console::tui::state::ManagerStage::Settings(settings) => {
            let modal = settings
                .mounts
                .modal
                .as_ref()
                .map_or("none", |modal| {
                    settings_mount_modal_debug_name(settings_mount_modal_debug_kind(modal))
                });
            format!(
                "settings tab={:?} selected={} modal={modal}",
                settings.active_tab, settings.mounts.selected
            )
        }
    };
    format!("{location}{list_modal}")
}

/// Render a key event for the `--debug` log. Redacts the literal
/// character when the focused widget is consuming text input.
pub(crate) fn key_debug_name(
    state: &ConsoleState,
    key: crossterm::event::KeyEvent,
) -> String {
    jackin_console::tui::debug::key_debug_name_for_input(key, super::consumes_letter_input(state))
}
