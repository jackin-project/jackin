// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use jackin_core::{FileKind, FileSystem, HostPath, ProcessStatus, RepoPath};

use crate::launch::LaunchError;
use crate::project::facts::Facts;
use crate::project::git::{HostGit, Pace, failure, words};
use crate::project::{ProjectStep, UnsupportedProject};

const BACK_REFERENCE_LIMIT: usize = 4096;

async fn real(fs: &dyn FileSystem, path: &[u8]) -> Result<Option<HostPath>, LaunchError> {
    match HostPath::parse(Path::new(OsStr::from_bytes(path))) {
        Ok(path) => Ok(fs.canonicalize(&path).await?),
        Err(_) => Ok(None),
    }
}

async fn linked_worktree(
    fs: &dyn FileSystem,
    dot_git: &HostPath,
    facts: &Facts,
) -> Result<bool, LaunchError> {
    let Some(id) = facts.own_dir.as_path().file_name() else {
        return Ok(false);
    };
    let registered = HostPath::resolve(&Path::new("worktrees").join(id), &facts.git_dir)?;
    if registered != facts.own_dir {
        return Ok(false);
    }
    let Ok(file) = fs
        .read_regular(
            &facts.own_dir,
            &RepoPath::parse("gitdir")?,
            BACK_REFERENCE_LIMIT,
        )
        .await?
    else {
        return Ok(false);
    };
    if file.bytes.len() > BACK_REFERENCE_LIMIT {
        return Ok(false);
    }
    let line = file
        .bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or(&[]);
    Ok(real(fs, line.trim_ascii()).await?.as_ref() == Some(dot_git))
}

async fn submodule(
    git: &HostGit<'_>,
    fs: &dyn FileSystem,
    root: &HostPath,
) -> Result<bool, LaunchError> {
    let configured = git
        .capture(
            Pace::Quick,
            words(&["config", "--local", "--get", "core.worktree"]),
            Vec::new(),
        )
        .await?;
    match configured.status {
        ProcessStatus::Exited(0) => {}
        ProcessStatus::Exited(1) => return Ok(false),
        _ => return Err(failure(ProjectStep::Describe, &configured)),
    }
    let top = git
        .capture(
            Pace::Quick,
            words(&["rev-parse", "--path-format=absolute", "--show-toplevel"]),
            Vec::new(),
        )
        .await?;
    if top.status != ProcessStatus::Exited(0) {
        return Ok(false);
    }
    let line = top
        .stdout
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or(&[]);
    Ok(real(fs, line).await?.as_ref() == Some(root))
}

pub(super) async fn belongs(
    git: &HostGit<'_>,
    fs: &dyn FileSystem,
    root: &HostPath,
    facts: &Facts,
) -> Result<(), LaunchError> {
    let Some(root_real) = fs.canonicalize(root).await? else {
        return Err(UnsupportedProject::ForeignGit(root.clone()).into());
    };
    let dot_git = HostPath::resolve(Path::new(".git"), &root_real)?;
    let owned = match fs.stat(&dot_git).await? {
        Some(FileKind::Dir) => facts.git_dir == dot_git && facts.own_dir == dot_git,
        Some(FileKind::File) if facts.own_dir == facts.git_dir => {
            submodule(git, fs, &root_real).await?
        }
        Some(FileKind::File) => linked_worktree(fs, &dot_git, facts).await?,
        Some(FileKind::Symlink | FileKind::Socket) | None => false,
    };
    if owned {
        Ok(())
    } else {
        Err(UnsupportedProject::ForeignGit(root.clone()).into())
    }
}
