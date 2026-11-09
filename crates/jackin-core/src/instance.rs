// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::agent::Agent;
use crate::ids::{AccountName, Sha256};
use crate::provider::Provider;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInstance {
    pub agent: Agent,
    pub provider: Provider,
    pub account: AccountName,
    pub model: String,
    pub branch: String,
}

impl AgentInstance {
    pub fn new(
        agent: Agent,
        provider: Provider,
        account: AccountName,
        model: String,
    ) -> AgentInstance {
        AgentInstance {
            branch: format!("agent/{}", agent.as_str()),
            agent,
            provider,
            account,
            model,
        }
    }

    pub fn lead(agent: Agent, account: AccountName) -> AgentInstance {
        let spec = agent.spec();
        AgentInstance::new(agent, spec.provider, account, spec.default_model.to_owned())
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

#[derive(Clone, Debug, Eq)]
pub struct AgentInstances {
    items: Vec<AgentInstance>,
    label_override: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AgentInstanceError {
    #[error("a launch needs at least one agent instance")]
    Empty,
    #[error("agent {} is repeated in one launch", .agent.as_str())]
    RepeatedAgent { agent: Agent },
}

impl PartialEq for AgentInstances {
    fn eq(&self, other: &AgentInstances) -> bool {
        if self.label_override.is_some() || other.label_override.is_some() {
            self.agents_label() == other.agents_label()
        } else {
            self.items == other.items
        }
    }
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
        }
        Ok(AgentInstances {
            items,
            label_override: None,
        })
    }

    pub fn single(item: AgentInstance) -> AgentInstances {
        AgentInstances {
            items: vec![item],
            label_override: None,
        }
    }

    pub fn from_label_value(label: String) -> AgentInstances {
        AgentInstances {
            items: Vec::new(),
            label_override: Some(label),
        }
    }

    pub fn lead(&self) -> &AgentInstance {
        &self.items[0]
    }

    pub fn lead_opt(&self) -> Option<&AgentInstance> {
        self.items.first()
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

    pub fn canonical(&self) -> Vec<AgentInstance> {
        let mut items = self.items.clone();
        if let Some((_, secondary)) = items.split_first_mut() {
            secondary.sort_by_key(AgentInstance::item);
        }
        items
    }

    pub fn agents_label(&self) -> String {
        if let Some(label) = &self.label_override {
            return label.clone();
        }
        let items: Vec<String> = self.canonical().iter().map(AgentInstance::item).collect();
        let joined = items.join(",");
        let preimage = if self.items.len() > 1 {
            format!("lead-first-v1:{joined}")
        } else {
            joined
        };
        Sha256::of(preimage.as_bytes()).as_str()[..32].to_owned()
    }

    pub fn agents_json(&self) -> String {
        let entries: Vec<String> = self
            .canonical()
            .iter()
            .map(|item| {
                format!(
                    "{{\"agent\":\"{}\",\"provider\":\"{}\",\"account\":\"{}\",\"model\":\"{}\",\"branch\":\"{}\"}}",
                    json_escape(item.agent.as_str()),
                    json_escape(item.provider.as_str()),
                    json_escape(item.account.as_str()),
                    json_escape(&item.model),
                    json_escape(&item.branch)
                )
            })
            .collect();
        format!("[{}]", entries.join(","))
    }
}

fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

impl Serialize for AgentInstances {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.label_override.is_some() {
            serializer.serialize_str(&self.agents_label())
        } else {
            self.items.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for AgentInstances {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<AgentInstances, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Label(String),
            Items(Vec<AgentInstance>),
        }
        match Form::deserialize(deserializer)? {
            Form::Label(label) => Ok(AgentInstances::from_label_value(label)),
            Form::Items(items) if items.is_empty() => Ok(AgentInstances {
                items,
                label_override: None,
            }),
            Form::Items(items) => AgentInstances::new(items).map_err(serde::de::Error::custom),
        }
    }
}
