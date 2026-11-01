// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::Serialize;
use thiserror::Error;

use crate::agent::Agent;
use crate::ids::{AccountName, IdError, ImageLabel, LaunchId, RoleName, WorkspaceName};

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
