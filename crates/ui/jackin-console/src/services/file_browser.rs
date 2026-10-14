// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::tui::runtime::BlockingSubscription;
use directories::BaseDirs;

use crate::tui::components::file_browser::{FileBrowserOutcome, FileBrowserState};
pub use crate::tui::components::file_browser::{FolderEntry, FolderListing};
use crate::tui::effect::FileBrowserEffectContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserOpenTarget {
    EditorAddMount,
    EditorAuthSourceFolder,
    CreatePrelude,
    GlobalMount,
    SettingsAuthSourceFolder,
}

#[derive(Debug, Clone)]
pub enum FileBrowserListingRequest {
    OpenHome {
        target: FileBrowserOpenTarget,
        last_cwd: Option<PathBuf>,
        show_hidden: bool,
    },
    NavigateTo {
        context: FileBrowserEffectContext,
        root: PathBuf,
        path: PathBuf,
        show_hidden: bool,
    },
    NavigateUp {
        context: FileBrowserEffectContext,
        root: PathBuf,
        cwd: PathBuf,
        show_hidden: bool,
    },
}

#[derive(Debug)]
pub enum FileBrowserListingResult {
    OpenHome {
        target: FileBrowserOpenTarget,
        result: Result<Box<FileBrowserState>, String>,
    },
    Listing {
        context: FileBrowserEffectContext,
        listing: Option<FolderListing>,
    },
}

pub const EXCLUDED: &[&str] = &[
    "Library",
    "Applications",
    "Movies",
    "Music",
    "OrbStack",
    "Pictures",
];

pub fn has_git_dir(path: &Path) -> bool {
    let dotgit = path.join(".git");
    dotgit.is_dir() || dotgit.is_file()
}

pub fn is_excluded(name: &str) -> bool {
    EXCLUDED.contains(&name)
}

pub fn is_within_root(candidate: &Path, root: &Path) -> bool {
    let Ok(real_candidate) = candidate.canonicalize() else {
        return false;
    };
    real_candidate.starts_with(root)
}

pub fn canonicalize_or_self(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

pub fn is_directory(path: &Path) -> bool {
    path.is_dir()
}

pub fn load_entries(cwd: &Path, root: &Path, show_hidden: bool) -> Vec<FolderEntry> {
    let mut out: Vec<FolderEntry> = Vec::new();

    if cwd != root
        && let Some(parent) = cwd.parent()
        && parent.starts_with(root)
    {
        out.push(FolderEntry {
            name: "..".to_owned(),
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
        if name.starts_with('.') && !show_hidden {
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

pub fn listing_from_home() -> anyhow::Result<FolderListing> {
    listing_from_home_with_hidden_inner(false)
}

pub fn state_from_home() -> anyhow::Result<FileBrowserState> {
    Ok(FileBrowserState::from_listing(listing_from_home()?))
}

pub fn state_from_home_with_hidden() -> anyhow::Result<FileBrowserState> {
    let mut state = FileBrowserState::from_listing(listing_from_home_with_hidden()?);
    state.show_hidden = true;
    Ok(state)
}

pub fn start_listing_request(
    request: FileBrowserListingRequest,
) -> BlockingSubscription<FileBrowserListingResult> {
    crate::tui::runtime::spawn_named_blocking_subscription(
        "jackin-file-browser-listing",
        move || run_listing_request(request),
    )
}

fn run_listing_request(request: FileBrowserListingRequest) -> FileBrowserListingResult {
    match request {
        FileBrowserListingRequest::OpenHome {
            target,
            last_cwd,
            show_hidden,
        } => {
            let result = if show_hidden {
                state_from_home_with_hidden()
            } else {
                state_from_home()
            }
            .map(|mut state| {
                if let Some(cwd) = last_cwd.as_ref() {
                    clamp_state_to_cwd(&mut state, cwd);
                }
                Box::new(state)
            })
            .map_err(|error| error.to_string());
            FileBrowserListingResult::OpenHome { target, result }
        }
        FileBrowserListingRequest::NavigateTo {
            context,
            root,
            path,
            show_hidden,
        } => FileBrowserListingResult::Listing {
            context,
            listing: Some(clamped_listing_with_hidden(&root, &path, show_hidden)),
        },
        FileBrowserListingRequest::NavigateUp {
            context,
            root,
            cwd,
            show_hidden,
        } => FileBrowserListingResult::Listing {
            context,
            listing: parent_listing_with_hidden(&root, &cwd, show_hidden),
        },
    }
}

pub fn listing_at(root: PathBuf, cwd: PathBuf) -> FolderListing {
    listing_at_with_hidden(root, cwd, false)
}

pub fn clamped_listing(root: &Path, cwd: &Path) -> FolderListing {
    clamped_listing_with_hidden(root, cwd, false)
}

pub fn parent_listing(root: &Path, cwd: &Path) -> Option<FolderListing> {
    parent_listing_with_hidden(root, cwd, false)
}

pub fn listing_at_with_hidden(root: PathBuf, cwd: PathBuf, show_hidden: bool) -> FolderListing {
    let root = canonicalize_or_self(root);
    let cwd = canonicalize_or_self(cwd);
    let entries = load_entries(&cwd, &root, show_hidden);
    FolderListing { root, cwd, entries }
}

pub fn clamped_listing_with_hidden(root: &Path, cwd: &Path, show_hidden: bool) -> FolderListing {
    let target = if is_within_root(cwd, root) && is_directory(cwd) {
        canonicalize_or_self(cwd.to_path_buf())
    } else {
        root.to_path_buf()
    };
    listing_at_with_hidden(root.to_path_buf(), target, show_hidden)
}

pub fn parent_listing_with_hidden(
    root: &Path,
    cwd: &Path,
    show_hidden: bool,
) -> Option<FolderListing> {
    if cwd == root {
        return None;
    }
    let parent = cwd.parent()?;
    if !parent.starts_with(root) {
        return None;
    }
    Some(listing_at_with_hidden(
        root.to_path_buf(),
        parent.to_path_buf(),
        show_hidden,
    ))
}

pub fn listing_from_home_with_hidden() -> anyhow::Result<FolderListing> {
    listing_from_home_with_hidden_inner(true)
}

fn listing_from_home_with_hidden_inner(show_hidden: bool) -> anyhow::Result<FolderListing> {
    let home = BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;
    Ok(listing_at_with_hidden(home.clone(), home, show_hidden))
}

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
    crate::tui::runtime::spawn_named_blocking_subscription(
        "jackin-file-browser-git-url",
        move || resolve_git_url(&path),
    )
}

pub fn request_git_url_resolution(state: &mut FileBrowserState, path: PathBuf) {
    let rx = start_git_url_resolution(path);
    state.attach_git_url_resolution(rx);
}

pub fn clamp_state_to_cwd(state: &mut FileBrowserState, cwd: &Path) {
    let listing = clamped_listing_with_hidden(&state.root, cwd, state.show_hidden);
    state.apply_listing(listing);
}

pub fn apply_state_outcome(
    state: &mut FileBrowserState,
    outcome: FileBrowserOutcome<PathBuf>,
) -> FileBrowserOutcome<PathBuf> {
    match outcome {
        FileBrowserOutcome::NavigateTo(path) => {
            let listing = clamped_listing_with_hidden(&state.root, &path, state.show_hidden);
            state.apply_listing(listing);
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::NavigateUp => {
            if let Some(listing) =
                parent_listing_with_hidden(&state.root, state.cwd(), state.show_hidden)
            {
                state.apply_listing(listing);
            }
            FileBrowserOutcome::Continue
        }
        FileBrowserOutcome::RequestCommit(path) => match validate_commit(&state.root, &path) {
            Ok(path) => FileBrowserOutcome::Commit(path),
            Err(reason) => {
                state.reject_commit(reason);
                FileBrowserOutcome::Continue
            }
        },
        other => other,
    }
}

pub fn open_git_url(url: &str) {
    drop(open::that_detached(url));
}
