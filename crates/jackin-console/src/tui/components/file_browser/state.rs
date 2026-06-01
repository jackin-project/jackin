//! File-browser terminal state.
//!
//! Houses `FileBrowserState` (the modal's working state). Directory scanning,
//! sandbox policy, and git-origin inspection live in `services::file_browser`.

use std::path::{Path, PathBuf};

use tui_widget_list::ListState;

use super::git_prompt::GitPromptFocus;
use crate::services::file_browser::{FolderEntry, FolderListing};
use crate::tui::components::list_helpers::{cycle_select, list_state_for_count, selected_choice};

#[derive(Debug)]
pub struct FileBrowserState {
    /// $HOME — the browser cannot navigate above this path.
    pub root: PathBuf,
    /// Currently-displayed directory.
    pub cwd: PathBuf,
    /// Entries loaded from `cwd`, after filtering + sorting.
    pub entries: Vec<FolderEntry>,
    /// tui-widget-list selection state. Drives which row is highlighted.
    pub list_state: ListState,
    /// Set when the operator presses `s` but the selection is rejected
    /// (e.g. `$HOME` itself, `~/.jackin/...`). Cleared on the next key.
    pub rejected_reason: Option<String>,
    /// Active when the operator has pressed Enter on a git-repo row.
    pub pending_git_prompt: Option<PathBuf>,
    /// Origin URL (web form) for the repo referenced by
    /// `pending_git_prompt`. `None` for non-GitHub remotes or any repo
    /// whose origin can't be resolved — the overlay then omits the row.
    pub pending_git_url: Option<String>,
    pub(super) pending_git_url_rx:
        Option<jackin_tui::runtime::BlockingSubscription<Option<String>>>,
    /// Which button is highlighted in the git-repo prompt.
    pub pending_git_focus: GitPromptFocus,
}

impl FileBrowserState {
    /// Footer-bar hints for the current state. The screen footer renders these
    /// (hints are footer-only — the browser draws no internal hint row); the
    /// git-repo confirm overlay swaps in its own confirm/cancel keys.
    pub fn footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>> {
        use jackin_tui::HintSpan;
        if self.pending_git_prompt.is_some() {
            super::git_prompt::git_prompt_footer_items(self.pending_git_url.is_some())
        } else {
            vec![
                HintSpan::Key("\u{2191}\u{2193}"),
                HintSpan::Text("navigate"),
                HintSpan::GroupSep,
                HintSpan::Key("↵"),
                HintSpan::Text("open"),
                HintSpan::GroupSep,
                HintSpan::Key("H/\u{2190}"),
                HintSpan::Text("up"),
                HintSpan::GroupSep,
                HintSpan::Key("S"),
                HintSpan::Text("select"),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("up/cancel"),
            ]
        }
    }

    pub fn from_listing(listing: FolderListing) -> Self {
        let FolderListing { root, cwd, entries } = listing;
        let list_state = list_state_for_count(entries.len());
        Self {
            root,
            cwd,
            entries,
            list_state,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_url: None,
            pending_git_url_rx: None,
            pending_git_focus: GitPromptFocus::MountHere,
        }
    }

    pub fn apply_listing(&mut self, listing: FolderListing) {
        self.root = listing.root;
        self.cwd = listing.cwd;
        self.entries = listing.entries;
        self.list_state = list_state_for_count(self.entries.len());
    }

    /// Current working directory. Exposed so the create-workspace wizard
    /// can breadcrumb this across a step-back.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Move selection down one, wrapping at the end.
    pub(super) fn select_next(&mut self) {
        cycle_select(&mut self.list_state, self.entries.len(), 1);
    }

    /// Move selection up one, wrapping at the start.
    pub(super) fn select_prev(&mut self) {
        cycle_select(&mut self.list_state, self.entries.len(), -1);
    }

    /// The currently-highlighted entry, if any.
    pub(super) fn highlighted(&self) -> Option<&FolderEntry> {
        selected_choice(&self.entries, self.list_state.selected)
    }

    pub fn reject_commit(&mut self, reason: String) {
        self.rejected_reason = Some(reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::file_browser::EXCLUDED;
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
}
