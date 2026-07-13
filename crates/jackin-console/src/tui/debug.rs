// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLocationDebug {
    pub quit_confirm: bool,
    pub stage: ConsoleStageDebug,
    pub list_modal: Option<ModalDebugKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleStageDebug {
    List,
    Editor {
        mode: String,
        tab: String,
        field: String,
        modal: Option<ModalDebugKind>,
    },
    CreatePrelude {
        step: String,
        modal: Option<ModalDebugKind>,
    },
    ConfirmDelete,
    ConfirmInstancePurge,
    Settings {
        tab: String,
        selected: usize,
        modal: Option<SettingsMountModalDebugKind>,
    },
}

pub trait ConsoleModalDebugKind {
    fn modal_debug_kind(&self) -> ModalDebugKind;
}

pub trait ConsoleSettingsMountModalDebugKind {
    fn settings_mount_modal_debug_kind(&self) -> SettingsMountModalDebugKind;
}

pub trait ConsoleEditorDebugFacts {
    fn editor_stage_debug(&self) -> ConsoleStageDebug;
}

pub trait ConsoleCreatePreludeDebugFacts {
    fn create_prelude_stage_debug(&self) -> ConsoleStageDebug;
}

pub trait ConsoleSettingsDebugFacts {
    fn settings_stage_debug(&self) -> ConsoleStageDebug;
}

#[must_use]
pub fn console_location_debug_name(location: &ConsoleLocationDebug) -> String {
    if location.quit_confirm {
        return "quit-confirm".to_owned();
    }

    let mut name = match &location.stage {
        ConsoleStageDebug::List => "list".to_owned(),
        ConsoleStageDebug::Editor {
            mode,
            tab,
            field,
            modal,
        } => format!(
            "editor mode={mode} tab={tab} field={field} modal={}",
            modal.map_or("none", modal_debug_name)
        ),
        ConsoleStageDebug::CreatePrelude { step, modal } => {
            format!(
                "create-prelude step={step} modal={}",
                modal.map_or("none", modal_debug_name)
            )
        }
        ConsoleStageDebug::ConfirmDelete => "confirm-delete".to_owned(),
        ConsoleStageDebug::ConfirmInstancePurge => "confirm-instance-purge".to_owned(),
        ConsoleStageDebug::Settings {
            tab,
            selected,
            modal,
        } => format!(
            "settings tab={tab} selected={selected} modal={}",
            modal.map_or("none", settings_mount_modal_debug_name)
        ),
    };

    if let Some(modal) = location.list_modal {
        name.push_str(" list_modal=");
        name.push_str(modal_debug_name(modal));
    }
    name
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
            "Char(<redacted>)".to_owned()
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

pub fn console_location_debug(console_state: &crate::tui::console::ConsoleState) -> String {
    let crate::tui::console::ConsoleStage::Manager(ms) = &console_state.stage;
    console_location_debug_name(&ConsoleLocationDebug {
        quit_confirm: console_state.quit_confirm_open(),
        stage: ms.stage.debug_stage(),
        list_modal: ms
            .list_modal
            .as_ref()
            .map(ConsoleModalDebugKind::modal_debug_kind),
    })
}

#[cfg(test)]
mod tests;
