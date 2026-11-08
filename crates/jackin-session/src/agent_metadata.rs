// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AGENTS_LABEL, Agent, AgentHash, AgentInstance, AgentInstances, LEAD_AGENT_LABEL, LabelError,
    LaunchId, Phase, SandboxLabels, SandboxName, SandboxPath, SandboxSummary, Timestamp,
};
use jackin_openshell::{GatewayError, SandboxView};
use serde::Serialize;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::service::Session;

pub const AGENT_METADATA_LIMIT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentMetadataIdentity {
    pub sandbox: SandboxName,
    pub created_at: Timestamp,
    pub launch: LaunchId,
    pub agents_hash: AgentHash,
    pub lead_agent: Agent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VerifiedAgentMetadata {
    pub identity: AgentMetadataIdentity,
    pub instances: AgentInstances,
}

impl VerifiedAgentMetadata {
    pub fn lead_agent(&self) -> Agent {
        self.identity.lead_agent
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AgentMetadataError {
    #[error("the sandbox has no creation identity")]
    MissingCreationIdentity,
    #[error("the sandbox disappeared or changed identity while agent metadata was read")]
    Replaced,
    #[error("the sandbox labels needed to verify agent metadata are invalid")]
    InvalidLabels,
    #[error("the sandbox has no valid {AGENTS_LABEL} label")]
    MissingHash,
    #[error("the sandbox has no valid {LEAD_AGENT_LABEL} label")]
    MissingLead,
    #[error("agent metadata is unavailable while the sandbox is {0}")]
    NotReady(Phase),
    #[error("the agent metadata file is missing")]
    MissingFile,
    #[error("the agent metadata file exceeds the {AGENT_METADATA_LIMIT_BYTES}-byte limit")]
    Oversized,
    #[error("the agent metadata file is malformed")]
    Malformed,
    #[error("the agent metadata hash does not match the trusted label")]
    HashMismatch,
    #[error("the trusted lead agent is not in the verified instance list")]
    LeadNotMember,
    #[error("agent {} is not in the verified instance list", .agent.as_str())]
    UnknownAgent { agent: Agent },
    #[error("the gateway could not read agent metadata")]
    Gateway,
    #[error("the agent metadata read timed out")]
    TimedOut,
    #[error("cancelled")]
    Cancelled,
}

fn labels_error(error: LabelError) -> AgentMetadataError {
    match error {
        LabelError::Missing { key: AGENTS_LABEL }
        | LabelError::Invalid {
            key: AGENTS_LABEL, ..
        } => AgentMetadataError::MissingHash,
        LabelError::Missing {
            key: LEAD_AGENT_LABEL,
        }
        | LabelError::Invalid {
            key: LEAD_AGENT_LABEL,
            ..
        } => AgentMetadataError::MissingLead,
        _ => AgentMetadataError::InvalidLabels,
    }
}

fn identity(
    name: &SandboxName,
    created_at: Option<Timestamp>,
    labels: &jackin_core::Labels,
) -> Result<AgentMetadataIdentity, AgentMetadataError> {
    let created_at = created_at.ok_or(AgentMetadataError::MissingCreationIdentity)?;
    let labels = SandboxLabels::from_labels(labels).map_err(labels_error)?;
    Ok(AgentMetadataIdentity {
        sandbox: name.clone(),
        created_at,
        launch: labels.planned.launch,
        agents_hash: labels.planned.agents_hash,
        lead_agent: labels.planned.lead_agent,
    })
}

fn summary_identity(summary: &SandboxSummary) -> Result<AgentMetadataIdentity, AgentMetadataError> {
    Ok(AgentMetadataIdentity {
        sandbox: summary.name.clone(),
        created_at: summary
            .created_at
            .ok_or(AgentMetadataError::MissingCreationIdentity)?,
        launch: summary.labels.planned.launch.clone(),
        agents_hash: summary.labels.planned.agents_hash.clone(),
        lead_agent: summary.labels.planned.lead_agent,
    })
}

fn parse_instances(bytes: &[u8]) -> Result<AgentInstances, AgentMetadataError> {
    if bytes.len() > AGENT_METADATA_LIMIT_BYTES {
        return Err(AgentMetadataError::Oversized);
    }
    std::str::from_utf8(bytes).map_err(|_| AgentMetadataError::Malformed)?;
    let instances: AgentInstances =
        serde_json::from_slice(bytes).map_err(|_| AgentMetadataError::Malformed)?;
    let items: Vec<String> = instances.iter().map(AgentInstance::item).collect();
    let sorted: Vec<String> = instances.sorted().iter().map(AgentInstance::item).collect();
    if items != sorted {
        return Err(AgentMetadataError::Malformed);
    }
    Ok(instances)
}

fn verify_contents(
    identity: &AgentMetadataIdentity,
    bytes: &[u8],
) -> Result<VerifiedAgentMetadata, AgentMetadataError> {
    let instances = parse_instances(bytes)?;
    if instances.hash() != identity.agents_hash {
        return Err(AgentMetadataError::HashMismatch);
    }
    if !instances
        .iter()
        .any(|instance| instance.agent == identity.lead_agent)
    {
        return Err(AgentMetadataError::LeadNotMember);
    }
    Ok(VerifiedAgentMetadata {
        identity: identity.clone(),
        instances,
    })
}

fn gateway_error(error: GatewayError) -> AgentMetadataError {
    match error {
        GatewayError::Cancelled { .. } => AgentMetadataError::Cancelled,
        _ => AgentMetadataError::Gateway,
    }
}

impl Session {
    pub async fn agent_metadata_for_sandbox(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<VerifiedAgentMetadata, AgentMetadataError> {
        if cancel.is_cancelled() {
            return Err(AgentMetadataError::Cancelled);
        }
        let view = self
            .find_managed(sandbox, cancel)
            .await
            .map_err(gateway_error)?
            .ok_or(AgentMetadataError::Replaced)?;
        self.agent_metadata_for_view(&view, cancel).await
    }

    pub async fn agent_metadata(
        &self,
        summary: &SandboxSummary,
        cancel: &CancellationToken,
    ) -> Result<VerifiedAgentMetadata, AgentMetadataError> {
        if cancel.is_cancelled() {
            return Err(AgentMetadataError::Cancelled);
        }
        let expected = summary_identity(summary)?;
        let view = self
            .find_managed(&summary.name, cancel)
            .await
            .map_err(gateway_error)?
            .ok_or(AgentMetadataError::Replaced)?;
        let found = identity(&view.name, view.created_at, &view.labels)?;
        if found != expected {
            return Err(AgentMetadataError::Replaced);
        }
        self.agent_metadata_for_view(&view, cancel).await
    }

    pub(crate) async fn agent_metadata_for_view(
        &self,
        view: &SandboxView,
        cancel: &CancellationToken,
    ) -> Result<VerifiedAgentMetadata, AgentMetadataError> {
        if cancel.is_cancelled() {
            return Err(AgentMetadataError::Cancelled);
        }
        if view.phase != Phase::Ready {
            return Err(AgentMetadataError::NotReady(view.phase));
        }
        let expected = identity(&view.name, view.created_at, &view.labels)?;
        let path = SandboxPath::from_static("/sandbox/.jackin/agents.json");
        let files = self
            .gateway()
            .read_files(&view.name, &[path], cancel)
            .await
            .map_err(gateway_error)?;
        if cancel.is_cancelled() {
            return Err(AgentMetadataError::Cancelled);
        }
        let [Some(bytes)] = files.as_slice() else {
            return Err(AgentMetadataError::MissingFile);
        };
        let verified = verify_contents(&expected, bytes)?;
        self.revalidate_agent_identity(&expected, cancel).await?;
        Ok(verified)
    }

    pub(crate) async fn revalidate_agent_identity(
        &self,
        expected: &AgentMetadataIdentity,
        cancel: &CancellationToken,
    ) -> Result<(), AgentMetadataError> {
        if cancel.is_cancelled() {
            return Err(AgentMetadataError::Cancelled);
        }
        let view = self
            .find_managed(&expected.sandbox, cancel)
            .await
            .map_err(gateway_error)?
            .ok_or(AgentMetadataError::Replaced)?;
        let found = identity(&view.name, view.created_at, &view.labels)?;
        if found != *expected {
            return Err(AgentMetadataError::Replaced);
        }
        if cancel.is_cancelled() {
            return Err(AgentMetadataError::Cancelled);
        }
        Ok(())
    }
}
