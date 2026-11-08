// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{Agent, AgentInstances, SandboxName, SandboxPath, plain};
use jackin_openshell::{GatewayError, ProgramOutcome, ProgramRun, ProgramStdin};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::AgentMetadataError;
use crate::service::Session;

#[derive(Debug, Error)]
pub enum IntegrateError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    AgentMetadata(#[from] AgentMetadataError),
    #[error("merge of branch {} stopped with a conflict", plain(.branch))]
    Conflict { branch: String },
    #[error("agent {} is not in this sandbox", plain(.agent))]
    UnknownAgent { agent: String },
}

#[derive(Clone)]
struct WorktreeRecord {
    path: String,
    branch: Option<String>,
}

fn protocol(message: impl ToString) -> GatewayError {
    GatewayError::Protocol {
        message: message.to_string(),
    }
}

fn parse_worktrees(output: &[u8]) -> Result<Vec<WorktreeRecord>, GatewayError> {
    let text =
        std::str::from_utf8(output).map_err(|_| protocol("git worktree list was not UTF-8"))?;
    let mut records = Vec::new();
    let mut current: Option<WorktreeRecord> = None;
    for field in text.split('\0') {
        if field.is_empty() {
            if let Some(record) = current.take() {
                records.push(record);
            }
            continue;
        }
        if let Some(path) = field.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            if path.is_empty() {
                return Err(protocol("git worktree list returned an empty path"));
            }
            current = Some(WorktreeRecord {
                path: path.to_owned(),
                branch: None,
            });
        } else if let Some(branch) = field.strip_prefix("branch ") {
            let record = current
                .as_mut()
                .ok_or_else(|| protocol("git worktree list returned a branch without a path"))?;
            record.branch = Some(branch.to_owned());
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    if records.is_empty() {
        return Err(protocol("git worktree list returned no worktrees"));
    }
    Ok(records)
}

fn physical_sandbox_path(
    physical_project: &str,
    path: &SandboxPath,
) -> Result<String, GatewayError> {
    let project = SandboxPath::from_static("/sandbox/project");
    let prefix = physical_project
        .strip_suffix(project.as_str())
        .ok_or_else(|| protocol("git returned a project path outside /sandbox/project"))?;
    Ok(format!("{prefix}{}", path.as_str()))
}

impl Session {
    async fn run_git(
        &self,
        sandbox: &SandboxName,
        args: Vec<String>,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        self.deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::from_static("/usr/bin/git"),
                    args,
                    workdir: Some(SandboxPath::from_static("/sandbox/project")),
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout,
                },
                cancel,
            )
            .await
    }

    async fn verify_worktree(
        &self,
        sandbox: &SandboxName,
        path: &SandboxPath,
        expected_physical_path: &str,
        expected_branch: &str,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let outcome = self
            .run_git(
                sandbox,
                vec![
                    "-C".to_owned(),
                    path.as_str().to_owned(),
                    "rev-parse".to_owned(),
                    "--show-toplevel".to_owned(),
                    "--symbolic-full-name".to_owned(),
                    "HEAD".to_owned(),
                ],
                Duration::from_secs(30),
                cancel,
            )
            .await?;
        if outcome.exit_code != 0 {
            return Err(protocol(format!(
                "git could not verify worktree {} (exit {})",
                path.as_str(),
                outcome.exit_code
            )));
        }
        let output = std::str::from_utf8(&outcome.stdout)
            .map_err(|_| protocol("git worktree identity was not UTF-8"))?;
        let mut lines = output.lines();
        let found_path = lines.next().unwrap_or_default();
        let found_branch = lines.next().unwrap_or_default();
        if found_path != expected_physical_path
            || found_branch != expected_branch
            || lines.next().is_some()
        {
            return Err(protocol(format!(
                "worktree {} has a different path or branch",
                path.as_str()
            )));
        }
        Ok(())
    }

    async fn ensure_worktree(
        &self,
        sandbox: &SandboxName,
        path: &SandboxPath,
        branch: &str,
        physical_project: &str,
        records: &mut Vec<WorktreeRecord>,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let branch_ref = format!("refs/heads/{branch}");
        let physical_path = physical_sandbox_path(physical_project, path)?;
        let existing = records.iter().find(|record| record.path == physical_path);
        if existing.is_some_and(|record| record.branch.as_deref() != Some(&branch_ref)) {
            return Err(protocol(format!(
                "worktree {} is registered on a different branch",
                path.as_str()
            )));
        }
        if existing.is_some() {
            return self
                .verify_worktree(sandbox, path, &physical_path, &branch_ref, cancel)
                .await;
        }
        if records
            .iter()
            .any(|record| record.branch.as_deref() == Some(branch_ref.as_str()))
        {
            return Err(protocol(format!(
                "branch {branch} is already checked out at another path"
            )));
        }
        let reference = self
            .run_git(
                sandbox,
                vec![
                    "show-ref".to_owned(),
                    "--verify".to_owned(),
                    "--quiet".to_owned(),
                    branch_ref.clone(),
                ],
                Duration::from_secs(30),
                cancel,
            )
            .await?;
        let args = match reference.exit_code {
            0 => vec![
                "worktree".to_owned(),
                "add".to_owned(),
                path.as_str().to_owned(),
                branch.to_owned(),
            ],
            1 => vec![
                "worktree".to_owned(),
                "add".to_owned(),
                "-b".to_owned(),
                branch.to_owned(),
                path.as_str().to_owned(),
                "HEAD".to_owned(),
            ],
            code => {
                return Err(protocol(format!(
                    "git could not inspect branch {branch} (exit {code})"
                )));
            }
        };
        let outcome = self
            .run_git(sandbox, args, Duration::from_secs(120), cancel)
            .await?;
        if outcome.exit_code != 0 {
            return Err(protocol(format!(
                "git worktree add for branch {branch} exited {}",
                outcome.exit_code
            )));
        }
        records.push(WorktreeRecord {
            path: physical_path.clone(),
            branch: Some(branch_ref.clone()),
        });
        self.verify_worktree(sandbox, path, &physical_path, &branch_ref, cancel)
            .await
    }

    pub async fn setup_worktrees(
        &self,
        sandbox: &SandboxName,
        instances: &AgentInstances,
        lead: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        if instances.len() <= 1 {
            return Ok(());
        }
        let probe = self
            .run_git(
                sandbox,
                vec![
                    "rev-parse".to_owned(),
                    "--verify".to_owned(),
                    "HEAD".to_owned(),
                ],
                Duration::from_secs(30),
                cancel,
            )
            .await?;
        let physical = self
            .run_git(
                sandbox,
                vec!["rev-parse".to_owned(), "--show-toplevel".to_owned()],
                Duration::from_secs(30),
                cancel,
            )
            .await?;
        if physical.exit_code != 0 {
            return Err(protocol(format!(
                "git could not resolve the project root (exit {})",
                physical.exit_code
            )));
        }
        let physical_project = std::str::from_utf8(&physical.stdout)
            .map_err(|_| protocol("git project path was not UTF-8"))?
            .trim_end_matches('\n')
            .to_owned();
        let listed = self
            .run_git(
                sandbox,
                vec![
                    "worktree".to_owned(),
                    "list".to_owned(),
                    "--porcelain".to_owned(),
                    "-z".to_owned(),
                ],
                Duration::from_secs(30),
                cancel,
            )
            .await?;
        if listed.exit_code != 0 {
            return Err(protocol(format!(
                "git worktree list exited {}",
                listed.exit_code
            )));
        }
        let mut records = parse_worktrees(&listed.stdout)?;
        if !records.iter().any(|record| record.path == physical_project) {
            return Err(protocol("git worktree list omitted the project root"));
        }
        let head = if probe.exit_code == 0 {
            crate::multi::HeadState::Born
        } else {
            crate::multi::HeadState::Unborn
        };
        let layout = crate::multi::plan_multi(instances, lead, None, head).map_err(protocol)?;
        let mkdir = SandboxPath::from_static("/usr/bin/mkdir");
        for directory in layout.directories.iter().filter(|path| {
            path.as_str().starts_with("/sandbox/.jackin/task")
                || path.as_str().starts_with("/sandbox/.jackin/run/")
        }) {
            self.mkdir(sandbox, &mkdir, directory, cancel).await?;
        }
        for home in &layout.homes {
            self.mkdir(sandbox, &mkdir, home, cancel).await?;
        }
        for worktree in layout.worktrees.iter().filter(|item| item.agent != lead) {
            if worktree.worktree {
                let parent = worktree.path.as_str().rsplit_once('/').map_or_else(
                    || "/sandbox/work".to_owned(),
                    |(parent, _)| parent.to_owned(),
                );
                let parent = SandboxPath::parse(&parent)
                    .unwrap_or_else(|_| SandboxPath::from_static("/sandbox/work"));
                self.mkdir(sandbox, &mkdir, &parent, cancel).await?;
                let branch = worktree.branch.clone().unwrap_or_default();
                self.ensure_worktree(
                    sandbox,
                    &worktree.path,
                    &branch,
                    &physical_project,
                    &mut records,
                    cancel,
                )
                .await?;
            } else {
                self.mkdir(sandbox, &mkdir, &worktree.path, cancel).await?;
            }
        }
        Ok(())
    }

    async fn mkdir(
        &self,
        sandbox: &SandboxName,
        mkdir: &SandboxPath,
        path: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let outcome = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: mkdir.clone(),
                    args: vec!["-p".to_owned(), path.as_str().to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        if outcome.exit_code != 0 {
            return Err(GatewayError::Protocol {
                message: format!("mkdir {} exited {}", path.as_str(), outcome.exit_code),
            });
        }
        Ok(())
    }

    pub async fn stop_instance(
        &self,
        expected: &crate::AgentMetadataIdentity,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), IntegrateError> {
        let sandbox = &expected.sandbox;
        let view = self
            .find_managed(sandbox, cancel)
            .await?
            .ok_or(AgentMetadataError::Replaced)?;
        self.revalidate_agent_identity(expected, cancel).await?;
        let metadata = self.agent_metadata_for_view(&view, cancel).await?;
        if metadata.identity != *expected {
            return Err(AgentMetadataError::Replaced.into());
        }
        if !metadata.instances.iter().any(|item| item.agent == agent) {
            return Err(AgentMetadataError::UnknownAgent { agent }.into());
        }
        self.revalidate_agent_identity(expected, cancel).await?;
        let program = SandboxPath::from_static("/usr/bin/pkill");
        let _outcome = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program,
                    args: vec!["-x".to_owned(), agent.as_str().to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        Ok(())
    }

    pub async fn integrate(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<Vec<String>, IntegrateError> {
        let view = self
            .find_managed(sandbox, cancel)
            .await?
            .ok_or(AgentMetadataError::Replaced)?;
        let metadata = self.agent_metadata_for_view(&view, cancel).await?;
        let lead = metadata.lead_agent();
        let mut merged = Vec::new();
        for instance in metadata.instances.iter() {
            if instance.agent == lead {
                continue;
            }
            let current_view = self
                .find_managed(sandbox, cancel)
                .await?
                .ok_or(AgentMetadataError::Replaced)?;
            let current = self.agent_metadata_for_view(&current_view, cancel).await?;
            if current.identity != metadata.identity {
                return Err(AgentMetadataError::Replaced.into());
            }
            if !current
                .instances
                .iter()
                .any(|item| item.agent == instance.agent)
                || current.lead_agent() != lead
            {
                return Err(AgentMetadataError::Replaced.into());
            }
            self.revalidate_agent_identity(&metadata.identity, cancel)
                .await?;
            let branch = format!("agent/{}", instance.agent.as_str());
            let outcome = self
                .deps
                .gateway
                .run_program(
                    &ProgramRun {
                        sandbox: sandbox.clone(),
                        program: SandboxPath::from_static("/usr/bin/git"),
                        args: vec!["merge".to_owned(), "--no-edit".to_owned(), branch.clone()],
                        workdir: Some(SandboxPath::from_static("/sandbox/project")),
                        env: BTreeMap::new(),
                        stdin: ProgramStdin::Closed,
                        touch_on_success: None,
                        timeout: Duration::from_secs(120),
                    },
                    cancel,
                )
                .await?;
            if outcome.exit_code != 0 {
                return Err(IntegrateError::Conflict { branch });
            }
            merged.push(branch);
        }
        Ok(merged)
    }
}

impl Session {
    pub async fn restart_instance(
        &self,
        expected: &crate::AgentMetadataIdentity,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), IntegrateError> {
        self.stop_instance(expected, agent, cancel).await
    }
}
