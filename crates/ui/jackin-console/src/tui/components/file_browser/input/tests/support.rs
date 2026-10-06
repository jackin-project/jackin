// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

pub(super) fn make_state_at(path: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        path.clone(),
        path,
    ))
}

pub(super) fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(root, cwd))
}

pub(super) fn apply_with_services(
    state: &mut FileBrowserState,
    outcome: FileBrowserOutcome<PathBuf>,
) -> FileBrowserOutcome<PathBuf> {
    match outcome {
        FileBrowserOutcome::NavigateTo(path) => {
            let listing = crate::services::file_browser::clamped_listing(&state.root, &path);
            state.apply_listing(listing);
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::NavigateUp => {
            if let Some(listing) =
                crate::services::file_browser::parent_listing(&state.root, state.cwd())
            {
                state.apply_listing(listing);
            }
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::RequestCommit(path) => {
            match crate::services::file_browser::validate_commit(&state.root, &path) {
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

pub(super) fn handle_with_services(
    state: &mut FileBrowserState,
    key: KeyEvent,
) -> FileBrowserOutcome<PathBuf> {
    let outcome = state.handle_key(key);
    apply_with_services(state, outcome)
}

pub(super) fn commit_with_services(
    state: &mut FileBrowserState,
    target: PathBuf,
) -> FileBrowserOutcome<PathBuf> {
    let outcome = FileBrowserState::commit_or_reject(target);
    apply_with_services(state, outcome)
}

pub(super) fn manufactured_modal_area() -> Rect {
    // Mirrors the shared file-browser modal rect for a term of 120x40:
    //   w = 120 * 70 / 100 = 84; h = 22.
    //   x = 0 + (120 - 84)/2 = 18; y = 0 + (40 - 22)/2 = 9.
    Rect {
        x: 18,
        y: 9,
        width: 84,
        height: 22,
    }
}
