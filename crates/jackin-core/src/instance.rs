// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::agent::Agent;
use crate::ids::{AccountName, AgentHash, IdError, ModelName, Sha256};
use crate::provider::Provider;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentInstance {
    pub agent: Agent,
    pub provider: Provider,
    pub account: AccountName,
    pub model: ModelName,
    pub branch: String,
}

impl AgentInstance {
    pub fn new(
        agent: Agent,
        provider: Provider,
        account: AccountName,
        model: ModelName,
    ) -> AgentInstance {
        AgentInstance {
            branch: format!("agent/{}", agent.as_str()),
            agent,
            provider,
            account,
            model,
        }
    }

    pub fn lead(agent: Agent, account: AccountName) -> Result<AgentInstance, IdError> {
        let spec = agent.spec();
        Ok(AgentInstance::new(
            agent,
            spec.provider,
            account,
            ModelName::parse(spec.default_model)?,
        ))
    }

    pub fn item(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.agent.as_str(),
            self.provider.as_str(),
            self.account,
            self.model
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInstances {
    items: Vec<AgentInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AgentInstanceError {
    #[error("a launch needs at least one agent instance")]
    Empty,
    #[error("agent {} is repeated in one launch", .agent.as_str())]
    RepeatedAgent { agent: Agent },
    #[error("agent {} has no catalog tuple for provider {}", .agent.as_str(), .provider.as_str())]
    UnknownTuple { agent: Agent, provider: Provider },
    #[error("agent {} has an invalid branch", .agent.as_str())]
    InvalidBranch { agent: Agent },
}

impl AgentInstances {
    pub fn new(items: Vec<AgentInstance>) -> Result<AgentInstances, AgentInstanceError> {
        if items.is_empty() {
            return Err(AgentInstanceError::Empty);
        }
        let mut seen = BTreeSet::new();
        for item in &items {
            if !seen.insert(item.agent) {
                return Err(AgentInstanceError::RepeatedAgent { agent: item.agent });
            }
            if crate::lookup_tuple(item.agent, item.provider).is_err() {
                return Err(AgentInstanceError::UnknownTuple {
                    agent: item.agent,
                    provider: item.provider,
                });
            }
            if item.branch != format!("agent/{}", item.agent.as_str()) {
                return Err(AgentInstanceError::InvalidBranch { agent: item.agent });
            }
        }
        Ok(AgentInstances { items })
    }

    pub fn single(item: AgentInstance) -> Result<AgentInstances, AgentInstanceError> {
        AgentInstances::new(vec![item])
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &AgentInstance> {
        self.items.iter()
    }

    pub fn as_slice(&self) -> &[AgentInstance] {
        &self.items
    }

    pub fn sorted(&self) -> Vec<AgentInstance> {
        let mut items = self.items.clone();
        items.sort_by_key(AgentInstance::item);
        items
    }

    pub fn hash(&self) -> AgentHash {
        let items: Vec<String> = self.sorted().iter().map(AgentInstance::item).collect();
        let digest = Sha256::of(items.join(",").as_bytes());
        AgentHash::from(&digest)
    }

    pub fn agents_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.sorted())
    }
}

impl Serialize for AgentInstances {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.sorted().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AgentInstances {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AgentInstances, D::Error> {
        let items = Vec::<AgentInstance>::deserialize(deserializer)?;
        AgentInstances::new(items).map_err(serde::de::Error::custom)
    }
}
