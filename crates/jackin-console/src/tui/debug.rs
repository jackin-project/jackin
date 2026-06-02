//! Debug-log helpers for console TUI event traces.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalDebugKind {
    TextInput,
    FileBrowser,
    MountDstChoice,
    WorkdirPick,
    Confirm,
    SaveDiscardCancel,
    GithubPicker,
    ConfirmSave,
    ErrorPopup,
    StatusPopup,
    ContainerInfo,
    OpPicker,
    RolePicker,
    RoleOverridePicker,
    SourcePicker,
    AuthSourcePicker,
    ScopePicker,
    AuthForm,
    AuthRolePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMountModalDebugKind {
    TextInput,
    FileBrowser,
    MountDstChoice,
    ScopePicker,
    RolePicker,
    ConfirmRemove,
    ConfirmSave,
    ConfirmSensitive,
    ConfirmDiscard,
    PreviewSave,
}

pub const fn modal_debug_name(kind: ModalDebugKind) -> &'static str {
    match kind {
        ModalDebugKind::TextInput => "TextInput",
        ModalDebugKind::FileBrowser => "FileBrowser",
        ModalDebugKind::MountDstChoice => "MountDstChoice",
        ModalDebugKind::WorkdirPick => "WorkdirPick",
        ModalDebugKind::Confirm => "Confirm",
        ModalDebugKind::SaveDiscardCancel => "SaveDiscardCancel",
        ModalDebugKind::GithubPicker => "GithubPicker",
        ModalDebugKind::ConfirmSave => "ConfirmSave",
        ModalDebugKind::ErrorPopup => "ErrorPopup",
        ModalDebugKind::StatusPopup => "StatusPopup",
        ModalDebugKind::ContainerInfo => "ContainerInfo",
        ModalDebugKind::OpPicker => "OpPicker",
        ModalDebugKind::RolePicker => "RolePicker",
        ModalDebugKind::RoleOverridePicker => "RoleOverridePicker",
        ModalDebugKind::SourcePicker => "SourcePicker",
        ModalDebugKind::AuthSourcePicker => "AuthSourcePicker",
        ModalDebugKind::ScopePicker => "ScopePicker",
        ModalDebugKind::AuthForm => "AuthForm",
        ModalDebugKind::AuthRolePicker => "AuthRolePicker",
    }
}

pub const fn settings_mount_modal_debug_name(kind: SettingsMountModalDebugKind) -> &'static str {
    match kind {
        SettingsMountModalDebugKind::TextInput => "text-input",
        SettingsMountModalDebugKind::FileBrowser => "file-browser",
        SettingsMountModalDebugKind::MountDstChoice => "mount-dst-choice",
        SettingsMountModalDebugKind::ScopePicker => "scope-picker",
        SettingsMountModalDebugKind::RolePicker => "role-picker",
        SettingsMountModalDebugKind::ConfirmRemove => "confirm-remove",
        SettingsMountModalDebugKind::ConfirmSave => "confirm-save",
        SettingsMountModalDebugKind::ConfirmSensitive => "confirm-sensitive",
        SettingsMountModalDebugKind::ConfirmDiscard => "confirm-discard",
        SettingsMountModalDebugKind::PreviewSave => "preview-save",
    }
}

/// Render a key event for debug logs. Redacts literal text input when the
/// focused widget owns character entry.
pub fn key_debug_name_for_input(
    key: crossterm::event::KeyEvent,
    consumes_letter_input: bool,
) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    let has_command_modifier = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    let code = match key.code {
        KeyCode::Char(_) if consumes_letter_input && !has_command_modifier => {
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

#[cfg(test)]
mod tests {
    use super::{
        ModalDebugKind, SettingsMountModalDebugKind, key_debug_name_for_input, modal_debug_name,
        settings_mount_modal_debug_name,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn key_debug_name_redacts_text_input() {
        assert_eq!(
            key_debug_name_for_input(key(KeyCode::Char('s')), true),
            "Char(<redacted>)"
        );
    }

    #[test]
    fn key_debug_name_keeps_command_modified_chars() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(
            key_debug_name_for_input(key, true),
            "KeyModifiers(CONTROL)+Char(s)"
        );
    }

    #[test]
    fn modal_debug_names_match_root_log_vocabulary() {
        assert_eq!(modal_debug_name(ModalDebugKind::TextInput), "TextInput");
        assert_eq!(
            modal_debug_name(ModalDebugKind::GithubPicker),
            "GithubPicker"
        );
        assert_eq!(
            modal_debug_name(ModalDebugKind::AuthRolePicker),
            "AuthRolePicker"
        );
    }

    #[test]
    fn settings_mount_modal_debug_names_match_root_log_vocabulary() {
        assert_eq!(
            settings_mount_modal_debug_name(SettingsMountModalDebugKind::TextInput),
            "text-input"
        );
        assert_eq!(
            settings_mount_modal_debug_name(SettingsMountModalDebugKind::ConfirmSensitive),
            "confirm-sensitive"
        );
        assert_eq!(
            settings_mount_modal_debug_name(SettingsMountModalDebugKind::PreviewSave),
            "preview-save"
        );
    }
}
