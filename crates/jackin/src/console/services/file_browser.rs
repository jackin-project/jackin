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

#[allow(dead_code)] // Used by B.3 auth source-folder picker (not yet wired to UI)
/// Open a file browser from `$HOME` with dotfile directories visible.
/// Used for the auth source-folder picker (credential dirs are dotfiles).
pub(in crate::console) fn from_home_with_hidden() -> anyhow::Result<FileBrowserState> {
    let mut state = FileBrowserState::from_listing(
        jackin_console::services::file_browser::listing_from_home_with_hidden()?,
    );
    state.show_hidden = true;
    Ok(state)
}

pub(in crate::console) fn clamp_to_cwd(state: &mut FileBrowserState, cwd: &std::path::Path) {
    let listing = jackin_console::services::file_browser::clamped_listing_with_hidden(
        &state.root,
        cwd,
        state.show_hidden,
    );
    state.apply_listing(listing);
}

pub(in crate::console) fn apply_file_browser_outcome(
    state: &mut FileBrowserState,
    outcome: FileBrowserOutcome<PathBuf>,
) -> FileBrowserOutcome<PathBuf> {
    match outcome {
        FileBrowserOutcome::NavigateTo(path) => {
            let listing = jackin_console::services::file_browser::clamped_listing_with_hidden(
                &state.root,
                &path,
                state.show_hidden,
            );
            state.apply_listing(listing);
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::NavigateUp => {
            if let Some(listing) =
                jackin_console::services::file_browser::parent_listing_with_hidden(
                    &state.root,
                    state.cwd(),
                    state.show_hidden,
                )
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
