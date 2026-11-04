// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{HostPath, ProcessStatus, TerminalText};

use crate::launch::LaunchError;
use crate::project::facts::Head;
use crate::project::git::{HostGit, Pace, failure, words};
use crate::project::{ProjectStep, UnsupportedProject};

async fn has_refs(git: &HostGit<'_>) -> Result<bool, LaunchError> {
    let listed = git
        .require(
            ProjectStep::Bundle,
            Pace::Quick,
            words(&[
                "for-each-ref",
                "--count=1",
                "--exclude=refs/heads/jackin/**",
                "--format=%(refname)",
                "refs/heads",
                "refs/tags",
                "refs/remotes",
            ]),
            Vec::new(),
        )
        .await?;
    Ok(!listed.is_empty())
}

pub(super) async fn create(
    git: &HostGit<'_>,
    head: &Head,
    file: &HostPath,
) -> Result<Option<HostPath>, LaunchError> {
    if !head.is_born() && !has_refs(git).await? {
        return Ok(None);
    }
    let mut args = words(&["--no-lazy-fetch", "bundle", "create", "--quiet"]);
    args.push(file.as_path().display().to_string());
    if head.is_born() {
        args.push("HEAD".to_owned());
    }
    args.extend(words(&[
        "--exclude=jackin/*",
        "--branches",
        "--tags",
        "--remotes",
    ]));
    let output = git.capture(Pace::Long, args, Vec::new()).await?;
    match output.status {
        ProcessStatus::Exited(0) => Ok(Some(file.clone())),
        ProcessStatus::Exited(_) => Err(UnsupportedProject::PartialClone {
            output: TerminalText::sanitize(&output.stderr),
        }
        .into()),
        ProcessStatus::Signaled(_) => Err(failure(ProjectStep::Bundle, &output)),
    }
}
