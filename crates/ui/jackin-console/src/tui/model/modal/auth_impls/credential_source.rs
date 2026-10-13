// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ConsoleModal` credential source picker and plain-text apply.

use super::super::ConsoleModal;
use crate::tui::components::modal_overlay::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRolePickerState,
};
use crate::tui::debug::ConsoleModalDebugKind;
use crate::tui::screens::editor::model::{
    EditorErrorPopupModal, EditorRoleOverridePickerModal, EditorSaveDiscardModal,
    EditorStatusPopupModal,
};
use std::path::PathBuf;

impl<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
> crate::tui::auth_config::ModalAuthSourcePickerOpen<SourcePickerState>
    for ConsoleModal<
        TextInputTarget,
        TextInputState,
        FileBrowserTarget,
        FileBrowserState,
        MountDstChoiceState,
        WorkdirPickState,
        ConfirmTarget,
        ConfirmState,
        SaveDiscardState,
        GithubPickerState,
        ConfirmSaveState,
        ErrorPopupState,
        ContainerInfoState,
        StatusPopupState,
        OpPickerState,
        RolePickerState,
        SourcePickerState,
        ScopePickerState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
        SecretsScopeTag,
    >
where
    AuthForm: crate::tui::auth_config::AuthFormCredentialSourceState,
{
    fn open_auth_source_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        make_source_picker: impl FnOnce(&'static str) -> SourcePickerState,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal.take()
        else {
            return false;
        };

        let Some(env_var) = state.required_credential_env_var() else {
            *modal = Some(Self::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            return false;
        };

        modal_parents.push(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        *modal = Some(Self::AuthSourcePicker {
            state: make_source_picker(env_var),
        });
        true
    }
}

impl<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
> crate::tui::auth_config::ModalAuthFormCredentialApply<AuthFormFocus>
    for ConsoleModal<
        TextInputTarget,
        TextInputState,
        FileBrowserTarget,
        FileBrowserState,
        MountDstChoiceState,
        WorkdirPickState,
        ConfirmTarget,
        ConfirmState,
        SaveDiscardState,
        GithubPickerState,
        ConfirmSaveState,
        ErrorPopupState,
        ContainerInfoState,
        StatusPopupState,
        OpPickerState,
        RolePickerState,
        SourcePickerState,
        ScopePickerState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
        SecretsScopeTag,
    >
where
    AuthForm: crate::tui::auth_config::AuthFormCredentialEdit,
{
    fn apply_auth_plain_text(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: &str,
    ) -> bool {
        let Some(Self::AuthForm {
            target, mut state, ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        state.set_auth_literal(value.to_owned());
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus: save_focus,
            literal_buffer: value.to_owned(),
        });
        true
    }

    fn apply_auth_source_folder(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: PathBuf,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            mut state,
            literal_buffer,
            ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        state.set_auth_source_folder(value);
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus: save_focus,
            literal_buffer,
        });
        true
    }

    fn restore_auth_form_modal(modal: &mut Option<Self>, modal_parents: &mut Vec<Self>) -> bool {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal_parents.pop()
        else {
            return false;
        };
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        true
    }
}
