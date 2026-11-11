// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::path::Path;

use jackin_core::{
    CommitId, FileSystem, HostPath, ProcessStatus, RegularFileProblem, RepoPath, TerminalText,
};

use crate::launch::LaunchError;
use crate::project::git::{HostGit, Pace, failure, words};
use crate::project::home::HostProject;
use crate::project::{ProjectStep, UnsupportedProject};

const BRANCHES: &str = "refs/heads/";
const JACKIN_HEADS: &str = "refs/heads/jackin/";
const SHALLOW_LIMIT: usize = 4 << 20;
const EXCLUDE_LIMIT: usize = 1 << 20;
const ATTRIBUTES_LIMIT: usize = 8 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

impl fmt::Display for ObjectFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ObjectFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectFormat::Sha1 => "sha1",
            ObjectFormat::Sha256 => "sha256",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<ObjectFormat> {
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
    pub(super) attributes: Option<Vec<u8>>,
    pub(super) objects: HostPath,
    pub(super) git_dir: HostPath,
}

fn malformed(step: ProjectStep, message: &str) -> LaunchError {
    LaunchError::ProjectOutput {
        step,
        message: message.to_owned(),
    }
}

fn is_branch(reference: &str) -> bool {
    reference.starts_with(BRANCHES) && !reference.starts_with(JACKIN_HEADS)
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
            std::str::from_utf8(&symbolic.stdout)
                .map_err(|_| malformed(ProjectStep::Head, "HEAD names a non-UTF-8 reference"))?
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
        (Some(reference), Some(commit)) if is_branch(&reference) => {
            Ok(Head::Branch { reference, commit })
        }
        (Some(_) | None, Some(commit)) => Ok(Head::Detached { commit }),
        (Some(reference), None) if is_branch(&reference) => Ok(Head::Unborn { reference }),
        (Some(reference), None) => Err(UnsupportedProject::UnbornHead {
            reference: TerminalText::sanitize(reference.as_bytes()),
        }
        .into()),
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

async fn metadata(
    fs: &dyn FileSystem,
    git_dir: &HostPath,
    name: &str,
    limit: usize,
) -> Result<Result<Vec<u8>, RegularFileProblem>, LaunchError> {
    let relative = RepoPath::parse(name)
        .map_err(|error| malformed(ProjectStep::Describe, &error.to_string()))?;
    match fs.read_regular(git_dir, &relative, limit).await? {
        Ok(file) if file.bytes.len() > limit => Err(UnsupportedProject::OversizedMetadata {
            file: HostPath::resolve(Path::new(name), git_dir)?,
            limit,
        }
        .into()),
        Ok(file) => Ok(Ok(file.bytes)),
        Err(problem) => Ok(Err(problem)),
    }
}

async fn shallow_commits(fs: &dyn FileSystem, git_dir: &HostPath) -> Result<Vec<u8>, LaunchError> {
    match metadata(fs, git_dir, "shallow", SHALLOW_LIMIT).await? {
        Ok(bytes) => Ok(bytes),
        Err(RegularFileProblem::Missing) => Err(malformed(
            ProjectStep::Describe,
            "the repository is shallow but its shallow file is missing",
        )),
        Err(RegularFileProblem::Symlink | RegularFileProblem::NotARegularFile) => Err(
            UnsupportedProject::LinkedShallow(HostPath::resolve(Path::new("shallow"), git_dir)?)
                .into(),
        ),
    }
}

pub(super) async fn read(
    project: &HostProject<'_>,
    fs: &dyn FileSystem,
) -> Result<Facts, LaunchError> {
    let git = &project.git();
    let described = git
        .require(
            ProjectStep::Describe,
            Pace::Quick,
            words(&[
                "rev-parse",
                "--show-object-format",
                "--is-shallow-repository",
            ]),
            Vec::new(),
        )
        .await?;
    let lines: Vec<&[u8]> = described.split(|byte| *byte == b'\n').collect();
    let [format, shallow, ..] = lines.as_slice() else {
        return Err(malformed(
            ProjectStep::Describe,
            "rev-parse printed too few lines",
        ));
    };
    let format = ObjectFormat::parse(&String::from_utf8_lossy(format))
        .ok_or_else(|| malformed(ProjectStep::Describe, "unknown object format"))?;
    let git_dir = project.location.git_dir.clone();
    let shallow = if *shallow == b"true" {
        Some(shallow_commits(fs, &git_dir).await?)
    } else {
        None
    };
    let exclude = metadata(fs, &git_dir, "info/exclude", EXCLUDE_LIMIT)
        .await?
        .ok();
    let attributes = metadata(fs, &git_dir, "info/attributes", ATTRIBUTES_LIMIT)
        .await?
        .ok();
    Ok(Facts {
        head: head(git).await?,
        format,
        shallow,
        identity: Identity {
            name: identity_key(git, "user.name").await?,
            email: identity_key(git, "user.email").await?,
        },
        exclude,
        attributes,
        objects: HostPath::resolve(Path::new("objects"), &git_dir)?,
        git_dir,
    })
}
