// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `debug`.
use super::{
    ConsoleLocationDebug, ConsoleStageDebug, ModalDebugKind, SettingsMountModalDebugKind,
    console_location_debug_name, key_debug_name_for_input, modal_debug_name,
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

#[test]
fn console_location_debug_formats_editor_without_values() {
    let location = ConsoleLocationDebug {
        quit_confirm: false,
        stage: ConsoleStageDebug::Editor {
            mode: "Create".to_owned(),
            tab: "Auth".to_owned(),
            field: "EnvValue".to_owned(),
            modal: Some(ModalDebugKind::TextInput),
        },
        list_modal: Some(ModalDebugKind::ErrorPopup),
    };

    assert_eq!(
        console_location_debug_name(&location),
        "editor mode=Create tab=Auth field=EnvValue modal=TextInput list_modal=ErrorPopup"
    );
}

#[test]
fn console_location_debug_quit_confirm_takes_precedence() {
    let location = ConsoleLocationDebug {
        quit_confirm: true,
        stage: ConsoleStageDebug::List,
        list_modal: Some(ModalDebugKind::ErrorPopup),
    };

    assert_eq!(console_location_debug_name(&location), "quit-confirm");
}

#[test]
fn console_location_debug_formats_settings_mount_modal() {
    let location = ConsoleLocationDebug {
        quit_confirm: false,
        stage: ConsoleStageDebug::Settings {
            tab: "Mounts".to_owned(),
            selected: 2,
            modal: Some(SettingsMountModalDebugKind::ConfirmSensitive),
        },
        list_modal: None,
    };

    assert_eq!(
        console_location_debug_name(&location),
        "settings tab=Mounts selected=2 modal=confirm-sensitive"
    );
}
