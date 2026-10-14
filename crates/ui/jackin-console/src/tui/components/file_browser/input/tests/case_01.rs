// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn s_commits_highlighted_entry() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);

    handle_with_services(&mut state, key(KeyCode::Down));

    let outcome = handle_with_services(&mut state, key(KeyCode::Char('s')));
    match outcome {
        FileBrowserOutcome::Commit(path) => {
            assert_eq!(path.canonicalize().unwrap(), child.canonicalize().unwrap(),);
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

#[test]
fn s_falls_back_to_cwd_when_directory_is_empty() {
    let tmp = tempdir().unwrap();
    let empty = tmp.path().join("empty");
    std::fs::create_dir(&empty).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), empty.clone());

    let outcome = handle_with_services(&mut state, key(KeyCode::Char('s')));
    match outcome {
        FileBrowserOutcome::Commit(path) => {
            assert_eq!(path.canonicalize().unwrap(), empty.canonicalize().unwrap(),);
        }
        other => panic!("expected Commit, got {other:?}"),
    }
}

#[test]
fn s_rejects_root_itself() {
    let tmp = tempdir().unwrap();
    let mut state = make_state_at(tmp.path().to_path_buf());
    let outcome = handle_with_services(&mut state, key(KeyCode::Char('s')));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    assert!(state.rejected_reason.is_some());
}

#[test]
fn s_rejects_jackin_data_dir() {
    let tmp = tempdir().unwrap();
    let jackin = tmp.path().join(".jackin").join("workspaces");
    std::fs::create_dir_all(&jackin).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), jackin);
    let outcome = handle_with_services(&mut state, key(KeyCode::Char('s')));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    assert!(state.rejected_reason.is_some());
}

#[test]
fn rejection_cleared_on_next_keypress() {
    let tmp = tempdir().unwrap();
    let mut state = make_state_at(tmp.path().to_path_buf());
    handle_with_services(&mut state, key(KeyCode::Char('s')));
    assert!(state.rejected_reason.is_some());
    handle_with_services(&mut state, key(KeyCode::Char('j')));
    assert!(state.rejected_reason.is_none());
}

#[test]
fn page_down_moves_by_visible_rows_without_wrapping() {
    let tmp = tempdir().unwrap();
    for idx in 0..10 {
        std::fs::create_dir(tmp.path().join(format!("dir-{idx:02}"))).unwrap();
    }
    let mut state = make_state_at(tmp.path().to_path_buf());

    state.handle_key_with_page_rows(key(KeyCode::PageDown), Some(4));
    assert_eq!(state.list_state.selected().copied(), Some(4));

    state.handle_key_with_page_rows(key(KeyCode::PageDown), Some(4));
    assert_eq!(state.list_state.selected().copied(), Some(8));

    state.handle_key_with_page_rows(key(KeyCode::PageDown), Some(4));
    assert_eq!(state.list_state.selected().copied(), Some(9));
}

#[test]
fn page_up_moves_by_visible_rows_without_wrapping() {
    let tmp = tempdir().unwrap();
    for idx in 0..10 {
        std::fs::create_dir(tmp.path().join(format!("dir-{idx:02}"))).unwrap();
    }
    let mut state = make_state_at(tmp.path().to_path_buf());
    state.list_state.select(Some(8));

    state.handle_key_with_page_rows(key(KeyCode::PageUp), Some(4));
    assert_eq!(state.list_state.selected().copied(), Some(4));

    state.handle_key_with_page_rows(key(KeyCode::PageUp), Some(4));
    assert_eq!(state.list_state.selected().copied(), Some(0));

    state.handle_key_with_page_rows(key(KeyCode::PageUp), Some(4));
    assert_eq!(state.list_state.selected().copied(), Some(0));
}

#[test]
fn esc_at_root_cancels_modal() {
    let tmp = tempdir().unwrap();
    let mut state = make_state_at(tmp.path().to_path_buf());
    let outcome = handle_with_services(&mut state, key(KeyCode::Esc));
    assert!(matches!(outcome, FileBrowserOutcome::Cancel));
}

#[test]
fn esc_inside_subfolder_navigates_up() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), sub);
    let outcome = handle_with_services(&mut state, key(KeyCode::Esc));
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        tmp.path().canonicalize().unwrap(),
    );
}

#[test]
fn esc_deep_navigates_up_one_level() {
    let tmp = tempdir().unwrap();
    let l1 = tmp.path().join("a");
    let l2 = l1.join("b");
    let l3 = l2.join("c");
    std::fs::create_dir_all(&l3).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), l3);
    handle_with_services(&mut state, key(KeyCode::Esc));
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        l2.canonicalize().unwrap(),
    );
}

#[test]
fn esc_clears_rejected_reason() {
    let tmp = tempdir().unwrap();
    let mut state = make_state_at(tmp.path().to_path_buf());
    state.rejected_reason = Some("stale reason".into());
    let outcome = handle_with_services(&mut state, key(KeyCode::Esc));
    assert!(matches!(outcome, FileBrowserOutcome::Cancel));
    assert!(state.rejected_reason.is_none());
}

#[test]
fn h_navigates_up() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let mut state = state_rooted_at(tmp.path().to_path_buf(), sub);
    handle_with_services(&mut state, key(KeyCode::Char('h')));
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        tmp.path().canonicalize().unwrap(),
    );
}

#[test]
fn h_at_root_is_noop() {
    let tmp = tempdir().unwrap();
    let mut state = make_state_at(tmp.path().to_path_buf());
    handle_with_services(&mut state, key(KeyCode::Char('h')));

    assert_eq!(state.cwd, state.root);
}

#[test]
fn l_navigates_into_highlighted_dir() {
    let tmp = tempdir().unwrap();
    let child = tmp.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let mut state = make_state_at(tmp.path().to_path_buf());

    handle_with_services(&mut state, key(KeyCode::Char('l')));
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        child.canonicalize().unwrap(),
    );
}

#[test]
fn enter_on_plain_folder_still_navigates() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let plain = parent.join("plain");
    std::fs::create_dir_all(&plain).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(state.pending_git_prompt.is_none());
    assert_eq!(
        state.cwd.canonicalize().unwrap(),
        plain.canonicalize().unwrap(),
    );
}

#[test]
fn url_row_rect_none_when_no_url_flag() {
    let rect = git_prompt_url_row_rect(manufactured_modal_area(), false);
    assert!(rect.is_some(), "URL row should resolve for a valid modal");
    assert_eq!(rect.unwrap().y, 19);
}

#[test]
fn click_on_url_row_without_url_returns_false() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    assert!(state.pending_git_prompt.is_some());
    assert!(state.pending_git_url.is_none());

    let modal = manufactured_modal_area();
    let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
    let opened = state.url_to_open_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y);
    assert!(
        opened.is_none(),
        "click should not open when no URL is resolved"
    );
}

#[test]
fn click_outside_url_row_returns_false_even_with_url() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    state.pending_git_url = Some("file:///tmp/definitely-not-real".to_owned());

    let modal = manufactured_modal_area();
    let url_rect = git_prompt_url_row_rect(modal, false).unwrap();

    let opened = state.url_to_open_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y + 1);
    assert!(opened.is_none(), "click outside URL row should not open");

    let opened = state.url_to_open_on_click(modal, modal.x, url_rect.y);
    assert!(opened.is_none(), "click on left border should not open");
}

#[test]
fn click_on_url_row_with_url_returns_true() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let repo = parent.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
    handle_with_services(&mut state, key(KeyCode::Down));
    handle_with_services(&mut state, key(KeyCode::Enter));
    state.pending_git_url = Some("file:///tmp/definitely-not-real".to_owned());

    let modal = manufactured_modal_area();
    let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
    let opened = state.url_to_open_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y);
    assert_eq!(
        opened.as_deref(),
        Some("file:///tmp/definitely-not-real"),
        "click on URL row with URL should return URL",
    );

    assert!(state.pending_git_prompt.is_some());
}

#[cfg(unix)]
#[test]
fn commit_rejects_out_of_root_target() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("home");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    let mut fb = state_rooted_at(root.clone(), root);
    let outcome = commit_with_services(&mut fb, outside);
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    let reason = fb
        .rejected_reason
        .as_deref()
        .expect("out-of-root commit should set rejected_reason");
    assert!(
        reason.contains("outside"),
        "rejection should cite the sandbox boundary; got {reason:?}",
    );
}

#[cfg(unix)]
#[test]
fn commit_rejects_symlink_resolving_to_root() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    let link = root.join("escape_to_root");
    std::os::unix::fs::symlink(&root, &link).unwrap();

    let mut fb = state_rooted_at(root.clone(), root);
    let outcome = commit_with_services(&mut fb, link);
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    let reason = fb
        .rejected_reason
        .as_deref()
        .expect("root-aliased symlink should set rejected_reason");
    assert!(
        reason.contains("$HOME"),
        "rejection should cite the $HOME-itself rule; got {reason:?}",
    );
}

#[cfg(unix)]
#[test]
fn commit_rejects_symlink_resolving_to_jackin_data() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("home");
    let jackin = root.join(".jackin").join("workspaces");
    std::fs::create_dir_all(&jackin).unwrap();
    let link = root.join("escape_to_jackin");
    std::os::unix::fs::symlink(&jackin, &link).unwrap();

    let mut fb = state_rooted_at(root.clone(), root);
    let outcome = commit_with_services(&mut fb, link);
    assert!(matches!(outcome, FileBrowserOutcome::Continue));
    let reason = fb
        .rejected_reason
        .as_deref()
        .expect("jackin-aliased symlink should set rejected_reason");
    assert!(
        reason.contains(".jackin"),
        "rejection should cite the reserved-paths rule; got {reason:?}",
    );
}

#[cfg(unix)]
#[test]
fn commit_accepts_symlink_resolving_to_normal_in_root_folder() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("home");
    let plain = root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let link = root.join("link_to_plain");
    std::os::unix::fs::symlink(&plain, &link).unwrap();

    let mut fb = state_rooted_at(root.clone(), root);
    let outcome = commit_with_services(&mut fb, link.clone());
    match outcome {
        FileBrowserOutcome::Commit(path) => {
            assert_eq!(path, link);
        }
        other => panic!("expected Commit, got {other:?}"),
    }
    assert!(fb.rejected_reason.is_none());
}
