// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use jackin_core::{FileKind, FileSystem, HostPath, RepoPath};

use crate::launch::LaunchError;
use crate::project::git::{HostGit, Pace, words};
use crate::project::listing::Listing as UploadListing;
use crate::project::{ProjectStep, UnsupportedProject};

const GITLINK_MODE: &[u8] = b"160000";

#[derive(Debug, Default)]
struct Listing {
    skipped: bool,
    gitlinks: Vec<Vec<u8>>,
    untracked_dirs: Vec<Vec<u8>>,
}

fn parse(bytes: &[u8]) -> Listing {
    let mut listing = Listing::default();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        if let Some(path) = entry.strip_prefix(b"? ") {
            if let Some(dir) = path.strip_suffix(b"/") {
                listing.untracked_dirs.push(dir.to_vec());
            }
        } else if let Some(tab) = entry.iter().position(|byte| *byte == b'\t') {
            let mut fields = entry[..tab].split(|byte| *byte == b' ');
            let tag = fields.next().unwrap_or_default();
            let mode = fields.next().unwrap_or_default();
            if tag == b"S" || tag == b"s" {
                listing.skipped = true;
            }
            if mode == GITLINK_MODE {
                listing.gitlinks.push(entry[tab + 1..].to_vec());
            }
        }
    }
    listing
}

async fn populated(
    fs: &dyn FileSystem,
    root: &HostPath,
    candidates: &[Vec<u8>],
) -> Result<Vec<RepoPath>, LaunchError> {
    let mut found = Vec::new();
    for name in candidates {
        let relative = Path::new(OsStr::from_bytes(name));
        let dir =
            HostPath::resolve(relative, root).map_err(|error| malformed(&error.to_string()))?;
        if fs.stat(&dir).await? == Some(FileKind::Dir) && !fs.list(&dir).await?.is_empty() {
            found.push(
                RepoPath::parse(&String::from_utf8_lossy(name))
                    .map_err(|error| malformed(&error.to_string()))?,
            );
        }
    }
    Ok(found)
}

fn malformed(message: &str) -> LaunchError {
    LaunchError::ProjectOutput {
        step: ProjectStep::Listing,
        message: message.to_owned(),
    }
}

pub(super) async fn check(git: &HostGit<'_>, fs: &dyn FileSystem) -> Result<(), LaunchError> {
    let output = git
        .require(
            ProjectStep::Listing,
            Pace::Long,
            words(&["ls-files", "-v", "-s", "-o", "--exclude-standard", "-z"]),
            Vec::new(),
        )
        .await?;
    let listing = parse(&output);
    if listing.skipped {
        return Err(UnsupportedProject::SparseCheckout.into());
    }
    let submodules = populated(fs, git.root, &listing.gitlinks).await?;
    if !submodules.is_empty() {
        return Err(UnsupportedProject::PopulatedSubmodules(submodules).into());
    }
    let nested = populated(fs, git.root, &listing.untracked_dirs).await?;
    if nested.is_empty() {
        Ok(())
    } else {
        Err(UnsupportedProject::NestedRepositories(nested).into())
    }
}

pub(super) async fn symlinked_parents(
    fs: &dyn FileSystem,
    root: &HostPath,
    listing: &UploadListing,
) -> Result<(), LaunchError> {
    let mut real: BTreeSet<&[u8]> = BTreeSet::new();
    let mut linked: BTreeSet<&[u8]> = BTreeSet::new();
    let mut found = Vec::new();
    for name in listing.paths() {
        for (index, byte) in name.iter().enumerate() {
            if *byte != b'/' {
                continue;
            }
            let parent = &name[..index];
            if real.contains(parent) {
                continue;
            }
            if !linked.contains(parent) {
                let dir = HostPath::resolve(Path::new(OsStr::from_bytes(parent)), root)
                    .map_err(|error| malformed(&error.to_string()))?;
                if fs.stat(&dir).await? == Some(FileKind::Symlink) {
                    linked.insert(parent);
                } else {
                    real.insert(parent);
                    continue;
                }
            }
            found.push(
                RepoPath::parse(&String::from_utf8_lossy(name))
                    .map_err(|error| malformed(&error.to_string()))?,
            );
            break;
        }
    }
    if found.is_empty() {
        Ok(())
    } else {
        Err(UnsupportedProject::SymlinkedPath(found).into())
    }
}
