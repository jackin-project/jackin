// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! File-browser git-URL resolution and polling.

use crate::tui::state::{ManagerStage, ManagerState, Modal, SettingsModal};

pub fn execute_file_browser_git_url_resolution(
    state: &mut ManagerState<'_>,
    path: &std::path::Path,
) -> bool {
    if let Some(modal) = state.list_modal.as_mut()
        && attach_modal_file_browser_git_url(modal, path.to_owned())
    {
        return true;
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = editor.modal.as_mut()
                && attach_modal_file_browser_git_url(modal, path.to_owned())
            {
                return true;
            }
            for modal in &mut editor.modal_parents {
                if attach_modal_file_browser_git_url(modal, path.to_owned()) {
                    return true;
                }
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = prelude.modal.as_mut()
                && attach_modal_file_browser_git_url(modal, path.to_owned())
            {
                return true;
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modals.current_mut()
                && attach_global_mount_file_browser_git_url(modal, path.to_owned())
            {
                return true;
            }
            for modal in settings.mounts.modals.parents_mut() {
                if attach_global_mount_file_browser_git_url(modal, path.to_owned()) {
                    return true;
                }
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    false
}

pub(crate) fn attach_modal_file_browser_git_url(
    modal: &mut Modal<'_>,
    path: std::path::PathBuf,
) -> bool {
    match modal {
        Modal::FileBrowser { state, .. } => {
            crate::services::file_browser::request_git_url_resolution(state, path);
            true
        }
        _ => false,
    }
}

pub(crate) fn attach_global_mount_file_browser_git_url(
    modal: &mut SettingsModal<'_>,
    path: std::path::PathBuf,
) -> bool {
    match modal {
        SettingsModal::MountFileBrowser { state } => {
            crate::services::file_browser::request_git_url_resolution(state, path);
            true
        }
        _ => false,
    }
}

pub fn poll_file_browser_git_urls(state: &mut ManagerState<'_>) -> bool {
    let mut dirty = false;
    if let Some(modal) = state.list_modal.as_mut() {
        dirty |= poll_modal_file_browser_git_url(modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = editor.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
            for modal in &mut editor.modal_parents {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = prelude.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modals.current_mut() {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
            for modal in settings.mounts.modals.parents_mut() {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    dirty
}

pub(crate) fn poll_modal_file_browser_git_url(modal: &mut Modal<'_>) -> bool {
    match modal {
        Modal::FileBrowser { state, .. } => state.poll_git_url_resolution(),
        _ => false,
    }
}

pub(crate) fn poll_global_mount_file_browser_git_url(modal: &mut SettingsModal<'_>) -> bool {
    match modal {
        SettingsModal::MountFileBrowser { state } => state.poll_git_url_resolution(),
        _ => false,
    }
}
