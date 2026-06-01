//! Non-TUI file-browser services.
//!
//! Directory scanning, sandbox policy, git-origin inspection, and host browser
//! launching live here so the TUI component can keep terminal state/rendering
//! separate from external work.

use std::path::{Path, PathBuf};

/// Directories excluded from the listing when browsing $HOME.
pub const EXCLUDED: &[&str] = &[
    "Library",
    "Applications",
    "Movies",
    "Music",
    "OrbStack",
    "Pictures",
];

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

/// Does `path` contain a `.git` child? Dir (regular clone) OR file
/// (submodule worktree, `.git` is a file pointing at the real gitdir).
pub fn has_git_dir(path: &Path) -> bool {
    let dotgit = path.join(".git");
    dotgit.is_dir() || dotgit.is_file()
}

/// Is `name` one of the top-level noise directories we hide at `$HOME`?
pub fn is_excluded(name: &str) -> bool {
    EXCLUDED.contains(&name)
}

/// True when `candidate`'s canonicalized path is contained within `root`.
pub fn is_within_root(candidate: &Path, root: &Path) -> bool {
    let Ok(real_candidate) = candidate.canonicalize() else {
        return false;
    };
    real_candidate.starts_with(root)
}

/// Canonicalize `path`, falling back to `path` itself on error.
pub fn canonicalize_or_self(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

/// True when the path is currently a directory.
pub fn is_directory(path: &Path) -> bool {
    path.is_dir()
}

/// Read directories under `cwd` and build the entry list.
pub fn load_entries(cwd: &Path, root: &Path) -> Vec<FolderEntry> {
    let mut out: Vec<FolderEntry> = Vec::new();

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
        if name.starts_with('.') {
            continue;
        }
        if at_root && is_excluded(&name) {
            continue;
        }
        let Ok(ty) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let is_dir = ty.is_dir() || (ty.is_symlink() && path.is_dir());
        if !is_dir {
            continue;
        }
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
    dirs.sort_by_key(|e| e.name.to_lowercase());
    out.extend(dirs);
    out
}

/// Resolve the web URL for a git-repo path via `mount_info::inspect`.
pub fn resolve_git_url(path: &Path) -> Option<String> {
    match crate::mount_info::inspect(&path.display().to_string()) {
        crate::mount_info::MountKind::Git {
            origin: Some(crate::mount_info::GitOrigin::Github { web_url, .. }),
            ..
        } => Some(web_url),
        _ => None,
    }
}

/// Open a resolved git web URL in the host browser.
pub fn open_git_url(url: &str) {
    let _ = open::that_detached(url);
}
