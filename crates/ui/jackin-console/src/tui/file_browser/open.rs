// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! File-browser open starters per surface.

use crate::services::file_browser::{FileBrowserListingRequest, FileBrowserOpenTarget};

use crate::tui::effect::FileBrowserEffectContext;

use crate::tui::state::{ManagerStage, ManagerState};

pub type AuthSourceFolderValidator =
    fn(Option<crate::tui::auth::AuthKind>, &std::path::Path) -> Result<(), String>;

#[derive(Debug)]
pub enum FileBrowserCommitResult {
    Accepted {
        context: FileBrowserEffectContext,
        path: std::path::PathBuf,
    },
    Rejected {
        context: FileBrowserEffectContext,
        reason: String,
    },
}

pub fn start_global_mount_file_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Settings(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::GlobalMount,
            last_cwd: None,
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_editor_add_mount_file_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Editor(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::EditorAddMount,
            last_cwd: None,
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_editor_auth_source_folder_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Editor(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::EditorAuthSourceFolder,
            last_cwd: None,
            show_hidden: true,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_create_prelude_file_browser_open(state: &mut ManagerState<'_>) -> bool {
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::CreatePrelude,
            last_cwd: None,
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_settings_auth_source_folder_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Settings(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::SettingsAuthSourceFolder,
            last_cwd: None,
            show_hidden: true,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_create_prelude_file_browser_reopen(state: &mut ManagerState<'_>) -> bool {
    let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
        return false;
    };
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::CreatePrelude,
            last_cwd: prelude.last_browser_cwd.clone(),
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}
