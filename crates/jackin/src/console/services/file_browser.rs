//! File-browser service adapter for manager input.
//!
//! The `jackin-console` file-browser component emits typed navigation,
//! commit-validation, and git-url requests. This adapter executes those
//! non-TUI service calls for the root console.

use std::path::PathBuf;

use jackin_console::tui::components::file_browser::{FileBrowserOutcome, FileBrowserState};

pub(in crate::console) fn request_file_browser_git_url_resolution(
    state: &mut FileBrowserState,
    path: PathBuf,
) {
    let rx = jackin_console::services::file_browser::start_git_url_resolution(path);
    state.attach_git_url_resolution(rx);
}

pub(in crate::console) fn from_home() -> anyhow::Result<FileBrowserState> {
    Ok(FileBrowserState::from_listing(
        jackin_console::services::file_browser::listing_from_home()?,
    ))
}

pub(in crate::console) fn clamp_to_cwd(state: &mut FileBrowserState, cwd: &std::path::Path) {
    let listing = jackin_console::services::file_browser::clamped_listing(&state.root, cwd);
    state.apply_listing(listing);
}

pub(in crate::console) fn apply_file_browser_outcome(
    state: &mut FileBrowserState,
    outcome: FileBrowserOutcome<PathBuf>,
) -> FileBrowserOutcome<PathBuf> {
    match outcome {
        FileBrowserOutcome::NavigateTo(path) => {
            let listing =
                jackin_console::services::file_browser::clamped_listing(&state.root, &path);
            state.apply_listing(listing);
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::NavigateUp => {
            if let Some(listing) =
                jackin_console::services::file_browser::parent_listing(&state.root, state.cwd())
            {
                state.apply_listing(listing);
            }
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::RequestCommit(path) => {
            match jackin_console::services::file_browser::validate_commit(&state.root, &path) {
                Ok(path) => FileBrowserOutcome::Commit(path),
                Err(reason) => {
                    state.reject_commit(reason);
                    FileBrowserOutcome::Continue
                }
            }
        }
        other => other,
    }
}
