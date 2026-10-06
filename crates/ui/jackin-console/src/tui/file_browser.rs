// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! File-browser effect executors: open, apply outcome, resolve git URL, poll.
//!
//! `execute_editor_file_browser_outcome` accepts an injectable
//! `auth_source_folder_validator` so the root binary can supply the macOS
//! `security`-subprocess check without creating a runtime dep here.
mod commit;
mod git_urls;
mod listing;
mod open;
mod surfaces;
pub(crate) use commit::{FileBrowserListingRequestKind, start_file_browser_listing_for_navigation};
pub use commit::{apply_file_browser_commit_result, start_file_browser_commit_validation};
pub use git_urls::{execute_file_browser_git_url_resolution, poll_file_browser_git_urls};
pub use listing::{
    apply_file_browser_listing_result, execute_file_browser_outcome,
    execute_file_browser_outcome_or_start_listing,
};
pub use open::{
    AuthSourceFolderValidator, FileBrowserCommitResult, start_create_prelude_file_browser_open,
    start_create_prelude_file_browser_reopen, start_editor_add_mount_file_browser_open,
    start_editor_auth_source_folder_browser_open, start_global_mount_file_browser_open,
    start_settings_auth_source_folder_browser_open,
};

pub(crate) use surfaces::{
    active_file_browser_state_mut, apply_file_browser_listing, execute_editor_file_browser_outcome,
    execute_prelude_file_browser_outcome, execute_settings_file_browser_outcome,
};
