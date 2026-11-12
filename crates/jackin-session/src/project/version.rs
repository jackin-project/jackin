// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::time::Duration;

use jackin_core::{ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program, plain};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

const fn version(major: u32, minor: u32, patch: u32) -> GitVersion {
    GitVersion {
        major,
        minor,
        patch,
    }
}

const GIT_FLOORS: [GitVersion; 7] = [
    version(2, 45, 4),
    version(2, 46, 4),
    version(2, 47, 3),
    version(2, 48, 2),
    version(2, 49, 1),
    version(2, 50, 1),
    version(2, 51, 0),
];

fn required_for(found: GitVersion) -> Option<GitVersion> {
    let own_line = GIT_FLOORS
        .iter()
        .find(|floor| (floor.major, floor.minor) == (found.major, found.minor));
    match own_line {
        Some(floor) => (found < *floor).then_some(*floor),
        None => GIT_FLOORS.iter().find(|floor| **floor > found).copied(),
    }
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Error)]
pub enum HostGitError {
    #[error("host git {found} is older than the required {required}")]
    TooOld {
        found: GitVersion,
        required: GitVersion,
    },
    #[error("host git cannot be used: {}", plain(.detail))]
    Unusable { detail: String },
}

fn number(part: &str) -> Option<u32> {
    let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn parse(text: &str) -> Option<GitVersion> {
    let rest = text.trim().strip_prefix("git version ")?;
    let mut parts = rest.split_whitespace().next()?.split('.');
    let major = number(parts.next()?)?;
    let minor = number(parts.next()?)?;
    let patch = parts.next().map_or(Some(0), number)?;
    Some(GitVersion {
        major,
        minor,
        patch,
    })
}

fn unusable(detail: impl Into<String>) -> HostGitError {
    HostGitError::Unusable {
        detail: detail.into(),
    }
}

pub async fn check_git(
    runner: &dyn ProcessRunner,
    cancel: &CancellationToken,
) -> Result<GitVersion, HostGitError> {
    let spec = ProcessSpec {
        program: Program::Git,
        args: vec!["--version".to_owned()],
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: vec!["GIT_".to_owned()],
        stdin: ProcessStdin::Null,
        timeout: VERSION_TIMEOUT,
    };
    let output = runner
        .run(&spec, None, cancel)
        .await
        .map_err(|error| unusable(error.to_string()))?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(unusable(format!(
            "git --version ended with {:?}",
            output.status
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let found =
        parse(&text).ok_or_else(|| unusable(format!("unrecognized version output {text:?}")))?;
    match required_for(found) {
        Some(required) => Err(HostGitError::TooOld { found, required }),
        None => Ok(found),
    }
}
