// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `state`.
use super::*;
use crate::services::file_browser::EXCLUDED;
use ratatui::layout::Rect;
use tempfile::tempdir;

fn make_state_at(path: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        path.clone(),
        path,
    ))
}

fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(root, cwd))
}

// ── Filtering + directory-only listing ────────────────────────────

#[test]
fn filter_excludes_files() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("folder")).unwrap();
    std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"folder"), "folder missing: {names:?}");
    assert!(
        !names.contains(&"file.txt"),
        "file should be filtered out: {names:?}"
    );
}

#[test]
fn hidden_files_are_excluded() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("visible")).unwrap();
    std::fs::create_dir(tmp.path().join(".hidden")).unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"visible"));
    assert!(!names.contains(&".hidden"));
}

#[test]
fn excluded_names_filtered_at_root() {
    let tmp = tempdir().unwrap();
    for name in EXCLUDED {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }
    std::fs::create_dir(tmp.path().join("Projects")).unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
    for name in EXCLUDED {
        assert!(!names.contains(name), "excluded `{name}` slipped through");
    }
    assert!(names.contains(&"Projects"));
}

#[test]
fn excluded_names_visible_below_root() {
    // EXCLUDED only applies at the sandbox root; a folder named
    // "Library" one level below should still be visible.
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("sub");
    std::fs::create_dir_all(sub.join("Library")).unwrap();

    let state = state_rooted_at(tmp.path().to_path_buf(), sub);
    assert!(state.entries.iter().any(|e| e.name == "Library"));
}

#[test]
fn parent_link_absent_at_root() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("a")).unwrap();
    let state = make_state_at(tmp.path().to_path_buf());
    assert!(
        state.entries.iter().all(|e| !e.is_parent),
        "`..` must not appear at root: {:?}",
        state.entries
    );
}

#[test]
fn parent_link_present_below_root() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let state = state_rooted_at(tmp.path().to_path_buf(), sub);
    assert!(state.entries.first().is_some_and(|e| e.is_parent));
}

#[test]
fn wheel_selection_scroll_clamps_without_wrapping() {
    let tmp = tempdir().unwrap();
    for name in ["a", "b", "c"] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }
    let mut state = make_state_at(tmp.path().to_path_buf());

    assert!(!state.scroll_selection(-1));
    assert_eq!(state.list_state.selected().copied(), Some(0));
    assert!(state.scroll_selection(2));
    assert_eq!(state.list_state.selected().copied(), Some(2));
    assert!(!state.scroll_selection(1));
    assert_eq!(state.list_state.selected().copied(), Some(2));
    assert!(state.scroll_selection(-5));
    assert_eq!(state.list_state.selected().copied(), Some(0));
}

#[test]
fn wheel_selection_scroll_at_area_ignores_prompt_and_outside_pointer() {
    let tmp = tempdir().unwrap();
    for name in ["a", "b", "c"] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }
    let area = Rect {
        x: 2,
        y: 3,
        width: 10,
        height: 4,
    };
    let mut state = make_state_at(tmp.path().to_path_buf());

    assert!(!state.scroll_selection_at(area, 1, 3, 1));
    assert_eq!(state.list_state.selected().copied(), Some(0));
    assert!(state.scroll_selection_at(area, 2, 3, 1));
    assert_eq!(state.list_state.selected().copied(), Some(1));

    state.pending_git_prompt = Some(tmp.path().join("a"));
    assert!(!state.scroll_selection_at(area, 2, 3, 1));
    assert_eq!(state.list_state.selected().copied(), Some(1));
}

// ── Git-repo detection ────────────────────────────────────────────

#[test]
fn git_repo_entries_have_is_git_true() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let entry = state
        .entries
        .iter()
        .find(|e| e.name == "repo")
        .expect("repo row must exist");
    assert!(entry.is_git, "repo row must be flagged as git");
}

#[test]
fn non_git_folders_have_is_git_false() {
    let tmp = tempdir().unwrap();
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let entry = state
        .entries
        .iter()
        .find(|e| e.name == "plain")
        .expect("plain row must exist");
    assert!(!entry.is_git);
}

#[test]
fn submodule_gitfile_counts_as_git() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("submodule");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/submodule\n").unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let entry = state
        .entries
        .iter()
        .find(|e| e.name == "submodule")
        .expect("submodule row must exist");
    assert!(entry.is_git);
}

// ── Symlink sandbox hardening ─────────────────────────────────────
//
// Finding #2 of the PR #166 current-branch review: lexical
// `Path::starts_with(root)` treated a symlink under `$HOME` as
// in-sandbox because its *path* starts with `$HOME`, but its
// canonical target could escape. Canonicalizing at the
// decision points fixes the leak.

#[cfg(unix)]
#[test]
fn symlink_to_outside_root_is_excluded_from_listing() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("home");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(root.join("normal_dir")).unwrap();
    // Symlink under root pointing at the sibling directory. A lexical
    // `starts_with(root)` check accepts this path; a canonical one
    // correctly rejects it.
    std::os::unix::fs::symlink(&outside, root.join("escape_link")).unwrap();

    let fb = state_rooted_at(root.clone(), root);
    let names: Vec<&str> = fb.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"normal_dir"),
        "regular child dir should still appear; got {names:?}",
    );
    assert!(
        !names.contains(&"escape_link"),
        "symlink escaping $HOME must not appear in listing; got {names:?}",
    );
}

#[cfg(unix)]
#[test]
fn symlink_to_inside_root_still_appears() {
    // Complementary test to `symlink_to_outside_root_is_excluded_from_listing`:
    // we must not over-reject. A symlink that resolves back inside
    // root is legitimate and should still be listed.
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("home");
    let inner = root.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::os::unix::fs::symlink(&inner, root.join("inner_link")).unwrap();

    let fb = state_rooted_at(root.clone(), root);
    let names: Vec<&str> = fb.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"inner"));
    assert!(
        names.contains(&"inner_link"),
        "symlink whose target stays inside root should still list; got {names:?}",
    );
}
