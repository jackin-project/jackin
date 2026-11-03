// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use jackin_core::{
    BranchName, FsError, PathError, ProcessError, ProcessStatus, ProjectRoot, TerminalText, plain,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::project::git::{Pace, exit_code, words};
use crate::project::{HostProject, HostProjectError, UnsupportedProject};
use crate::service::Session;

const GITHUB: &str = "github.com";
const REMOTE_BRANCH_PREFIX: &str = "refs/remotes/origin/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubRepo {
    owner: String,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubOrigin {
    pub repo: GithubRepo,
    pub default_branch: Option<BranchName>,
}

#[derive(Debug, Error)]
pub enum OriginError {
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Unsupported(#[from] UnsupportedProject),
    #[error("git exited with code {exit_code}: {output}")]
    Git {
        exit_code: i32,
        output: TerminalText,
    },
    #[error("git printed unusable output: {}", plain(.message))]
    Output { message: String },
}

impl From<HostProjectError> for OriginError {
    fn from(error: HostProjectError) -> OriginError {
        match error {
            HostProjectError::Unsupported(error) => OriginError::Unsupported(error),
            HostProjectError::Fs(error) => OriginError::Fs(error),
            HostProjectError::Process(error) => OriginError::Process(error),
            HostProjectError::Path(error) => OriginError::Path(error),
            HostProjectError::Git { status, output } => OriginError::Git {
                exit_code: exit_code(status),
                output,
            },
            HostProjectError::Output { message } => OriginError::Output { message },
        }
    }
}

fn segment(text: &str, allow_dot: bool) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b'-'
                || (allow_dot && matches!(byte, b'.' | b'_'))
        })
}

impl GithubRepo {
    pub fn parse(url: &str) -> Option<GithubRepo> {
        let path = url
            .strip_prefix("git@github.com:")
            .or_else(|| url.strip_prefix("https://github.com/"))
            .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, name) = path.split_once('/')?;
        (segment(owner, false) && segment(name, true)).then(|| GithubRepo {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for GithubRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "https://{GITHUB}/{}/{}", self.owner, self.name)
    }
}

impl Session {
    pub async fn project_origin(
        &self,
        project: &ProjectRoot,
        cancel: &CancellationToken,
    ) -> Result<Option<GithubOrigin>, OriginError> {
        let git = HostProject::open(&self.deps, project, cancel).await?.git();
        let configured = git
            .capture(
                Pace::Quick,
                words(&["config", "--get-all", "remote.origin.url"]),
                Vec::new(),
            )
            .await?;
        match configured.status {
            ProcessStatus::Exited(0) => {}
            ProcessStatus::Exited(1) => return Ok(None),
            status => {
                return Err(OriginError::Git {
                    exit_code: exit_code(status),
                    output: TerminalText::sanitize(&configured.stderr),
                });
            }
        }
        if configured
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count()
            > 1
        {
            return Ok(None);
        }
        let fetched = git
            .capture(
                Pace::Quick,
                words(&["ls-remote", "--get-url", "origin"]),
                Vec::new(),
            )
            .await?;
        match fetched.status {
            ProcessStatus::Exited(0) => {}
            status => {
                return Err(OriginError::Git {
                    exit_code: exit_code(status),
                    output: TerminalText::sanitize(&fetched.stderr),
                });
            }
        }
        let text = String::from_utf8_lossy(&fetched.stdout);
        let Some(repo) = GithubRepo::parse(text.trim()) else {
            return Ok(None);
        };
        let head = git
            .capture(
                Pace::Quick,
                words(&["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]),
                Vec::new(),
            )
            .await?;
        let default_branch = match head.status {
            ProcessStatus::Exited(0) => String::from_utf8_lossy(&head.stdout)
                .trim()
                .strip_prefix(REMOTE_BRANCH_PREFIX)
                .and_then(|branch| BranchName::parse(branch).ok()),
            _ => None,
        };
        Ok(Some(GithubOrigin {
            repo,
            default_branch,
        }))
    }
}
