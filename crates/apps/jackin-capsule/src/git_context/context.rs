// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Branch/head context readers.

use super::{git_capture_at_workdir, read_git_ref_oid};

use std::path::{Path, PathBuf};

use crate::session::{BranchName, GitContext, Oid};

pub(crate) fn git_current_context(workdir: &Path) -> GitContext {
    // Try the cheap path first: read `.git/HEAD` and parse the symref.
    // For a normal checkout on a branch the file is one line of
    // `ref: refs/heads/<name>\n` (no subprocess fork, ~50µs vs ~3-15ms
    // for `git branch --show-current`). Detached HEAD writes the raw
    // SHA which we treat as "no branch" — the bar slot stays hidden,
    // matching `git branch --show-current` which prints empty.
    //
    // Falls back to the subprocess path for worktrees (where `.git`
    // is a file, not a directory) and for any other unusual layout
    // the file-read approach cannot handle.
    if let Some(context) = read_context_from_git_metadata(workdir) {
        return match context {
            // `Branch` with no head means the loose+packed lookup
            // missed (unborn, race with `pack-refs`, etc.). Try the
            // subprocess as a last-resort recovery for that single
            // case rather than ship a head-less context.
            GitContext::Branch { name, head: None } => {
                let head = git_capture_at_workdir(workdir, &["rev-parse", "--verify", "HEAD"])
                    .as_deref()
                    .and_then(Oid::parse);
                GitContext::Branch { name, head }
            }
            other => other,
        };
    }
    git_context_from_subprocess(workdir)
}

#[cfg(test)]
pub(crate) fn read_branch_from_git_head(workdir: &Path) -> Option<BranchName> {
    match read_context_from_git_metadata(workdir)? {
        GitContext::Branch { name, .. } => Some(name),
        _ => None,
    }
}

pub(crate) fn git_context_from_subprocess(workdir: &Path) -> GitContext {
    let branch = git_capture_at_workdir(workdir, &["branch", "--show-current"])
        .as_deref()
        .and_then(BranchName::parse);
    let head = git_capture_at_workdir(workdir, &["rev-parse", "--verify", "HEAD"])
        .as_deref()
        .and_then(Oid::parse);
    match (branch, head) {
        (Some(name), head) => GitContext::Branch { name, head },
        (None, Some(head)) => GitContext::Detached { head },
        (None, None) => GitContext::Absent,
    }
}

pub(crate) fn read_context_from_git_metadata(workdir: &Path) -> Option<GitContext> {
    let metadata = git_metadata_dirs(workdir)?;
    let head_path = metadata.git_dir.join("HEAD");
    let head = crate::util::read_text_bounded(&head_path, GIT_METADATA_FILE_MAX_BYTES)?;
    let trimmed = head.trim();
    if let Some(ref_name) = trimmed.strip_prefix("ref: ") {
        let oid = read_git_ref_oid(
            &metadata.git_dir,
            metadata.common_git_dir.as_deref(),
            ref_name,
        );
        return Some(match BranchName::parse(ref_name) {
            // `ref:` pointing outside `refs/heads/` (e.g. refs/remotes/origin/HEAD)
            // is treated as detached for our chrome purposes — we have no branch
            // to show and the resolved tip (if any) is the head OID.
            Some(name) => GitContext::Branch { name, head: oid },
            None => oid.map_or(GitContext::Absent, |head| GitContext::Detached { head }),
        });
    }
    Some(if let Some(head) = Oid::parse(trimmed) {
        GitContext::Detached { head }
    } else {
        GitContext::Absent
    })
}

pub(crate) struct GitMetadataDirs {
    pub(crate) git_dir: PathBuf,
    pub(crate) common_git_dir: Option<PathBuf>,
}

pub(crate) fn git_metadata_dirs(workdir: &Path) -> Option<GitMetadataDirs> {
    let git_path = workdir.join(".git");
    if git_path.is_dir() {
        return Some(GitMetadataDirs {
            git_dir: git_path,
            common_git_dir: None,
        });
    }
    let git_file = crate::util::read_text_bounded(&git_path, GIT_METADATA_FILE_MAX_BYTES)?;
    let suffix = git_file.trim().strip_prefix("gitdir:")?;
    let git_dir = PathBuf::from(suffix.trim());
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        workdir.join(git_dir)
    };
    let common_git_dir = common_git_dir(&git_dir, GIT_METADATA_FILE_MAX_BYTES);
    Some(GitMetadataDirs {
        git_dir,
        common_git_dir,
    })
}

pub(crate) fn common_git_dir(git_dir: &Path, max_bytes: u64) -> Option<PathBuf> {
    let raw = crate::util::read_text_bounded(&git_dir.join("commondir"), max_bytes)?;
    let path = PathBuf::from(raw.trim());
    Some(if path.is_absolute() {
        path
    } else {
        git_dir.join(path)
    })
}

pub(crate) const GIT_METADATA_FILE_MAX_BYTES: u64 = 64 * 1024;
