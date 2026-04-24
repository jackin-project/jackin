//! Filesystem scanning, sandbox policy, and browser state.
//!
//! Houses `FileBrowserState` (the modal's working state), `FolderEntry`
//! (one row), and the listing/sandbox primitives (`load_entries`,
//! `is_within_root`, `canonicalize_or_self`, `has_git_dir`,
//! `is_excluded`). Construction and navigation methods that don't
//! involve input events or rendering live here too.

use std::path::{Path, PathBuf};

use directories::BaseDirs;
use tui_widget_list::ListState;

use super::EXCLUDED;
use super::git_prompt::GitPromptFocus;

/// Does `path` contain a `.git` child? Dir (regular clone) OR file
/// (submodule worktree, `.git` is a file pointing at the real gitdir).
/// Single `metadata` call per directory entry — no filesystem walk.
pub(super) fn has_git_dir(path: &Path) -> bool {
    let dotgit = path.join(".git");
    dotgit.is_dir() || dotgit.is_file()
}

/// One row in the folder listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    /// Display name, no trailing slash. `".."` for the synthetic parent link.
    pub name: String,
    /// Absolute path the row resolves to. For `..` this is the parent dir.
    pub path: PathBuf,
    /// True for the synthetic `..` parent-link row.
    pub is_parent: bool,
    /// True iff `path` contains a `.git` child (dir or submodule file).
    pub is_git: bool,
}

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
    /// Which button is highlighted in the git-repo prompt.
    pub pending_git_focus: GitPromptFocus,
}

/// Is `name` one of the top-level noise directories we hide at `$HOME`?
pub(super) fn is_excluded(name: &str) -> bool {
    EXCLUDED.contains(&name)
}

/// True when `candidate`'s canonicalized path is contained within
/// `root` (which is already canonicalized when possible). Used to
/// defeat lexical-prefix spoofs: a symlink under `$HOME` pointing
/// outside `$HOME` has a lexical path that `Path::starts_with`
/// accepts, but `canonicalize` resolves the link and exposes the
/// real target.
///
/// Fails closed — any canonicalization error returns `false` rather
/// than admitting the path. Better to reject a legitimate candidate
/// (the operator sees an unexplained rejection and can investigate)
/// than to leak a sandbox-escaping path.
pub(super) fn is_within_root(candidate: &Path, root: &Path) -> bool {
    let Ok(real_candidate) = candidate.canonicalize() else {
        return false;
    };
    real_candidate.starts_with(root)
}

/// Canonicalize `path`, falling back to `path` itself on error.
/// Used to normalise `root` once at construction time so later
/// `is_within_root` comparisons share a common prefix. Production
/// `$HOME` is almost always canonicalizable; tests under platforms
/// with weird mount layouts (e.g. macOS `/tmp` → `/private/tmp`)
/// rely on the fallback to keep behavior sane.
pub(super) fn canonicalize_or_self(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

/// Read directories under `cwd` and build the entry list. Hidden files
/// (leading `.`) are excluded; the `..` synthetic parent-link is prepended
/// iff `cwd != root`; at the root level the `EXCLUDED` list filters out
/// the macOS-style noise folders. Unreadable directories yield an empty
/// list — the caller surfaces that as an empty listing, not an error.
pub(super) fn load_entries(cwd: &Path, root: &Path) -> Vec<FolderEntry> {
    let mut out: Vec<FolderEntry> = Vec::new();

    // Synthetic parent link — only shown when there's somewhere to go.
    if cwd != root
        && let Some(parent) = cwd.parent()
        && parent.starts_with(root)
    {
        out.push(FolderEntry {
            name: "..".to_string(),
            path: parent.to_path_buf(),
            is_parent: true,
            is_git: false,
        });
    }

    let Ok(read) = std::fs::read_dir(cwd) else {
        return out;
    };

    let at_root = cwd == root;
    let mut dirs: Vec<FolderEntry> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden files (leading dot).
        if name.starts_with('.') {
            continue;
        }
        // Skip the $HOME-level noise folders.
        if at_root && is_excluded(&name) {
            continue;
        }
        // Only keep directories.
        let Ok(ty) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        // Follow symlinks to detect dir-symlinks; stat failures skip the entry.
        let is_dir = ty.is_dir() || (ty.is_symlink() && path.is_dir());
        if !is_dir {
            continue;
        }
        // Sandbox: a symlinked directory may point outside `root`. Lexical
        // `starts_with` can't catch that — it sees only the link's path
        // under root. Canonicalize the entry and drop anything whose real
        // target escapes. Regular (non-symlink) directories are trusted
        // because their canonical form necessarily starts with their
        // parent, which we're already listing because we're inside root.
        if ty.is_symlink() && !is_within_root(&path, root) {
            continue;
        }
        let is_git = has_git_dir(&path);
        dirs.push(FolderEntry {
            name,
            path,
            is_parent: false,
            is_git,
        });
    }
    // Alphabetical, case-insensitive. `..` (if present) was pushed first
    // and stays at index 0.
    dirs.sort_by_key(|e| e.name.to_lowercase());
    out.extend(dirs);
    out
}

impl FileBrowserState {
    /// Build a new browser starting at $HOME, filtered to directories only,
    /// excluding well-known noisy top-level folders.
    pub fn new_from_home() -> anyhow::Result<Self> {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;
        Ok(Self::new_at(home.clone(), home))
    }

    /// Build a browser with `root` as the sandbox boundary and `cwd` as
    /// the initial directory. Primarily for tests — production callers
    /// should use `new_from_home`.
    ///
    /// `root` (and `cwd`, when it was passed as equal to `root`) is
    /// canonicalized once here so later `is_within_root` checks compare
    /// apples to apples. If canonicalization fails (exotic mounts,
    /// missing paths in tests), the uncanonicalized path is used —
    /// production `$HOME` is always canonicalizable.
    pub fn new_at(root: PathBuf, cwd: PathBuf) -> Self {
        let root = canonicalize_or_self(root);
        // Keep `root == cwd` invariant when the caller intended that —
        // many tests pass the same path for both, relying on the
        // "no ..`" affordance at root. Otherwise canonicalize `cwd`
        // independently so downstream comparisons line up.
        let cwd = canonicalize_or_self(cwd);
        let entries = load_entries(&cwd, &root);
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            root,
            cwd,
            entries,
            list_state,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_url: None,
            pending_git_focus: GitPromptFocus::MountHere,
        }
    }

    /// Current working directory. Exposed so the create-workspace wizard
    /// can breadcrumb this across a step-back.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Re-point the browser at `cwd`, clamped to the sandbox root.
    /// Silently falls back to the root when `cwd` is outside the sandbox
    /// or not a readable directory.
    pub fn set_cwd(&mut self, cwd: &Path) {
        let target = if is_within_root(cwd, &self.root) && cwd.is_dir() {
            canonicalize_or_self(cwd.to_path_buf())
        } else {
            self.root.clone()
        };
        self.cwd = target;
        self.reload();
    }

    /// Re-read entries from disk and reset the selection to index 0.
    pub fn reload(&mut self) {
        self.entries = load_entries(&self.cwd, &self.root);
        let sel = if self.entries.is_empty() {
            None
        } else {
            Some(0)
        };
        self.list_state.select(sel);
    }

    /// Move selection down one, wrapping at the end.
    pub(super) fn select_next(&mut self) {
        let n = self.entries.len();
        if n == 0 {
            return;
        }
        let next = self
            .list_state
            .selected
            .map_or(0, |i| if i + 1 >= n { 0 } else { i + 1 });
        self.list_state.select(Some(next));
    }

    /// Move selection up one, wrapping at the start.
    pub(super) fn select_prev(&mut self) {
        let n = self.entries.len();
        if n == 0 {
            return;
        }
        let prev = self
            .list_state
            .selected
            .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(prev));
    }

    /// The currently-highlighted entry, if any.
    pub(super) fn highlighted(&self) -> Option<&FolderEntry> {
        self.list_state.selected.and_then(|i| self.entries.get(i))
    }

    /// Navigate up one level (`cwd` → `cwd.parent()`), clamped to `root`.
    pub(super) fn navigate_up(&mut self) {
        if self.cwd == self.root {
            return;
        }
        let Some(parent) = self.cwd.parent() else {
            return;
        };
        if !parent.starts_with(&self.root) {
            return;
        }
        self.cwd = parent.to_path_buf();
        self.reload();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_state_at(path: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(path.clone(), path)
    }

    fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(root, cwd)
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
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Library"));
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

        let fb = FileBrowserState::new_at(root.clone(), root);
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

        let fb = FileBrowserState::new_at(root.clone(), root);
        let names: Vec<&str> = fb.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"inner"));
        assert!(
            names.contains(&"inner_link"),
            "symlink whose target stays inside root should still list; got {names:?}",
        );
    }
}
