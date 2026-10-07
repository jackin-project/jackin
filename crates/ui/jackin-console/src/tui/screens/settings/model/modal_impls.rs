// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings modal impls.

use super::{GlobalMountConfirm, SettingsModal, footer_items_for_mode};

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalFileBrowserFooterState,
    ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_overlay::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalOpPickerState, ModalRolePickerState, exact_dialog_size, fixed_dialog_size,
    modal_overlay_rect,
};

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
>
    SettingsModal<
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
    OpPickerState: ModalOpPickerState,
    RolePickerState: ModalRolePickerState,
    ConfirmState: ModalConfirmState,
    ConfirmSaveState: ModalConfirmSaveState,
    AuthForm: ModalAuthFormState,
{
    #[must_use]
    pub fn debug_kind(&self) -> crate::tui::debug::SettingsMountModalDebugKind {
        use crate::tui::debug::SettingsMountModalDebugKind;
        match self {
            Self::MountText { .. } => SettingsMountModalDebugKind::TextInput,
            Self::MountFileBrowser { .. } => SettingsMountModalDebugKind::FileBrowser,
            Self::MountDstChoice { .. } => SettingsMountModalDebugKind::MountDstChoice,
            Self::MountScopePicker { .. } => SettingsMountModalDebugKind::ScopePicker,
            Self::MountRolePicker { .. } => SettingsMountModalDebugKind::RolePicker,
            Self::MountConfirm { action, .. } => match action {
                GlobalMountConfirm::Remove => SettingsMountModalDebugKind::ConfirmRemove,
                GlobalMountConfirm::Save => SettingsMountModalDebugKind::ConfirmSave,
                GlobalMountConfirm::Sensitive => SettingsMountModalDebugKind::ConfirmSensitive,
                GlobalMountConfirm::Discard => SettingsMountModalDebugKind::ConfirmDiscard,
            },
            Self::MountPreviewSave { .. } => SettingsMountModalDebugKind::PreviewSave,
            _ => unreachable!("mount debug facts were requested for a non-mount settings modal"),
        }
    }

    /// Overlay kind for the stack entry: confirm-class blockers are alert
    /// dialogs; behavior comes from the explicit policy, not the preset.
    #[must_use]
    pub const fn overlay_kind(&self) -> termrock::interaction::OverlayKind {
        match self {
            Self::MountConfirm { .. } | Self::EnvConfirm { .. } => {
                termrock::interaction::OverlayKind::AlertDialog
            }
            _ => termrock::interaction::OverlayKind::Dialog,
        }
    }

    /// Esc classification: file-browser and op-picker components spend Esc
    /// on internal back-navigation first; every other variant cancels
    /// outright.
    #[must_use]
    pub const fn dismiss_policy(&self) -> termrock::interaction::DismissPolicy {
        let escape = match self {
            Self::MountFileBrowser { .. }
            | Self::AuthSourceFolderPicker { .. }
            | Self::EnvOpPicker { .. }
            | Self::AuthOpPicker { .. } => termrock::interaction::DismissAction::Bubble,
            _ => termrock::interaction::DismissAction::Dismiss,
        };
        crate::tui::components::modal_overlay::console_modal_dismiss_policy(escape)
    }

    /// Preferred overlay size: the retired `ModalRectMode` numbers, kept
    /// byte-identical per variant.
    #[must_use]
    pub fn overlay_size(&self, outer: ratatui::layout::Rect) -> termrock::interaction::OverlaySize {
        match self {
            Self::MountText { .. } | Self::EnvText { .. } | Self::AuthTextInput { .. } => {
                fixed_dialog_size(outer, 60, 5)
            }
            Self::MountFileBrowser { .. } | Self::AuthSourceFolderPicker { .. } => {
                fixed_dialog_size(outer, 70, 22)
            }
            Self::MountDstChoice { .. } => exact_dialog_size(outer, 80, 8),
            Self::MountScopePicker { .. } | Self::EnvScopePicker { .. } => {
                fixed_dialog_size(outer, 50, 5)
            }
            Self::MountRolePicker { state } | Self::EnvRolePicker { state } => {
                let rows = (state.filtered_len() as u16).saturating_add(6).min(15);
                fixed_dialog_size(outer, 50, rows)
            }
            Self::EnvSourcePicker { .. } | Self::AuthSourcePicker { .. } => {
                fixed_dialog_size(outer, 50, 5)
            }
            Self::EnvOpPicker { state, .. } | Self::AuthOpPicker { state }
                if state.has_naming_stage_input() =>
            {
                fixed_dialog_size(outer, 60, 5)
            }
            Self::EnvOpPicker { .. } | Self::AuthOpPicker { .. } => {
                fixed_dialog_size(outer, 80, 22)
            }
            Self::MountConfirm { state, .. } | Self::EnvConfirm { state, .. } => {
                fixed_dialog_size(outer, state.width_pct(), state.required_height())
            }
            Self::MountPreviewSave { state } => {
                fixed_dialog_size(outer, 80, state.required_height().min(outer.height))
            }
            Self::AuthForm { state, .. } => fixed_dialog_size(outer, 80, state.required_height()),
        }
    }

    /// Stack-resolved modal rect.
    #[must_use]
    pub fn rect(&self, outer: ratatui::layout::Rect) -> ratatui::layout::Rect {
        modal_overlay_rect(
            outer,
            self.overlay_kind(),
            self.overlay_size(outer),
            self.dismiss_policy().escape,
        )
    }

    pub fn prepare_for_render(&mut self, outer: ratatui::layout::Rect)
    where
        ConfirmSaveState: ModalConfirmSavePrepareState,
    {
        let modal_area = self.rect(outer);
        if let Self::MountPreviewSave { state } = self {
            state.prepare_for_render(modal_area);
        }
    }

    #[must_use]
    pub fn env_footer_items(&self) -> Vec<termrock::widgets::HintSpan<'static>>
    where
        OpPickerState: ModalOpPickerFooterState,
    {
        match self {
            Self::EnvText { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::EnvSourcePicker { .. } | Self::EnvScopePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::EnvOpPicker { state, .. } => footer_items_for_mode(state.footer_mode(false)),
            Self::EnvRolePicker { .. } => footer_items_for_mode(ModalFooterMode::FilteredPicker {
                include_refresh: false,
                include_collapse: false,
            }),
            Self::EnvConfirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn mounts_footer_items(&self) -> Vec<termrock::widgets::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
    {
        match self {
            Self::MountText { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::MountFileBrowser { state } => state.footer_items(),
            Self::MountDstChoice { .. } => footer_items_for_mode(ModalFooterMode::MountDestination),
            Self::MountScopePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::MountRolePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::FilteredPicker {
                    include_refresh: false,
                    include_collapse: false,
                })
            }
            Self::MountConfirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
            Self::MountPreviewSave { state } => footer_items_for_mode(state.footer_mode()),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub fn auth_footer_items(
        &self,
        can_generate_token: bool,
    ) -> Vec<termrock::widgets::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
    {
        match self {
            Self::AuthForm { state, focus, .. } => {
                footer_items_for_mode(state.footer_mode(*focus, can_generate_token))
            }
            Self::AuthTextInput { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::AuthSourcePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::SegmentedChoice)
            }
            Self::AuthSourceFolderPicker { state } => state.footer_items(),
            Self::AuthOpPicker { state } => footer_items_for_mode(state.footer_mode(false)),
            _ => Vec::new(),
        }
    }

    #[must_use]
    pub const fn letter_input_kind(&self) -> Option<crate::tui::run::LetterInputModalKind> {
        crate::tui::run::letter_input_modal_kind(
            matches!(self, Self::MountText { .. }),
            false,
            true,
        )
    }
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
>
    SettingsModal<
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
{
    #[must_use]
    pub const fn mount_scroll_target(&self) -> crate::tui::update::SettingsModalScrollTarget {
        use crate::tui::update::SettingsModalScrollTarget;
        match self {
            Self::MountRolePicker { .. } => SettingsModalScrollTarget::MountRolePicker,
            _ => SettingsModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn env_scroll_target(&self) -> crate::tui::update::SettingsModalScrollTarget {
        use crate::tui::update::SettingsModalScrollTarget;
        match self {
            Self::EnvOpPicker { .. } => SettingsModalScrollTarget::EnvOpPicker,
            Self::EnvRolePicker { .. } => SettingsModalScrollTarget::EnvRolePicker,
            _ => SettingsModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn auth_scroll_target(&self) -> crate::tui::update::SettingsModalScrollTarget {
        use crate::tui::update::SettingsModalScrollTarget;
        match self {
            Self::AuthOpPicker { .. } => SettingsModalScrollTarget::AuthOpPicker,
            _ => SettingsModalScrollTarget::None,
        }
    }
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
> crate::tui::debug::ConsoleSettingsMountModalDebugKind
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
    OpPickerState: ModalOpPickerState,
    RolePickerState: ModalRolePickerState,
    ConfirmState: ModalConfirmState,
    ConfirmSaveState: ModalConfirmSaveState,
    AuthForm: ModalAuthFormState,
{
    fn settings_mount_modal_debug_kind(&self) -> crate::tui::debug::SettingsMountModalDebugKind {
        self.debug_kind()
    }
}
