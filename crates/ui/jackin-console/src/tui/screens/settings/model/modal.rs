// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings modal type.

use super::{
    GlobalMountConfirm, GlobalMountTextTarget, SettingsEnvConfirm, SettingsEnvOpPickerTarget,
    SettingsEnvScope, SettingsEnvTextTarget,
};

#[derive(Debug)]
pub enum SettingsModal<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> {
    MountText {
        target: GlobalMountTextTarget,
        state: Box<TextInputState>,
    },
    MountFileBrowser {
        state: Box<FileBrowserState>,
    },
    MountDstChoice {
        state: MountDstChoiceState,
    },
    MountScopePicker {
        state: ScopePickerState,
    },
    MountRolePicker {
        state: RolePickerState,
    },
    MountConfirm {
        action: GlobalMountConfirm,
        state: ConfirmState,
    },
    MountPreviewSave {
        state: ConfirmSaveState,
    },
    EnvText {
        target: SettingsEnvTextTarget,
        pending_value: Option<EnvValue>,
        state: Box<TextInputState>,
    },
    EnvSourcePicker {
        key: (SettingsEnvScope, String),
        state: SourcePickerState,
    },
    EnvOpPicker {
        target: SettingsEnvOpPickerTarget,
        state: Box<OpPickerState>,
    },
    EnvRolePicker {
        state: RolePickerState,
    },
    EnvScopePicker {
        state: ScopePickerState,
    },
    EnvConfirm {
        action: SettingsEnvConfirm,
        state: ConfirmState,
    },
    AuthTextInput {
        state: Box<TextInputState>,
    },
    AuthSourcePicker {
        state: SourcePickerState,
    },
    AuthOpPicker {
        state: Box<OpPickerState>,
    },
    AuthSourceFolderPicker {
        state: FileBrowserState,
    },
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        focus: AuthFormFocus,
        literal_buffer: String,
    },
}

impl<
    EnvValue,
    TextInputState,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
> crate::tui::model::ConsoleAnimationTick
    for SettingsModal<
        EnvValue,
        TextInputState,
        SourcePickerState,
        OpPickerState,
        FileBrowserState,
        MountDstChoiceState,
        RolePickerState,
        ScopePickerState,
        ConfirmState,
        ConfirmSaveState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
    >
where
    OpPickerState: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        match self {
            Self::EnvOpPicker { state, .. } | Self::AuthOpPicker { state } => {
                state.tick_active_animation()
            }
            Self::MountText { .. }
            | Self::MountFileBrowser { .. }
            | Self::MountDstChoice { .. }
            | Self::MountScopePicker { .. }
            | Self::MountRolePicker { .. }
            | Self::MountConfirm { .. }
            | Self::MountPreviewSave { .. }
            | Self::EnvText { .. }
            | Self::EnvSourcePicker { .. }
            | Self::EnvRolePicker { .. }
            | Self::EnvScopePicker { .. }
            | Self::EnvConfirm { .. }
            | Self::AuthTextInput { .. }
            | Self::AuthSourcePicker { .. }
            | Self::AuthSourceFolderPicker { .. }
            | Self::AuthForm { .. } => false,
        }
    }
}
