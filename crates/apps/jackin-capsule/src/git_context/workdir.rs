// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::record_recovered_degradation;

use std::ffi::OsStr;
use std::path::Path;

use std::time::Duration;

use crate::util::{WaitOutcome, command_stdout_trimmed_with_timeout, wait_child_with_timeout};

pub(crate) const GIT_CONTEXT_COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);
pub(crate) const GH_PULL_REQUEST_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

pub(crate) struct WorkdirContext {
    pub(crate) is_git_repo: bool,
    pub(crate) git_available: bool,
    pub(crate) gh_available: bool,

    pub(crate) default_branch: Option<String>,
}

impl WorkdirContext {
    pub(crate) fn resolve(workdir: &Path) -> Self {
        let git_available = command_in_path("git");
        let gh_available = command_in_path("gh");

        let git_metadata = workdir.join(".git");
        let has_git_metadata = match git_metadata.try_exists() {
            Ok(present) => present,
            Err(_error) => {
                record_recovered_degradation();
                false
            }
        };
        let is_git_repo =
            has_git_metadata || (git_available && workdir_is_inside_git_tree(workdir));
        let default_branch = if is_git_repo {
            resolve_default_branch(workdir)
        } else {
            None
        };
        Self {
            is_git_repo,
            git_available,
            gh_available,
            default_branch,
        }
    }

    pub(crate) fn is_default_branch(&self, branch: &str) -> bool {
        if branch.is_empty() {
            return true;
        }
        if let Some(default) = self.default_branch.as_deref() {
            return branch == default;
        }
        matches!(branch, "main" | "master")
    }
}

pub(crate) fn command_in_path(name: &str) -> bool {
    let request = jackin_process::ExecRequest::new(name, ["--version"])
        .stdout_mode(jackin_process::StdioMode::Null)
        .stderr_mode(jackin_process::StdioMode::Null);
    let Ok((operation, mut child)) = crate::process_telemetry::spawn_sync(&request) else {
        return false;
    };
    match wait_child_with_timeout(&mut child, GIT_CONTEXT_COMMAND_TIMEOUT) {
        WaitOutcome::Exited(status) if status.success() => {
            operation.complete_status(status, &[0]);
            true
        }
        WaitOutcome::Exited(status) => {
            operation.complete_status(status, &[0]);
            false
        }
        WaitOutcome::Reaped => {
            operation.complete_reaped();
            true
        }
        WaitOutcome::Failed => {
            operation.complete_io_failure();
            false
        }
        WaitOutcome::TimedOut => {
            operation.complete_timeout();
            false
        }
    }
}

pub(crate) fn git_capture_at_workdir(workdir: &Path, args: &[&str]) -> Option<String> {
    let command_args = std::iter::once(OsStr::new("-C"))
        .chain(std::iter::once(workdir.as_os_str()))
        .chain(args.iter().map(OsStr::new));
    let request = jackin_process::ExecRequest::new("git", command_args)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Null);
    command_stdout_trimmed_with_timeout(&request, GIT_CONTEXT_COMMAND_TIMEOUT)
}

pub(crate) fn resolve_default_branch(workdir: &Path) -> Option<String> {
    let raw = git_capture_at_workdir(
        workdir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )?;
    raw.strip_prefix("origin/").map(ToOwned::to_owned)
}

pub(crate) fn workdir_is_inside_git_tree(workdir: &Path) -> bool {
    git_capture_at_workdir(workdir, &["rev-parse", "--is-inside-work-tree"])
        .is_some_and(|value| value == "true")
}
