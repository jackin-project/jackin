//! Debug-log naming helpers for the root console TUI.

use crate::console::{ConsoleStage, ConsoleState};

const fn modal_debug_name(modal: &crate::console::tui::state::Modal<'_>) -> &'static str {
    use crate::console::tui::state::Modal;
    match modal {
        Modal::TextInput { .. } => "TextInput",
        Modal::FileBrowser { .. } => "FileBrowser",
        Modal::MountDstChoice { .. } => "MountDstChoice",
        Modal::WorkdirPick { .. } => "WorkdirPick",
        Modal::Confirm { .. } => "Confirm",
        Modal::SaveDiscardCancel { .. } => "SaveDiscardCancel",
        Modal::GithubPicker { .. } => "GithubPicker",
        Modal::ConfirmSave { .. } => "ConfirmSave",
        Modal::ErrorPopup { .. } => "ErrorPopup",
        Modal::StatusPopup { .. } => "StatusPopup",
        Modal::ContainerInfo { .. } => "ContainerInfo",
        Modal::OpPicker { .. } => "OpPicker",
        Modal::RolePicker { .. } => "RolePicker",
        Modal::RoleOverridePicker { .. } => "RoleOverridePicker",
        Modal::SourcePicker { .. } => "SourcePicker",
        Modal::AuthSourcePicker { .. } => "AuthSourcePicker",
        Modal::ScopePicker { .. } => "ScopePicker",
        Modal::AuthForm { .. } => "AuthForm",
        Modal::AuthRolePicker { .. } => "AuthRolePicker",
    }
}

pub(crate) fn console_location_debug(console_state: &ConsoleState) -> String {
    if console_state.quit_confirm.is_some() {
        return "quit-confirm".into();
    }

    let ConsoleStage::Manager(ms) = &console_state.stage;
    let list_modal = ms.list_modal.as_ref().map_or_else(String::new, |modal| {
        format!(" list_modal={}", modal_debug_name(modal))
    });
    let location = match &ms.stage {
        crate::console::tui::state::ManagerStage::List => "list".to_string(),
        crate::console::tui::state::ManagerStage::Editor(editor) => {
            let modal = editor.modal.as_ref().map_or("none", modal_debug_name);
            format!(
                "editor mode={:?} tab={:?} field={:?} modal={modal}",
                editor.mode, editor.active_tab, editor.active_field
            )
        }
        crate::console::tui::state::ManagerStage::CreatePrelude(prelude) => {
            let modal = prelude.modal.as_ref().map_or("none", modal_debug_name);
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
                .map_or("none", |modal| match modal {
                    crate::console::tui::state::GlobalMountModal::Text { .. } => "text-input",
                    crate::console::tui::state::GlobalMountModal::FileBrowser { .. } => {
                        "file-browser"
                    }
                    crate::console::tui::state::GlobalMountModal::MountDstChoice { .. } => {
                        "mount-dst-choice"
                    }
                    crate::console::tui::state::GlobalMountModal::ScopePicker { .. } => {
                        "scope-picker"
                    }
                    crate::console::tui::state::GlobalMountModal::RolePicker { .. } => {
                        "role-picker"
                    }
                    crate::console::tui::state::GlobalMountModal::Confirm {
                        action, ..
                    } => match action {
                        crate::console::tui::state::GlobalMountConfirm::Remove => {
                            "confirm-remove"
                        }
                        crate::console::tui::state::GlobalMountConfirm::Save => "confirm-save",
                        crate::console::tui::state::GlobalMountConfirm::Sensitive => {
                            "confirm-sensitive"
                        }
                        crate::console::tui::state::GlobalMountConfirm::Discard => {
                            "confirm-discard"
                        }
                    },
                    crate::console::tui::state::GlobalMountModal::PreviewSave { .. } => {
                        "preview-save"
                    }
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
    use crossterm::event::{KeyCode, KeyModifiers};
    let has_command_modifier = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    let code = match key.code {
        KeyCode::Char(_) if super::consumes_letter_input(state) && !has_command_modifier => {
            "Char(<redacted>)".to_string()
        }
        KeyCode::Char(ch) => format!("Char({})", ch.escape_default()),
        other => format!("{other:?}"),
    };
    if key.modifiers.is_empty() {
        code
    } else {
        format!("{:?}+{code}", key.modifiers)
    }
}
