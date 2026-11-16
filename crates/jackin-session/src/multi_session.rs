// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{Agent, SandboxName, SandboxPath};
use jackin_openshell::{GatewayError, ProgramRun, ProgramStdin};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

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
