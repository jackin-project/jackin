// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ChangedFile, changed_files, unpushed_commit_count};
use jackin_core::{CommandRunner, RunOptions};
use jackin_process::{ExecRequest, StdioMode};
use jackin_protocol::{CapsuleConfig, EXIT_ACTION_PATH, ExitAction};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyRepo {
    pub path: String,
    pub changed: Vec<ChangedFile>,
    pub unpushed: usize,
}

impl DirtyRepo {
    #[must_use]
    pub fn label(&self) -> &str {
        self.path
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or(self.path.as_str())
    }

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

struct GitRunner;

impl CommandRunner for GitRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        std::future::ready(()).await;
        let mut request = ExecRequest::new(program, args)
            .stdout_mode(StdioMode::Null)
            .stderr_mode(StdioMode::Null);
        if let Some(dir) = cwd {
            request = request.cwd(dir);
        }
        let result = crate::process_telemetry::exec_sync(&request)?;
        if !result.success {
            anyhow::bail!("Git command exited unsuccessfully");
        }
        Ok(())
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        std::future::ready(()).await;
        let mut request = ExecRequest::new(program, args);
        if let Some(dir) = cwd {
            request = request.cwd(dir);
        }
        let output = crate::process_telemetry::exec_sync(&request)?;
        if !output.success {
            anyhow::bail!("Git command exited unsuccessfully");
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

#[must_use]
pub fn policy_is_ask(config: &CapsuleConfig) -> bool {
    config.dirty_exit_policy.as_deref().unwrap_or("ask") == "ask"
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitDecision {
    Drain,
    DrainWithAction(ExitAction),
    ShowModal(Vec<DirtyRepo>),
}

pub async fn decide_exit(config: &CapsuleConfig) -> ExitDecision {
    let dirty = assess_dirty(config).await;
    if dirty.is_empty() {
        return ExitDecision::Drain;
    }
    match config.dirty_exit_policy.as_deref().unwrap_or("ask") {
        "keep" => ExitDecision::DrainWithAction(ExitAction::Keep),
        "discard" => ExitDecision::DrainWithAction(ExitAction::Discard),
        _ => ExitDecision::ShowModal(dirty),
    }
}

pub fn write_exit_action(action: ExitAction) -> std::io::Result<()> {
    write_exit_action_to(Path::new(EXIT_ACTION_PATH), action)
}

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
