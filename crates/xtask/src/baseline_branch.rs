// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

use thiserror::Error;

use crate::checks::{BASELINE_BRANCH, BASELINE_URL, pinned_revs};
use crate::repo::{Repo, capture};
use crate::text::plain;

const PASSING: [&str; 2] = ["ahead", "identical"];

#[derive(Debug, Error)]
#[error("{}", plain(.0))]
pub struct CompareError(String);

impl CompareError {
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }
}

pub trait CompareSource {
    fn status(&self, rev: &str) -> Result<String, CompareError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GhCompare;

impl CompareSource for GhCompare {
    fn status(&self, rev: &str) -> Result<String, CompareError> {
        let repository = BASELINE_URL.trim_start_matches("https://github.com/");
        let output = capture(Command::new("gh").args([
            "api",
            &format!("repos/{repository}/compare/{rev}...{BASELINE_BRANCH}"),
            "--jq",
            ".status",
        ]))
        .map_err(|error| CompareError::new(format!("cannot start gh: {error}")))?;
        if !output.status.success() {
            return Err(CompareError::new(
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinStatus {
    pub rev: String,
    pub status: String,
}

#[derive(Debug, Error)]
pub enum BaselineBranchError {
    #[error("the workspace pins no baseline rev")]
    NoPin,
    #[error("cannot compare the rev {} with {BASELINE_BRANCH}: {reason}", plain(.rev))]
    Compare { rev: String, reason: CompareError },
    #[error(
        "the rev {} is not on {BASELINE_BRANCH}: the branch is {} against it, not ahead or identical",
        plain(.rev),
        status_text(.status)
    )]
    NotOnBranch { rev: String, status: String },
}

fn status_text(status: &str) -> String {
    if status.is_empty() {
        "an empty reply".to_owned()
    } else {
        plain(status)
    }
}

pub fn baseline_branch(
    repo: &Repo,
    source: &dyn CompareSource,
) -> Result<Vec<PinStatus>, BaselineBranchError> {
    let revs = pinned_revs(repo);
    if revs.is_empty() {
        return Err(BaselineBranchError::NoPin);
    }
    let mut statuses = Vec::new();
    for rev in revs {
        let status = source
            .status(&rev)
            .map_err(|reason| BaselineBranchError::Compare {
                rev: rev.clone(),
                reason,
            })?;
        if !PASSING.contains(&status.as_str()) {
            return Err(BaselineBranchError::NotOnBranch { rev, status });
        }
        statuses.push(PinStatus { rev, status });
    }
    Ok(statuses)
}
