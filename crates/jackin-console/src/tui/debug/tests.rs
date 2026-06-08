//! Tests for `debug`.
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
