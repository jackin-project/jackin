//! Non-TUI file-browser services.
//!
//! Directory scanning, sandbox policy, git-origin inspection, and host browser
//! launching live here so the TUI component can keep terminal state/rendering
//! separate from external work.

use std::path::{Path, PathBuf};

use directories::BaseDirs;
use jackin_tui::runtime::BlockingSubscription;

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

/// Fully-resolved directory listing handed to the TUI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListing {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<FolderEntry>,
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

/// Build the initial browser listing rooted at `$HOME`.
pub fn listing_from_home() -> anyhow::Result<FolderListing> {
    let home = BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;
    Ok(listing_at(home.clone(), home))
}

/// Build a listing at `cwd`, canonicalizing paths but not clamping to root.
pub fn listing_at(root: PathBuf, cwd: PathBuf) -> FolderListing {
    let root = canonicalize_or_self(root);
    let cwd = canonicalize_or_self(cwd);
    let entries = load_entries(&cwd, &root);
    FolderListing { root, cwd, entries }
}

/// Re-point a listing at `cwd`, clamped to the sandbox root.
pub fn clamped_listing(root: &Path, cwd: &Path) -> FolderListing {
    let target = if is_within_root(cwd, root) && is_directory(cwd) {
        canonicalize_or_self(cwd.to_path_buf())
    } else {
        root.to_path_buf()
    };
    listing_at(root.to_path_buf(), target)
}

/// Move one level up inside the sandbox root.
pub fn parent_listing(root: &Path, cwd: &Path) -> Option<FolderListing> {
    if cwd == root {
        return None;
    }
    let parent = cwd.parent()?;
    if !parent.starts_with(root) {
        return None;
    }
    Some(listing_at(root.to_path_buf(), parent.to_path_buf()))
}

/// Validate a candidate workspace source path.
pub fn validate_commit(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let canonical = canonicalize_or_self(target.to_path_buf());

    if !is_within_root(&canonical, root) {
        return Err("Cannot commit a path outside $HOME.".into());
    }
    if canonical == root {
        return Err("Cannot use $HOME itself — navigate into a subfolder.".into());
    }
    let jackin_data = canonicalize_or_self(root.join(".jackin"));
    if canonical.starts_with(&jackin_data) {
        return Err("Cannot use ~/.jackin/* — those paths are reserved.".into());
    }

    Ok(target.to_path_buf())
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

pub fn start_git_url_resolution(path: PathBuf) -> BlockingSubscription<Option<String>> {
    jackin_tui::runtime::spawn_named_blocking_subscription(
        "jackin-file-browser-git-url",
        move || resolve_git_url(&path),
    )
}

/// Open a resolved git web URL in the host browser.
pub fn open_git_url(url: &str) {
    let _ = open::that_detached(url);
}
