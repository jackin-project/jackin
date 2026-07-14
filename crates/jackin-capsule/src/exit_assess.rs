// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! In-container dirty assessment for the last-session-exit modal.
//!
//! Runs git synchronously via `std::process` (the capsule's tokio build carries
//! no `process` feature) behind the shared [`jackin_core::CommandRunner`]
//! seam, so the modal's trigger uses the same detection vocabulary as host
//! cleanup. Only the no-live-session exit path calls this, where briefly
//! blocking the single-threaded runtime is acceptable.
//!
//! The container does not have the host's per-worktree `base_commit`, so the
//! trigger uses the base-commit-free checks: uncommitted/untracked changes
//! ([`changed_files`]) plus an unpushed-commit count
//! ([`unpushed_commit_count`]). The host still runs the authoritative
//! safe-to-delete assessment for cleanup; this only decides whether to warn.

#[cfg(test)]
mod tests;

use jackin_core::{ChangedFile, changed_files, unpushed_commit_count};
use jackin_core::{CommandRunner, RunOptions};
use jackin_protocol::{CapsuleConfig, EXIT_ACTION_PATH, ExitAction};
use std::path::Path;
use std::process::Stdio;

/// One isolated worktree carrying uncommitted or unpushed work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyRepo {
    /// Container-side worktree path.
    pub path: String,
    /// Uncommitted/untracked changed files (for the summary count + Inspect).
    pub changed: Vec<ChangedFile>,
    /// Count of unpushed commits across local branches.
    pub unpushed: usize,
}

impl DirtyRepo {
    /// Short repo label — the final non-empty path component, falling back to
    /// the whole path.
    #[must_use]
    pub fn label(&self) -> &str {
        self.path
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or(self.path.as_str())
    }

    /// One-line modal summary, e.g. `jackin   2 changed · 1 unpushed`. Omits a
    /// zero count; at least one count is always non-zero (the repo is dirty).
    #[must_use]
    pub fn summary_line(&self) -> String {
        let mut parts = Vec::new();
        if !self.changed.is_empty() {
            parts.push(format!("{} changed", self.changed.len()));
        }
        if self.unpushed > 0 {
            parts.push(format!("{} unpushed", self.unpushed));
        }
        format!("{}   {}", self.label(), parts.join(" · "))
    }
}

/// Synchronous in-container git runner. The assessment helpers only call
/// `capture`; `run` is implemented for trait completeness.
struct GitRunner;

impl CommandRunner for GitRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let status = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?
            .wait()?;
        if !status.success() {
            anyhow::bail!("{program} exited with {status}");
        }
        Ok(())
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let output = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()?;
        if !output.status.success() {
            anyhow::bail!(
                "{program} {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }
}

/// Whether the dirty-exit modal should ever be shown for this exit: only when
/// the resolved policy is `ask`. `keep`/`discard` skip the modal and exit
/// straight to the host executing that policy.
#[must_use]
pub fn policy_is_ask(config: &CapsuleConfig) -> bool {
    config.dirty_exit_policy.as_deref().unwrap_or("ask") == "ask"
}

/// Assess every isolated worktree in `config`; return those with uncommitted or
/// unpushed work. Empty when nothing is dirty (or there are no isolated mounts).
pub async fn assess_dirty(config: &CapsuleConfig) -> Vec<DirtyRepo> {
    let mut runner = GitRunner;
    let mut dirty = Vec::new();
    for path in &config.isolated_worktrees {
        let changed = changed_files(path, &mut runner).await;
        let unpushed = unpushed_commit_count(path, &mut runner).await;
        if !changed.is_empty() || unpushed > 0 {
            dirty.push(DirtyRepo {
                path: path.clone(),
                changed,
                unpushed,
            });
        }
    }
    dirty
}

/// What the daemon does when the last live session exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitDecision {
    /// No dirty work — drain and exit cleanly (no exit-action file written).
    Drain,
    /// Dirty work under policy `keep`/`discard` — record the action for the host
    /// (no prompt), then drain.
    DrainWithAction(ExitAction),
    /// Dirty work under policy `ask` — show the in-capsule modal for these repos.
    ShowModal(Vec<DirtyRepo>),
}

/// Decide what to do when the last live session exits. A clean workspace always
/// drains. Dirty work resolves by policy: `ask` (default) shows the modal;
/// `keep`/`discard` record that action for the host and drain with no prompt.
pub async fn decide_exit(config: &CapsuleConfig) -> ExitDecision {
    let dirty = assess_dirty(config).await;
    if dirty.is_empty() {
        return ExitDecision::Drain;
    }
    match config.dirty_exit_policy.as_deref().unwrap_or("ask") {
        "keep" => ExitDecision::DrainWithAction(ExitAction::Keep),
        "discard" => ExitDecision::DrainWithAction(ExitAction::Discard),
        // "ask" and any unknown value fall back to the conservative prompt.
        _ => ExitDecision::ShowModal(dirty),
    }
}

/// Record the operator's dirty-exit choice for the host to execute on cleanup.
/// Writes to [`EXIT_ACTION_PATH`]; the host reads it via `serde_json`.
///
/// # Errors
/// Returns the underlying I/O error if the state file cannot be written.
pub fn write_exit_action(action: ExitAction) -> std::io::Result<()> {
    write_exit_action_to(Path::new(EXIT_ACTION_PATH), action)
}

/// The serialized form the host's `serde` reads back into [`ExitAction`].
fn exit_action_json(action: ExitAction) -> &'static str {
    match action {
        ExitAction::Keep => "\"keep\"",
        ExitAction::Discard => "\"discard\"",
    }
}

fn write_exit_action_to(path: &Path, action: ExitAction) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, exit_action_json(action))
}
