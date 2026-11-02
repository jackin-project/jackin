// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use jackin_core::{
    FileKind, FileSystem, HostPath, ProcessStatus, ProcessStdin, RepoPath, SecretBytes,
};

use crate::launch::LaunchError;
use crate::project::git::{HostGit, Pace, failure, words};
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

fn local_drivers(config: &[u8]) -> BTreeSet<String> {
    let mut drivers = BTreeSet::new();
    let fields: Vec<&[u8]> = config.split(|byte| *byte == 0).collect();
    for pair in fields.as_chunks::<2>().0 {
        if pair[0] != b"local" && pair[0] != b"worktree" {
            continue;
        }
        let key = pair[1]
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or_default();
        let key = String::from_utf8_lossy(key);
        if let Some(inner) = key.strip_prefix("filter.") {
            for variable in ["clean", "smudge", "process"] {
                if let Some(name) = inner.strip_suffix(&format!(".{variable}")) {
                    drivers.insert(name.to_owned());
                }
            }
        }
    }
    drivers
}

pub(super) async fn filters(git: &HostGit<'_>, listing: &UploadListing) -> Result<(), LaunchError> {
    let config = git
        .require(
            ProjectStep::Listing,
            Pace::Quick,
            words(&["config", "--show-scope", "-z", "--list"]),
            Vec::new(),
        )
        .await?;
    let drivers = local_drivers(&config);
    if drivers.is_empty() {
        return Ok(());
    }
    let stdin = ProcessStdin::Bytes(SecretBytes::new(listing.as_bytes().to_vec()));
    let output = git
        .capture_from(
            Pace::Long,
            words(&["check-attr", "-z", "--stdin", "filter"]),
            Vec::new(),
            stdin,
        )
        .await?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(failure(ProjectStep::Listing, &output));
    }
    let mut selected: BTreeMap<String, Vec<RepoPath>> = BTreeMap::new();
    let fields: Vec<&[u8]> = output.stdout.split(|byte| *byte == 0).collect();
    for triple in fields.as_chunks::<3>().0 {
        let value = String::from_utf8_lossy(triple[2]).into_owned();
        if drivers.contains(&value) {
            selected.entry(value).or_default().push(
                RepoPath::parse(&String::from_utf8_lossy(triple[0]))
                    .map_err(|error| malformed(&error.to_string()))?,
            );
        }
    }
    if selected.is_empty() {
        return Ok(());
    }
    let names = selected.keys().cloned().collect();
    let mut paths: Vec<RepoPath> = selected.into_values().flatten().collect();
    paths.sort();
    paths.dedup();
    Err(UnsupportedProject::RepositoryFilter {
        drivers: names,
        paths,
    }
    .into())
}
