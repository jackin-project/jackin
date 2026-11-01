// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use thiserror::Error;

use crate::agent::Agent;
use crate::ids::{AccountName, IdError, ImageId, ImageLabel, LaunchId, RoleName, WorkspaceName};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelKey(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelValue(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labels(BTreeMap<LabelKey, LabelValue>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlannedLabels {
    pub workspace: WorkspaceName,
    pub role: RoleName,
    pub agent: Agent,
    pub account: AccountName,
    pub launch: LaunchId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SandboxLabels {
    pub planned: PlannedLabels,
    pub image: ImageLabel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelSelector(Vec<(LabelKey, LabelValue)>);

#[derive(Debug, Error)]
pub enum LabelError {
    #[error("missing label {key}")]
    Missing { key: &'static str },
    #[error("invalid value for label {key}")]
    Invalid { key: &'static str, source: IdError },
    #[error("unknown agent label {0}")]
    UnknownAgent(String),
}

const MANAGED: &str = "jackin.dev/managed";
const WORKSPACE: &str = "jackin.dev/workspace";
const ROLE: &str = "jackin.dev/role";
const AGENT: &str = "jackin.dev/agent";
const ACCOUNT: &str = "jackin.dev/account";
const IMAGE: &str = "jackin.dev/image";
const LAUNCH: &str = "jackin.dev/launch";

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

fn pair(key: &str, value: &str) -> (LabelKey, LabelValue) {
    (LabelKey(key.to_owned()), LabelValue(value.to_owned()))
}

impl PlannedLabels {
    pub fn complete(self, image: &ImageId) -> SandboxLabels {
        SandboxLabels {
            planned: self,
            image: image.label_value(),
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
            pair(AGENT, planned.agent.as_str()),
            pair(ACCOUNT, planned.account.as_str()),
            pair(IMAGE, self.image.as_str()),
            pair(LAUNCH, planned.launch.as_str()),
        ]))
    }

    pub fn from_labels(labels: &Labels) -> Result<SandboxLabels, LabelError> {
        let agent_text = labels.value(AGENT)?;
        let agent = Agent::ALL
            .into_iter()
            .find(|agent| agent.as_str() == agent_text)
            .ok_or_else(|| LabelError::UnknownAgent(agent_text.to_owned()))?;
        Ok(SandboxLabels {
            planned: PlannedLabels {
                workspace: labels.parsed(WORKSPACE, WorkspaceName::parse)?,
                role: labels.parsed(ROLE, RoleName::parse)?,
                agent,
                account: labels.parsed(ACCOUNT, AccountName::parse)?,
                launch: labels.parsed(LAUNCH, LaunchId::parse)?,
            },
            image: labels.parsed(IMAGE, ImageLabel::parse)?,
        })
    }
}

impl Labels {
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
