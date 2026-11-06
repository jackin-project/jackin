// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{Agent, AgentInstances, SandboxName, SandboxPath, plain};
use jackin_openshell::{GatewayError, ProgramRun, ProgramStdin};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::service::Session;

#[derive(Debug, Error)]
pub enum IntegrateError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("merge of branch {} stopped with a conflict", plain(.branch))]
    Conflict { branch: String },
    #[error("agent {} is not in this sandbox", plain(.agent))]
    UnknownAgent { agent: String },
}

fn layout_worktree_lead(instances: &AgentInstances) -> Option<Agent> {
    instances.lead_opt().map(|item| item.agent)
}

fn agent_names(json: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = json;
    while let Some(pos) = rest.find("\"agent\":\"") {
        let after = &rest[pos + 9..];
        if let Some(end) = after.find('"') {
            names.push(after[..end].to_owned());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    names
}

impl Session {
    pub async fn setup_worktrees(
        &self,
        sandbox: &SandboxName,
        instances: &AgentInstances,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        if instances.len() <= 1 {
            return Ok(());
        }
        let git = SandboxPath::from_static("/usr/bin/git");
        let project = SandboxPath::from_static("/sandbox/project");
        let probe = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: git.clone(),
                    args: vec![
                        "rev-parse".to_owned(),
                        "--verify".to_owned(),
                        "HEAD".to_owned(),
                    ],
                    workdir: Some(project.clone()),
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        let head = if probe.exit_code == 0 {
            crate::multi::HeadState::Born
        } else {
            crate::multi::HeadState::Unborn
        };
        let layout = crate::multi::plan_multi(instances, None, head);
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
        for worktree in layout
            .worktrees
            .iter()
            .filter(|item| Some(item.agent) != layout_worktree_lead(instances))
        {
            if worktree.worktree {
                let parent = worktree.path.as_str().rsplit_once('/').map_or_else(
                    || "/sandbox/work".to_owned(),
                    |(parent, _)| parent.to_owned(),
                );
                let parent = SandboxPath::parse(&parent)
                    .unwrap_or_else(|_| SandboxPath::from_static("/sandbox/work"));
                self.mkdir(sandbox, &mkdir, &parent, cancel).await?;
                let branch = worktree.branch.clone().unwrap_or_default();
                let outcome = self
                    .deps
                    .gateway
                    .run_program(
                        &ProgramRun {
                            sandbox: sandbox.clone(),
                            program: git.clone(),
                            args: vec![
                                "worktree".to_owned(),
                                "add".to_owned(),
                                "-b".to_owned(),
                                branch.clone(),
                                worktree.path.as_str().to_owned(),
                                "HEAD".to_owned(),
                            ],
                            workdir: Some(project.clone()),
                            env: BTreeMap::new(),
                            stdin: ProgramStdin::Closed,
                            touch_on_success: None,
                            timeout: Duration::from_secs(120),
                        },
                        cancel,
                    )
                    .await?;
                if outcome.exit_code != 0 {
                    return Err(GatewayError::Protocol {
                        message: format!(
                            "git worktree add for branch {branch} exited {}",
                            outcome.exit_code
                        ),
                    });
                }
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

    pub async fn sandbox_agent_names(
        &self,
        sandbox: &SandboxName,
    ) -> Result<Vec<String>, GatewayError> {
        let path = SandboxPath::from_static("/sandbox/.jackin/agents.json");
        let found = self.deps.gateway.read_files(sandbox, &[path]).await?;
        let Some(bytes) = found.into_iter().next().flatten() else {
            return Ok(Vec::new());
        };
        Ok(agent_names(&String::from_utf8_lossy(&bytes)))
    }

    pub async fn stop_instance(
        &self,
        sandbox: &SandboxName,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), IntegrateError> {
        let names = self.sandbox_agent_names(sandbox).await?;
        if !names.is_empty() && !names.iter().any(|name| name == agent.as_str()) {
            return Err(IntegrateError::UnknownAgent {
                agent: agent.as_str().to_owned(),
            });
        }
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
        let names = self.sandbox_agent_names(sandbox).await?;
        let mut merged = Vec::new();
        for (index, name) in names.iter().enumerate() {
            if index == 0 {
                continue;
            }
            let branch = format!("agent/{name}");
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
        sandbox: &SandboxName,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), IntegrateError> {
        self.stop_instance(sandbox, agent, cancel).await
    }
}
