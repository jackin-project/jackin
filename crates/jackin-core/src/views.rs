// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::boundary::Timestamp;
use crate::display::plain;
use crate::ids::{ImageId, ImageTag, ProviderName, SandboxName};
use crate::labels::SandboxLabels;
use crate::phase::Phase;
use crate::supervisor::SupervisorState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LsOutput {
    pub schema: u32,
    pub sandboxes: Vec<SandboxSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxSummary {
    pub name: SandboxName,
    pub labels: SandboxLabels,
    pub phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectView {
    pub schema: u32,
    pub summary: SandboxSummary,
    pub providers: Vec<ProviderName>,
    pub policy: PolicyStatus,
    pub closure_violations: Vec<ClosureViolationView>,
    pub supervisor: SupervisorReading,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyStatus {
    pub hash: Option<String>,
    pub version: Option<u32>,
    pub loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureViolationView {
    pub rule: String,
    pub host: String,
    pub provider_host: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupervisorReading {
    Unreadable,
    Reported(SupervisorState),
    Invalid(String),
}

impl fmt::Display for SupervisorReading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupervisorReading::Unreadable => f.write_str("unreadable"),
            SupervisorReading::Reported(state) => f.write_str(&state.render()),
            SupervisorReading::Invalid(message) => write!(f, "invalid ({})", plain(message)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildOutput {
    pub schema: u32,
    pub image: ImageTag,
    pub id: ImageId,
}
