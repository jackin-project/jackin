// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod account;
mod agent;
mod attach;
mod build;
pub(crate) mod console;
mod doctor;
mod export;
mod gateway;
mod inspect;
mod load;
mod ls;
mod power;
mod progress;
mod purge;
mod role;
mod shell;
pub(crate) mod usage;
mod workspace;

use crate::app::App;
use jackin_openshell::PowerAction;

use crate::cli::Command;
use crate::error::CommandError;
use jackin_core::{Agent, AgentInstances};
use jackin_session::{AgentMetadataError, VerifiedAgentMetadata};
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum AgentMetadataReport {
    Verified {
        lead_agent: Agent,
        instances: AgentInstances,
    },
    Unavailable {
        reason: String,
    },
}

impl From<Result<VerifiedAgentMetadata, AgentMetadataError>> for AgentMetadataReport {
    fn from(result: Result<VerifiedAgentMetadata, AgentMetadataError>) -> Self {
        match result {
            Ok(metadata) => AgentMetadataReport::Verified {
                lead_agent: metadata.lead_agent(),
                instances: metadata.instances,
            },
            Err(error) => AgentMetadataReport::Unavailable {
                reason: error.to_string(),
            },
        }
    }
}

impl AgentMetadataReport {
    pub(crate) fn human(&self) -> String {
        match self {
            AgentMetadataReport::Verified {
                lead_agent,
                instances,
            } => format!(
                "verified lead={} agents={}",
                lead_agent.as_str(),
                instances
                    .iter()
                    .map(|instance| instance.agent.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            AgentMetadataReport::Unavailable { reason } => {
                format!("unavailable: {reason}")
            }
        }
    }
}

pub(crate) async fn dispatch(app: &mut App, command: Command) -> Result<(), CommandError> {
    match command {
        Command::Gateway(command) => gateway::run(app, command).await,
        Command::Account(command) => account::run(app, command).await,
        Command::Doctor => doctor::run(app).await,
        Command::Build(args) => build::run(app, args).await,
        Command::Load(args) => load::run(app, *args).await,
        Command::Attach(args) => attach::run(app, args).await,
        Command::Agent(command) => agent::run(app, command).await,
        Command::Integrate(arg) => agent::integrate(app, arg).await,
        Command::Shell(arg) => shell::run(app, arg).await,
        Command::Export(arg) => export::run(app, arg).await,
        Command::Stop(arg) => power::run(app, arg, PowerAction::Stop).await,
        Command::Start(arg) => power::run(app, arg, PowerAction::Start).await,
        Command::Purge(args) => purge::run(app, args).await,
        Command::Ls(arg) => ls::run(app, &arg).await,
        Command::Usage(arg) => usage::run(app, &arg).await,
        Command::Inspect(args) => inspect::run(app, args).await,
        Command::Role(command) => role::run(app, command).await,
        Command::Workspace(command) => workspace::run(app, command).await,
    }
}
