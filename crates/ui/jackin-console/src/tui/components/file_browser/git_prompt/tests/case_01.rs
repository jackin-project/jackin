// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
