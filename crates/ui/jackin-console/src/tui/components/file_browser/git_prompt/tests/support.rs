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

pub(super) fn attach_git_url_resolution(state: &mut FileBrowserState, repo: PathBuf) {
    let rx = crate::tui::runtime::spawn_named_blocking_subscription(
        "jackin-file-browser-git-url-test",
        move || crate::services::file_browser::resolve_git_url(&repo),
    );
    state.attach_git_url_resolution(rx);
}

pub(super) fn wait_for_git_url_resolution(state: &mut FileBrowserState) {
    for _ in 0..50 {
        if state.poll_git_url_resolution() {
            return;
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "test polls an owned git-url worker thread"
        )]
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("git URL worker did not finish");
}

pub(super) fn seed_git_repo_with_origin(repo: &Path, remote: &str) {
    let git = repo.join(".git");
    std::fs::create_dir_all(&git).unwrap();
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        git.join("config"),
        format!("[remote \"origin\"]\n\turl = {remote}\n"),
    )
    .unwrap();
}
