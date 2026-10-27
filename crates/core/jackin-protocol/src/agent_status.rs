// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRawState {
    Unknown,
    Working,
    Blocked,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusConfidence {
    Unknown,
    Weak,
    Strong,
    Authoritative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusSource {
    None,
    VisibleScreen,
    ShellIntegration,
    ForegroundProcess,

    Reported { source_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal arbitrated-state wire flags (visible_blocker, \
              visible_idle, visible_working, process_exited, \
              foreground_returned_to_shell, stale_report) — each is an independent \
              observable serialized to the control socket and consumed individually \
              by the host console. Named-field reads match the per-signal \
              wire-payload idiom this struct parallels."
)]
pub struct AgentStatusReport {
    pub raw_state: AgentRawState,
    pub source: AgentStatusSource,
    pub confidence: AgentStatusConfidence,
    pub detected_agent: Option<String>,
    pub foreground_pgid: Option<u32>,
    pub visible_blocker: bool,
    pub visible_idle: bool,
    pub visible_working: bool,
    pub process_exited: bool,
    pub foreground_returned_to_shell: bool,
    pub stale_report: bool,
    pub subagents_active: u32,
    pub revision: u64,
}
