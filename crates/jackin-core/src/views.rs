// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;

use crate::ids::{ImageId, ImageTag, ProviderName, SandboxName};
use crate::labels::SandboxLabels;
use crate::phase::Phase;
use crate::supervisor::SupervisorState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LsOutput {
    pub schema: u32,
    pub sandboxes: Vec<SandboxSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SandboxSummary {
    pub name: SandboxName,
    pub labels: SandboxLabels,
    pub phase: Phase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InspectView {
    pub schema: u32,
    pub summary: SandboxSummary,
    pub providers: Vec<ProviderName>,
    pub policy: PolicyStatus,
    pub closure_violations: Vec<ClosureViolationView>,
    pub supervisor: SupervisorReading,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyStatus {
    pub hash: Option<String>,
    pub version: Option<u32>,
    pub loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClosureViolationView {
    pub rule: String,
    pub host: String,
    pub provider_host: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SupervisorReading {
    Unreadable,
    Reported(SupervisorState),
    Invalid(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuildOutput {
    pub schema: u32,
    pub image: ImageTag,
    pub id: ImageId,
}
