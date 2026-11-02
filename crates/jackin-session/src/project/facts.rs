// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use jackin_core::{CommitId, FileSystem, HostPath, ProcessStatus, RepoPath};

use crate::launch::LaunchError;
use crate::project::ProjectStep;
use crate::project::git::{HostGit, Pace, failure, words};

const JACKIN_HEADS: &str = "refs/heads/jackin/";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            ObjectFormat::Sha1 => "sha1",
            ObjectFormat::Sha256 => "sha256",
        }
    }

    fn parse(text: &str) -> Option<ObjectFormat> {
        match text {
            "sha1" => Some(ObjectFormat::Sha1),
            "sha256" => Some(ObjectFormat::Sha256),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Head {
    Branch { reference: String, commit: CommitId },
    Detached { commit: CommitId },
    Unborn { reference: String },
}

impl Head {
    pub(super) fn is_born(&self) -> bool {
        !matches!(self, Head::Unborn { .. })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Identity {
    pub(super) name: Option<String>,
    pub(super) email: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct Facts {
    pub(super) head: Head,
    pub(super) format: ObjectFormat,
    pub(super) shallow: Option<Vec<u8>>,
    pub(super) identity: Identity,
    pub(super) exclude: Option<Vec<u8>>,
    pub(super) objects: HostPath,
}

fn malformed(step: ProjectStep, message: &str) -> LaunchError {
    LaunchError::ProjectOutput {
        step,
        message: message.to_owned(),
    }
}

fn host_path(step: ProjectStep, bytes: &[u8]) -> Result<HostPath, LaunchError> {
    HostPath::parse(&PathBuf::from(OsStr::from_bytes(bytes)))
        .map_err(|error| malformed(step, &error.to_string()))
}

async fn head(git: &HostGit<'_>) -> Result<Head, LaunchError> {
    let symbolic = git
        .capture(
            Pace::Quick,
            words(&["symbolic-ref", "-q", "HEAD"]),
            Vec::new(),
        )
        .await?;
    let reference = match symbolic.status {
        ProcessStatus::Exited(0) => Some(
            String::from_utf8_lossy(&symbolic.stdout)
                .trim_end()
                .to_owned(),
        ),
        ProcessStatus::Exited(1) => None,
        _ => return Err(failure(ProjectStep::Head, &symbolic)),
    };
    let resolved = git
        .capture(
            Pace::Quick,
            words(&["rev-parse", "-q", "--verify", "HEAD"]),
            Vec::new(),
        )
        .await?;
    let commit = match resolved.status {
        ProcessStatus::Exited(0) => Some(
            CommitId::parse(String::from_utf8_lossy(&resolved.stdout).trim())
                .map_err(|error| malformed(ProjectStep::Head, &error.to_string()))?,
        ),
        ProcessStatus::Exited(1) => None,
        _ => return Err(failure(ProjectStep::Head, &resolved)),
    };
    match (reference, commit) {
        (Some(reference), Some(commit)) if reference.starts_with(JACKIN_HEADS) => {
            Ok(Head::Detached { commit })
        }
        (Some(reference), Some(commit)) => Ok(Head::Branch { reference, commit }),
        (Some(reference), None) => Ok(Head::Unborn { reference }),
        (None, Some(commit)) => Ok(Head::Detached { commit }),
        (None, None) => Err(malformed(
            ProjectStep::Head,
            "HEAD is neither a branch nor a commit",
        )),
    }
}

async fn identity_key(git: &HostGit<'_>, key: &str) -> Result<Option<String>, LaunchError> {
    let output = git
        .capture(
            Pace::Quick,
            words(&["config", "-z", "--get", key]),
            Vec::new(),
        )
        .await?;
    match output.status {
        ProcessStatus::Exited(0) => {
            let value = output.stdout.strip_suffix(&[0]).unwrap_or(&output.stdout);
            Ok(Some(String::from_utf8_lossy(value).into_owned()))
        }
        ProcessStatus::Exited(1) => Ok(None),
        _ => Err(failure(ProjectStep::Identity, &output)),
    }
}

async fn regular_exclude(
    fs: &dyn FileSystem,
    git_dir: &HostPath,
) -> Result<Option<Vec<u8>>, LaunchError> {
    let relative = RepoPath::parse("info/exclude")
        .map_err(|error| malformed(ProjectStep::Describe, &error.to_string()))?;
    Ok(fs
        .read_regular(git_dir, &relative)
        .await?
        .ok()
        .map(|file| file.bytes))
}

pub(super) async fn read(git: &HostGit<'_>, fs: &dyn FileSystem) -> Result<Facts, LaunchError> {
    let described = git
        .require(
            ProjectStep::Describe,
            Pace::Quick,
            words(&[
                "rev-parse",
                "--path-format=absolute",
                "--show-object-format",
                "--is-shallow-repository",
                "--git-path",
                "objects",
                "--git-common-dir",
                "--git-path",
                "shallow",
            ]),
            Vec::new(),
        )
        .await?;
    let lines: Vec<&[u8]> = described.split(|byte| *byte == b'\n').collect();
    let [format, shallow, objects, git_dir, shallow_file, ..] = lines.as_slice() else {
        return Err(malformed(
            ProjectStep::Describe,
            "rev-parse printed too few lines",
        ));
    };
    let format = ObjectFormat::parse(&String::from_utf8_lossy(format))
        .ok_or_else(|| malformed(ProjectStep::Describe, "unknown object format"))?;
    let shallow = if *shallow == b"true" {
        fs.read(&host_path(ProjectStep::Describe, shallow_file)?)
            .await?
    } else {
        None
    };
    let exclude = regular_exclude(fs, &host_path(ProjectStep::Describe, git_dir)?).await?;
    Ok(Facts {
        head: head(git).await?,
        format,
        shallow,
        identity: Identity {
            name: identity_key(git, "user.name").await?,
            email: identity_key(git, "user.email").await?,
        },
        exclude,
        objects: host_path(ProjectStep::Describe, objects)?,
    })
}
