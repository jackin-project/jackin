// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{LaunchId, SandboxName};

use super::probe::ProbeReport;
use super::verdict::{StepId, Verdict};

#[derive(Clone, Debug, Default)]
pub(crate) struct LoadObs {
    pub exit_ok: bool,
    pub text: String,
    pub sandboxes: Vec<String>,
    pub ready: bool,
    pub supervisor: String,
    pub export_ok: Option<bool>,
}

#[derive(Debug, Default)]
pub(crate) struct World {
    pub build_failed: bool,
    pub image_id: Option<String>,
    pub probe_digest: Option<String>,
    pub s: Option<SandboxName>,
    pub l1: Option<LaunchId>,
    pub load1: Option<LoadObs>,
    pub load2: Option<LoadObs>,
    pub status_before: Option<Vec<u8>>,
    pub shared: Option<Result<ProbeReport, String>>,
    pub agent_pid: Option<u32>,
    pub verdicts: BTreeMap<StepId, Verdict>,
    pub held: BTreeMap<StepId, (Verdict, Duration)>,
    pub d: Option<SandboxName>,
    pub docker_env: Option<(String, String)>,
    pub container_created: bool,
    pub accounts_made: Vec<String>,
    pub digests_before: Option<Vec<(String, String)>>,
}
