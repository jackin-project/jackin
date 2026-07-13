// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `git_prompt`.
use super::*;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use std::path::Path;
use tempfile::tempdir;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(root, cwd))
}

fn apply_with_services(
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

fn handle_with_services(
    state: &mut FileBrowserState,
    key: KeyEvent,
) -> FileBrowserOutcome<PathBuf> {
    let outcome = state.handle_key(key);
    apply_with_services(state, outcome)
}

fn attach_git_url_resolution(state: &mut FileBrowserState, repo: PathBuf) {
    let rx = jackin_tui::runtime::spawn_named_blocking_subscription(
        "jackin-file-browser-git-url-test",
        move || crate::services::file_browser::resolve_git_url(&repo),
    );
    state.attach_git_url_resolution(rx);
}

fn wait_for_git_url_resolution(state: &mut FileBrowserState) {
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

// ── Git-repo prompt ───────────────────────────────────────────────

#[test]
fn enter_on_git_repo_opens_prompt() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    // Index 0 is `..`; advance to `repo`.
    handle_with_services(&mut state, key(KeyCode::Down));
    let outcome = handle_with_services(&mut state, key(KeyCode::Enter));
    match outcome {
        FileBrowserOutcome::ResolveGitUrl(path) => {
            assert_eq!(path.canonicalize().unwrap(), repo.canonicalize().unwrap());
        }
        other => panic!("expected ResolveGitUrl, got {other:?}"),
    }
    assert!(state.pending_git_prompt.is_some());
    assert_eq!(state.pending_git_focus, GitPromptFocus::MountHere);
}

/// Write a minimal `.git/HEAD` + `.git/config` with the given origin.
fn seed_git_repo_with_origin(repo: &Path, remote: &str) {
    let git = repo.join(".git");
    std::fs::create_dir_all(&git).unwrap();
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        git.join("config"),
        format!("[remote \"origin\"]\n\turl = {remote}\n"),
    )
    .unwrap();
}

#[test]
fn enter_on_git_repo_with_origin_sets_url() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    seed_git_repo_with_origin(&repo, "git@github.com:jackin-project/jackin.git");

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(state.pending_git_prompt.is_some());
    assert!(state.pending_git_url.is_none());
    attach_git_url_resolution(&mut state, repo);
    wait_for_git_url_resolution(&mut state);
    let url = state
        .pending_git_url
        .as_deref()
        .expect("GitHub origin must resolve");
    assert_eq!(url, "https://github.com/jackin-project/jackin/tree/main");
}

#[test]
fn resolve_git_url_returns_none_for_non_github_origin() {
    // Non-github remote (gitlab here) must yield `None` so the
    // `O open` keystroke is not advertised — the launcher only
    // speaks github web URLs.
    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("gitlab-repo");
    std::fs::create_dir_all(&repo).unwrap();
    seed_git_repo_with_origin(&repo, "git@gitlab.com:owner/repo.git");
    assert!(crate::services::file_browser::resolve_git_url(&repo).is_none());
}

#[test]
fn enter_on_git_repo_without_origin_leaves_url_none() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(state.pending_git_prompt.is_some());
    attach_git_url_resolution(&mut state, repo);
    wait_for_git_url_resolution(&mut state);
    assert!(state.pending_git_url.is_none());
}

#[test]
fn mount_here_commits_git_path() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert_eq!(state.pending_git_focus, GitPromptFocus::MountHere);
    let outcome = handle_with_services(&mut state, key(KeyCode::Enter));
    match outcome {
        FileBrowserOutcome::Commit(p) => {
            assert_eq!(p.canonicalize().unwrap(), repo.canonicalize().unwrap(),);
        }
        other => panic!("expected Commit, got {other:?}"),
    }
    assert!(state.pending_git_prompt.is_none());
}

#[test]
fn enter_in_navigates_into_subdir() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir(repo.join("sub")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter)); // open prompt
    handle_with_services(&mut state, key(KeyCode::Tab)); // MountHere -> EnterIn
    assert_eq!(state.pending_git_focus, GitPromptFocus::EnterIn);

    let outcome = handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    assert!(state.pending_git_prompt.is_none());
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        repo.canonicalize().unwrap(),
    );
}

#[test]
fn cancel_dismisses_prompt_via_focus() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent.clone());
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    handle_with_services(&mut state, key(KeyCode::Tab));
    handle_with_services(&mut state, key(KeyCode::Tab));
    assert_eq!(state.pending_git_focus, GitPromptFocus::Cancel);

    let outcome = handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    assert!(state.pending_git_prompt.is_none());
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        parent.canonicalize().unwrap(),
    );
}

#[test]
fn esc_dismisses_prompt_without_cancelling_browser() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(state.pending_git_prompt.is_some());
    let outcome = handle_with_services(&mut state, key(KeyCode::Esc));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    assert!(state.pending_git_prompt.is_none());
}

#[test]
fn m_shortcut_commits_repo_from_prompt() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    handle_with_services(&mut state, key(KeyCode::Tab));
    let outcome = handle_with_services(&mut state, key(KeyCode::Char('m')));
    match outcome {
        FileBrowserOutcome::Commit(p) => {
            assert_eq!(p.canonicalize().unwrap(), repo.canonicalize().unwrap(),);
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

// ── O hotkey (open URL in browser) ────────────────────────────────

/// With `pending_git_url == None`, `O` must be a silent no-op: the
/// prompt stays open, focus is unchanged, and no commit/cancel fires.
/// No URL-open outcome is emitted when the URL is absent.
#[test]
fn o_shortcut_without_url_is_silent_noop() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(state.pending_git_prompt.is_some());
    assert!(state.pending_git_url.is_none());
    let focus_before = state.pending_git_focus;

    let outcome = handle_with_services(&mut state, key(KeyCode::Char('o')));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    // Prompt still open, focus unchanged.
    assert!(state.pending_git_prompt.is_some());
    assert_eq!(state.pending_git_focus, focus_before);
}

/// With `pending_git_url == Some(url)`, `O` requests browser-open and
/// keeps the prompt open. The owning console input layer executes the
/// side effect.
#[test]
fn o_shortcut_with_url_returns_open_request_and_keeps_prompt_open() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    // Force a URL into state for the test; the real handler would have
    // populated this via `resolve_git_url` when origin is a GitHub URL.
    state.pending_git_url = Some("file:///tmp/definitely-not-real".to_owned());

    let outcome = handle_with_services(&mut state, key(KeyCode::Char('O')));
    assert!(matches!(
        outcome,
        FileBrowserOutcome::OpenGitUrl(url) if url == "file:///tmp/definitely-not-real"
    ));
    assert!(state.pending_git_prompt.is_some());
    // URL stays on state — O doesn't dismiss the prompt.
    assert_eq!(
        state.pending_git_url.as_deref(),
        Some("file:///tmp/definitely-not-real"),
    );
}

// ── Conditional hint segment ──────────────────────────────────────

/// The `O open` hint segment is only rendered when a URL is resolved.
/// With `has_url == false` the hint must not advertise `O open`.
#[test]
fn git_prompt_hint_omits_open_segment_when_url_is_none() {
    let rendered = format!("{:?}", git_prompt_footer_items(false));
    assert!(
        !rendered.contains('O'),
        "hint should not mention O when no URL: {rendered:?}"
    );
    assert!(
        !rendered.contains("open"),
        "hint should not mention 'open' when no URL: {rendered:?}"
    );
    assert!(rendered.contains('M'));
    assert!(rendered.contains('P'));
    assert!(rendered.contains("C/Esc"));
}

#[test]
fn git_prompt_hint_includes_open_segment_when_url_is_present() {
    let rendered = format!("{:?}", git_prompt_footer_items(true));
    assert!(
        rendered.contains('O'),
        "hint should mention O when URL resolved: {rendered:?}"
    );
    assert!(
        rendered.contains("open"),
        "hint should mention 'open' when URL resolved: {rendered:?}"
    );
    // Still preserves the other segments + trailing cancel.
    assert!(rendered.contains('M'));
    assert!(rendered.contains('P'));
    assert!(rendered.contains("C/Esc"));
}
