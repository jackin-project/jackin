// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{Agent, AgentInstances, EnvName, SandboxName, SandboxPath};
use jackin_openshell::{GatewayError, ProgramRun, ProgramStdin};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::multi::ExecLaunch;
use crate::service::Session;

#[derive(Debug, Error)]
pub enum IntegrateError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("merge of branch {branch} stopped with a conflict")]
    Conflict { branch: String },
    #[error("agent {agent} is not in this sandbox")]
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

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

impl Session {
    pub async fn start_instance_exec(&self, sandbox: &SandboxName, launch: &ExecLaunch, cancel: &CancellationToken) -> Result<(), GatewayError> {
        let agent = launch.agent.as_str();
        let run_dir = format!("/sandbox/.jackin/run/{agent}");
        let output = format!("{run_dir}/output");
        let command = launch.argv.iter().map(|part| shell_quote(part)).collect::<Vec<_>>().join(" ");
        let script = format!("mkdir -p {} && setsid nohup {command} </dev/null >{} 2>&1 &", shell_quote(&run_dir), shell_quote(&output));
        let mut env = BTreeMap::new();
        env.insert(EnvName::parse("HOME").map_err(|_| GatewayError::Protocol { message: "invalid HOME".to_owned() })?, launch.home.as_str().to_owned());
        if launch.agent == Agent::Codex {
            env.insert(EnvName::parse("CODEX_HOME").map_err(|_| GatewayError::Protocol { message: "invalid CODEX_HOME".to_owned() })?, format!("{}/.codex", launch.home.as_str()));
        }
        let outcome = self.deps.gateway.run_program(&ProgramRun { sandbox: sandbox.clone(), program: SandboxPath::from_static("/bin/sh"), args: vec!["-c".to_owned(), script], workdir: Some(launch.workdir.clone()), env, stdin: ProgramStdin::Closed, touch_on_success: None, timeout: Duration::from_secs(30) }, cancel).await?;
        if outcome.exit_code != 0 {
            return Err(GatewayError::Protocol { message: format!("starting instance {agent} exited {}", outcome.exit_code) });
        }
        Ok(())
    }

    pub async fn restart_instance(&self, sandbox: &SandboxName, agent: Agent, cancel: &CancellationToken) -> Result<(), IntegrateError> {
        self.stop_instance(sandbox, agent, cancel).await?;
        let spec = agent.spec();
        let launch = ExecLaunch { agent, home: SandboxPath::parse(&format!("/sandbox/.jackin/homes/{}", agent.as_str())).map_err(|error| GatewayError::Protocol { message: error.to_string() })?, workdir: SandboxPath::parse(&format!("/sandbox/work/{}/project", agent.as_str())).map_err(|error| GatewayError::Protocol { message: error.to_string() })?, argv: spec.launch_argv(), provider: spec.provider.as_str().to_owned(), account: String::new(), model: spec.default_model.to_owned() };
        self.start_instance_exec(sandbox, &launch, cancel).await?;
        Ok(())
    }
}
