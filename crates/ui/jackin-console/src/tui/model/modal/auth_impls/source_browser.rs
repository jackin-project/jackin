// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ConsoleModal` auth source-folder browser.

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
>
    crate::tui::auth_config::ModalAuthSourceFolderBrowserOpen<
        FileBrowserTarget,
        FileBrowserState,
        AuthFormFocus,
    >
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
    AuthForm: crate::tui::auth_config::AuthFormSourceFolderState,
{
    fn open_auth_source_folder_browser<E>(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        source_folder_focus: AuthFormFocus,
        file_browser_target: FileBrowserTarget,
        make_browser: impl FnOnce() -> Result<FileBrowserState, E>,
    ) -> crate::tui::auth_config::AuthSourceFolderBrowserOpenResult<E> {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal.take()
        else {
            return crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::NotAvailable;
        };

        if !state.shows_auth_source_folder() {
            *modal = Some(Self::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            return crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::NotAvailable;
        }

        match make_browser() {
            Ok(browser) => {
                modal_parents.push(Self::AuthForm {
                    target,
                    state,
                    focus: source_folder_focus,
                    literal_buffer,
                });
                *modal = Some(Self::FileBrowser {
                    target: file_browser_target,
                    state: browser,
                });
                crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::Opened
            }
            Err(error) => {
                *modal = Some(Self::AuthForm {
                    target,
                    state,
                    focus,
                    literal_buffer,
                });
                crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::BrowserError(error)
            }
        }
    }
}
