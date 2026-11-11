// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use jackin_core::{
    FileKind, FileSystem, FsError, HostPath, PathError, ProcessError, ProcessOutput, ProcessStatus,
    ProjectRoot, RepoPath, TerminalText, plain,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::launch::LaunchError;
use crate::project::git::{HostGit, Pace, exit_code, words};
use crate::project::refuse;
use crate::project::{ProjectStep, UnsupportedProject};
use crate::service::SessionDeps;

const BACK_REFERENCE_LIMIT: usize = 4096;

#[derive(Debug, Error)]
pub(crate) enum HostProjectError {
    #[error(transparent)]
    Unsupported(#[from] UnsupportedProject),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("git exited with {status}: {output}")]
    Git {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error("git printed unusable output: {}", plain(.message))]
    Output { message: String },
}

impl From<HostProjectError> for LaunchError {
    fn from(error: HostProjectError) -> LaunchError {
        match error {
            HostProjectError::Unsupported(error) => LaunchError::Unsupported(error),
            HostProjectError::Fs(error) => LaunchError::Fs(error),
            HostProjectError::Process(error) => LaunchError::Process(error),
            HostProjectError::Path(error) => LaunchError::Path(error),
            HostProjectError::Git { status, output } => LaunchError::ProjectFailed {
                step: ProjectStep::Describe,
                exit_code: exit_code(status),
                output,
            },
            HostProjectError::Output { message } => LaunchError::ProjectOutput {
                step: ProjectStep::Describe,
                message,
            },
        }
    }
}

fn failure(output: &ProcessOutput) -> HostProjectError {
    let text = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    HostProjectError::Git {
        status: output.status,
        output: TerminalText::sanitize(text),
    }
}

fn unusable(message: &str) -> HostProjectError {
    HostProjectError::Output {
        message: message.to_owned(),
    }
}

pub(super) struct Location {
    pub(super) git_dir: HostPath,
    own_dir: HostPath,
}

pub(crate) struct HostProject<'a> {
    discovered: HostGit<'a>,
    pub(super) location: Location,
}

async fn real(fs: &dyn FileSystem, path: &[u8]) -> Result<Option<HostPath>, HostProjectError> {
    match HostPath::parse(Path::new(OsStr::from_bytes(path))) {
        Ok(path) => Ok(fs.canonicalize(&path).await?),
        Err(_) => Ok(None),
    }
}

async fn linked_worktree(
    fs: &dyn FileSystem,
    dot_git: &HostPath,
    location: &Location,
) -> Result<bool, HostProjectError> {
    let Some(id) = location.own_dir.as_path().file_name() else {
        return Ok(false);
    };
    let registered = HostPath::resolve(&Path::new("worktrees").join(id), &location.git_dir)?;
    if registered != location.own_dir {
        return Ok(false);
    }
    let Ok(file) = fs
        .read_regular(
            &location.own_dir,
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
    Ok(real(fs, without_newline(&file.bytes)).await?.as_ref() == Some(dot_git))
}

async fn submodule(
    git: &HostGit<'_>,
    fs: &dyn FileSystem,
    root: &HostPath,
) -> Result<bool, HostProjectError> {
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
        _ => return Err(failure(&configured)),
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
    Ok(real(fs, without_newline(&top.stdout)).await?.as_ref() == Some(root))
}

async fn belongs(
    git: &HostGit<'_>,
    fs: &dyn FileSystem,
    root: &HostPath,
    location: &Location,
) -> Result<(), HostProjectError> {
    let Some(root_real) = fs.canonicalize(root).await? else {
        return Err(UnsupportedProject::ForeignGit(root.clone()).into());
    };
    let dot_git = HostPath::resolve(Path::new(".git"), &root_real)?;
    let owned = match fs.stat(&dot_git).await? {
        Some(FileKind::Dir) => location.git_dir == dot_git && location.own_dir == dot_git,
        Some(FileKind::File) if location.own_dir == location.git_dir => {
            submodule(git, fs, &root_real).await?
        }
        Some(FileKind::File) => linked_worktree(fs, &dot_git, location).await?,
        Some(FileKind::Symlink | FileKind::Socket | FileKind::Other) | None => false,
    };
    if owned {
        Ok(())
    } else {
        Err(UnsupportedProject::ForeignGit(root.clone()).into())
    }
}

async fn directory(fs: &dyn FileSystem, bytes: &[u8]) -> Result<HostPath, HostProjectError> {
    let path = HostPath::parse(Path::new(OsStr::from_bytes(bytes)))
        .map_err(|error| unusable(&error.to_string()))?;
    fs.canonicalize(&path)
        .await?
        .ok_or_else(|| unusable("the git directory does not exist"))
}

fn without_newline(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

async fn resolved(
    git: &HostGit<'_>,
    fs: &dyn FileSystem,
    flag: &str,
) -> Result<HostPath, HostProjectError> {
    let output = git
        .capture(
            Pace::Quick,
            words(&["rev-parse", "--path-format=absolute", flag]),
            Vec::new(),
        )
        .await?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(failure(&output));
    }
    directory(fs, without_newline(&output.stdout)).await
}

async fn locate(git: &HostGit<'_>, fs: &dyn FileSystem) -> Result<Location, HostProjectError> {
    Ok(Location {
        git_dir: resolved(git, fs, "--git-common-dir").await?,
        own_dir: resolved(git, fs, "--absolute-git-dir").await?,
    })
}

impl<'a> HostProject<'a> {
    pub(crate) async fn open(
        deps: &'a SessionDeps,
        project: &'a ProjectRoot,
        cancel: &'a CancellationToken,
    ) -> Result<HostProject<'a>, HostProjectError> {
        let git = HostGit::unauthenticated(deps.host.runner.as_ref(), project.path(), cancel);
        let fs = deps.host.fs.as_ref();
        let location = locate(&git, fs).await?;
        belongs(&git, fs, project.path(), &location).await?;
        Ok(HostProject {
            discovered: git,
            location,
        })
    }

    pub(crate) fn git(&self) -> HostGit<'_> {
        self.discovered.within(&self.location.own_dir)
    }

    pub(crate) async fn refuse_foreign_store(
        &self,
        fs: &dyn FileSystem,
        references: &[String],
    ) -> Result<(), HostProjectError> {
        let own = &self.location.own_dir;
        let intact = fs.stat(own).await? == Some(FileKind::Dir)
            && fs.canonicalize(own).await?.as_ref() == Some(own);
        if !intact {
            return Err(UnsupportedProject::ForeignGit(self.discovered.root().clone()).into());
        }
        refuse::own_store(fs, &self.location.git_dir, references).await
    }
}
