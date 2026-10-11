// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ConsoleModal` plain-source text input.

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
> crate::tui::auth_config::ModalAuthPlainSourceOpen<TextInputTarget, TextInputState, AuthFormFocus>
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
{
    fn open_auth_plain_source_text_input(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        text_input_target: TextInputTarget,
        make_text_input: impl FnOnce(String) -> TextInputState,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            state,
            literal_buffer,
            ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        modal_parents.push(Self::AuthForm {
            target,
            state,
            focus: credential_focus,
            literal_buffer: literal_buffer.clone(),
        });
        *modal = Some(Self::TextInput {
            target: text_input_target,
            state: make_text_input(literal_buffer),
        });
        true
    }
}
