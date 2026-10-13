// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workdir and tool probing (`WorkdirContext`).

use super::record_recovered_degradation;

use std::ffi::OsStr;
use std::path::Path;

use std::time::Duration;

use crate::util::{WaitOutcome, command_stdout_trimmed_with_timeout, wait_child_with_timeout};

pub(crate) const GIT_CONTEXT_COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);
pub(crate) const GH_PULL_REQUEST_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

/// One-shot resolution of workdir + tool facts. `gh_available` may
/// flip from false to true when a background PR lookup succeeds (so a
/// startup PATH race doesn't freeze the feature for the daemon
/// lifetime); the other fields are never re-probed.
pub(crate) struct WorkdirContext {
    pub(crate) is_git_repo: bool,
    pub(crate) git_available: bool,
    pub(crate) gh_available: bool,
    /// `origin/HEAD` resolved to a short branch name (`main`, `master`,
    /// `trunk`, `develop`, …). `None` when the workdir is not a git
    /// checkout, when `origin/HEAD` is not set, or when `git
    /// symbolic-ref` fails. Falls back to a `main`/`master` literal
    /// match for branches that look like defaults when this is `None`.
    pub(crate) default_branch: Option<String>,
}

impl WorkdirContext {
    pub(crate) fn resolve(workdir: &Path) -> Self {
        let git_available = command_in_path("git");
        let gh_available = command_in_path("gh");
        // `.git` may be a regular directory (normal checkout) or a
        // file containing `gitdir: …` (worktree / submodule).
        // `try_exists` covers both. Keep this independent of the
        // startup `git --version` probe: the hot branch path can read
        // `.git/HEAD` directly, so a normal checkout can still update
        // chrome even if the subprocess probe fails or runs before the
        // shell has expanded PATH.
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

    /// True when `branch` is the repo's default branch (the chrome bar
    /// stays hidden in that case). Falls back to a literal
    /// `main`/`master`/empty match when `origin/HEAD` is not set so
    /// freshly-cloned-but-not-`gh-repo-set-default`ed repos still
    /// suppress the bar.
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

/// Probe `name --version` once at construction. Stdin/stdout/stderr
/// are nulled so a misbehaving subprocess cannot leak output into the
/// daemon's logs and cannot block on stdin. As PID 1, Capsule has a
/// SIGCHLD zombie reaper that can win the race against Rust's
/// `Child::try_wait`; `ECHILD` after a successful spawn still proves
/// the executable exists, so treat it as available instead of freezing
/// the feature off for the daemon lifetime.
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

/// Bounded by `GIT_CONTEXT_COMMAND_TIMEOUT` so a stalled `git`
/// subprocess against a network-mounted `.git` cannot block the daemon.
pub(crate) fn git_capture_at_workdir(workdir: &Path, args: &[&str]) -> Option<String> {
    let command_args = std::iter::once(OsStr::new("-C"))
        .chain(std::iter::once(workdir.as_os_str()))
        .chain(args.iter().map(OsStr::new));
    let request = jackin_process::ExecRequest::new("git", command_args)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Null);
    command_stdout_trimmed_with_timeout(&request, GIT_CONTEXT_COMMAND_TIMEOUT)
}

/// `git symbolic-ref --short refs/remotes/origin/HEAD` returns
/// `origin/main` (or whatever the default branch is). Strip the
/// `origin/` so it can compare directly against `git branch
/// --show-current` output.
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
