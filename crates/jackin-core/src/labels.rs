// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::agent::Agent;
use crate::display::plain;
use crate::docker::{CaFingerprint, DockerLabel};
use crate::ids::{
    AgentHash, IdError, ImageId, ImageLabel, LaunchId, PolicyLabel, RoleName, WorkspaceName,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelKey(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelValue(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labels(BTreeMap<LabelKey, LabelValue>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedLabels {
    pub workspace: WorkspaceName,
    pub role: RoleName,
    pub agents_hash: AgentHash,
    pub lead_agent: Agent,
    pub policy: PolicyLabel,
    pub launch: LaunchId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxLabels {
    pub planned: PlannedLabels,
    pub image: ImageLabel,
    pub docker: DockerLabel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelDifference {
    pub key: LabelKey,
    pub found: LabelValue,
    pub wanted: LabelValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSelector(Vec<(LabelKey, LabelValue)>);

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum LabelError {
    #[error("missing label {key}")]
    Missing { key: &'static str },
    #[error("invalid value for label {key}")]
    Invalid { key: &'static str, source: IdError },
    #[error("unknown agent label {}", plain(.0))]
    UnknownAgent(String),
    #[error("invalid label key {}: {reason}", plain(.key))]
    InvalidKey { key: String, reason: &'static str },
    #[error("invalid value for label {}: {reason}", plain(.key))]
    InvalidValue { key: String, reason: &'static str },
}

const LABEL_KEY_MAX: usize = 253;
const LABEL_VALUE_MAX: usize = 63;
const MANAGED: &str = "jackin.dev/managed";
const WORKSPACE: &str = "jackin.dev/workspace";
const ROLE: &str = "jackin.dev/role";
pub const AGENTS_LABEL: &str = "jackin.dev/agents";
pub const LEAD_AGENT_LABEL: &str = "jackin.dev/lead-agent";
const POLICY: &str = "jackin.dev/policy";
const IMAGE: &str = "jackin.dev/image";
const LAUNCH: &str = "jackin.dev/launch";
const DOCKER: &str = "jackin.dev/docker";
const ENGINE_PREFIX: &str = "engine-";
const ABSENT: &str = "(none)";

impl LabelKey {
    pub fn from_static(text: &'static str) -> LabelKey {
        LabelKey(text.to_owned())
    }
}

impl LabelValue {
    pub fn from_static(text: &'static str) -> LabelValue {
        LabelValue(text.to_owned())
    }
}

impl fmt::Display for LabelKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for LabelValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for LabelDifference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is {} on the sandbox and {} in this launch",
            self.key, self.found, self.wanted
        )
    }
}

fn pair(key: &str, value: &str) -> (LabelKey, LabelValue) {
    (LabelKey(key.to_owned()), LabelValue(value.to_owned()))
}

impl PlannedLabels {
    pub fn complete(self, image: &ImageId, docker: DockerLabel) -> SandboxLabels {
        SandboxLabels {
            planned: self,
            image: image.label_value(),
            docker,
        }
    }
}

impl SandboxLabels {
    pub fn to_labels(&self) -> Labels {
        let planned = &self.planned;
        Labels(BTreeMap::from([
            pair(MANAGED, "true"),
            pair(WORKSPACE, planned.workspace.as_str()),
            pair(ROLE, planned.role.as_str()),
            pair(AGENTS_LABEL, planned.agents_hash.as_str()),
            pair(LEAD_AGENT_LABEL, planned.lead_agent.as_str()),
            pair(POLICY, planned.policy.as_str()),
            pair(IMAGE, self.image.as_str()),
            pair(LAUNCH, planned.launch.as_str()),
            pair(DOCKER, &docker_value(self.docker)),
        ]))
    }

    pub fn from_labels(labels: &Labels) -> Result<SandboxLabels, LabelError> {
        Ok(SandboxLabels {
            planned: PlannedLabels {
                workspace: labels.parsed(WORKSPACE, WorkspaceName::parse)?,
                role: labels.parsed(ROLE, RoleName::parse)?,
                agents_hash: labels.parsed(AGENTS_LABEL, AgentHash::parse)?,
                lead_agent: Agent::parse(labels.value(LEAD_AGENT_LABEL)?).map_err(|source| {
                    LabelError::Invalid {
                        key: LEAD_AGENT_LABEL,
                        source,
                    }
                })?,
                policy: labels.parsed(POLICY, PolicyLabel::parse)?,
                launch: labels.parsed(LAUNCH, LaunchId::parse)?,
            },
            image: labels.parsed(IMAGE, ImageLabel::parse)?,
            docker: docker_label(labels.value(DOCKER)?)?,
        })
    }
}

fn docker_value(label: DockerLabel) -> String {
    match label {
        DockerLabel::Off => "off".to_owned(),
        DockerLabel::Engine { port, ca } => format!("{ENGINE_PREFIX}{port}-{ca}"),
    }
}

fn docker_label(value: &str) -> Result<DockerLabel, LabelError> {
    if value == "off" {
        return Ok(DockerLabel::Off);
    }
    let parsed = value
        .strip_prefix(ENGINE_PREFIX)
        .and_then(|rest| rest.split_once('-'))
        .and_then(|(port, ca)| {
            let canonical =
                !port.starts_with('0') && port.bytes().all(|byte| byte.is_ascii_digit());
            Some(DockerLabel::Engine {
                port: canonical.then(|| port.parse().ok()).flatten()?,
                ca: CaFingerprint::parse(ca)?,
            })
        });
    parsed.ok_or_else(|| LabelError::InvalidValue {
        key: DOCKER.to_owned(),
        reason: "is neither off nor an engine port with a CA fingerprint",
    })
}

fn label_name_problem(name: &str) -> Option<&'static str> {
    let (Some(first), Some(last)) = (name.chars().next(), name.chars().next_back()) else {
        return Some("is empty");
    };
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Some("has a character outside letters, digits, '-', '_' and '.'");
    }
    if !first.is_alphanumeric() || !last.is_alphanumeric() {
        return Some("does not start and end with a letter or digit");
    }
    None
}

fn label_key_problem(key: &str) -> Option<&'static str> {
    if key.len() > LABEL_KEY_MAX {
        return Some("is longer than 253 characters");
    }
    let (prefix, name) = match key.split_once('/') {
        Some((prefix, name)) => (Some(prefix), name),
        None => (None, key),
    };
    if name.len() > LABEL_VALUE_MAX {
        return Some("has a name part longer than 63 characters");
    }
    if let Some(problem) = label_name_problem(name) {
        return Some(problem);
    }
    let prefix = prefix?;
    let dns_char = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.');
    let edge = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let valid = !prefix.is_empty()
        && prefix.chars().all(dns_char)
        && prefix.chars().next().is_some_and(edge)
        && prefix.chars().next_back().is_some_and(edge)
        && !prefix.contains("..");
    if valid {
        None
    } else {
        Some("has an invalid DNS prefix")
    }
}

fn label_value_problem(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return None;
    }
    if value.len() > LABEL_VALUE_MAX {
        return Some("is longer than 63 characters");
    }
    label_name_problem(value)
}

impl Labels {
    pub fn from_pairs(
        pairs: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Labels, LabelError> {
        let mut labels = BTreeMap::new();
        for (key, value) in pairs {
            if let Some(reason) = label_key_problem(&key) {
                return Err(LabelError::InvalidKey { key, reason });
            }
            if let Some(reason) = label_value_problem(&value) {
                return Err(LabelError::InvalidValue { key, reason });
            }
            labels.insert(LabelKey(key), LabelValue(value));
        }
        Ok(Labels(labels))
    }

    pub fn from_valid_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> Labels {
        Labels(
            pairs
                .into_iter()
                .filter(|(key, value)| {
                    label_key_problem(key).is_none() && label_value_problem(value).is_none()
                })
                .map(|(key, value)| (LabelKey(key), LabelValue(value)))
                .collect(),
        )
    }

    pub fn differences(&self, wanted: &SandboxLabels) -> Vec<LabelDifference> {
        let wanted = wanted.to_labels();
        let absent = LabelValue(ABSENT.to_owned());
        let keys: BTreeSet<&LabelKey> = self.0.keys().chain(wanted.0.keys()).collect();
        keys.into_iter()
            .filter_map(|key| {
                let found = self.0.get(key).unwrap_or(&absent);
                let wanted = wanted.0.get(key).unwrap_or(&absent);
                (found != wanted).then(|| LabelDifference {
                    key: key.clone(),
                    found: found.clone(),
                    wanted: wanted.clone(),
                })
            })
            .collect()
    }

    pub fn workspace(&self) -> Option<WorkspaceName> {
        self.parsed(WORKSPACE, WorkspaceName::parse).ok()
    }

    pub fn role(&self) -> Option<RoleName> {
        self.parsed(ROLE, RoleName::parse).ok()
    }

    pub fn launch(&self) -> Option<LaunchId> {
        self.parsed(LAUNCH, LaunchId::parse).ok()
    }

    pub fn is_managed(&self) -> bool {
        self.0
            .get(&LabelKey(MANAGED.to_owned()))
            .is_some_and(|value| value.0 == "true")
    }

    pub fn iter(&self) -> impl Iterator<Item = (&LabelKey, &LabelValue)> {
        self.0.iter()
    }

    fn value(&self, key: &'static str) -> Result<&str, LabelError> {
        self.0
            .get(&LabelKey(key.to_owned()))
            .map(|value| value.0.as_str())
            .ok_or(LabelError::Missing { key })
    }

    fn parsed<T>(
        &self,
        key: &'static str,
        parse: fn(&str) -> Result<T, IdError>,
    ) -> Result<T, LabelError> {
        parse(self.value(key)?).map_err(|source| LabelError::Invalid { key, source })
    }
}

impl LabelSelector {
    pub fn managed() -> LabelSelector {
        LabelSelector(vec![pair(MANAGED, "true")])
    }

    pub fn launch(id: &LaunchId) -> LabelSelector {
        LabelSelector(vec![pair(MANAGED, "true"), pair(LAUNCH, id.as_str())])
    }

    pub fn render(&self) -> String {
        self.0
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}
