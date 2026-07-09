use std::time::Instant;

pub use jackin_protocol::agent_status::AgentRawState as RawAgentState;
use jackin_protocol::agent_status::AgentStatusConfidence;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceSnapshot {
    pub authority: Option<AuthorityEvidence>,
    pub osc: OscEvidence,
    pub screen: ScreenEvidence,
    pub process: ProcessEvidence,
    pub activity: ActivityEvidence,
    pub subagents_active: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityEvidence {
    pub source_id: String,
    pub grade: AuthorityGrade,
    pub mapped_state: RawAgentState,
    pub pending_permission: bool,
    pub last_event: Instant,
    pub notes: Vec<EvidenceNote>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityGrade {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OscEvidence {
    pub title: Option<String>,
    pub progress_active: bool,
    pub progress_cleared_at: Option<Instant>,
    /// Last raw OSC 9;4 progress payload (`"4;<state>"`), for the rule pack's
    /// `osc_progress` virtual region. `None` until any progress is emitted, so a
    /// rule can never match progress that never happened.
    pub progress_raw: Option<String>,
    pub shell_state: Option<RawAgentState>,
    pub shell_state_marked_at: Option<Instant>,
}

impl OscEvidence {
    pub fn clear_agent_signals(&mut self) {
        self.title = None;
        self.progress_active = false;
        self.progress_cleared_at = None;
        self.progress_raw = None;
        self.shell_state = None;
        self.shell_state_marked_at = None;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenEvidence {
    pub state: Option<RawAgentState>,
    pub rule_id: Option<String>,
    pub strong: bool,
    pub freeze: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal /proc-derived process signals (process_exited, \
              foreground_returned_to_shell, child_alive, root_is_agent, \
              foreground_is_agent, physics_sampled) — each is an independent \
              observable the watchdog + arbitrators inspect individually. Named- \
              field reads match the per-signal rule-pipeline idiom."
)]
pub struct ProcessEvidence {
    pub process_exited: bool,
    pub foreground_returned_to_shell: bool,
    pub child_alive: bool,
    pub root_is_agent: bool,
    pub foreground_is_agent: bool,
    pub foreground_pgid: Option<u32>,
    pub child_process_count: u32,
    pub cpu_jiffies_delta: u64,
    /// `true` only when `/proc` physics was actually sampled this evaluation
    /// (Linux, agent PID known). When `false`, CPU/child counts are absent, not
    /// "quiet": the watchdog must not demote on unavailable physics. Set by the
    /// `/proc` process sampler; the non-Linux stub leaves it `false`.
    pub physics_sampled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityEvidence {
    pub last_output: Option<Instant>,
    pub last_input: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Ten orthogonal arbitrated-state flags (physics_sampled, \
              osc_progress_active, shell_integration, visible_blocker, \
              visible_idle, visible_working, process_exited, \
              foreground_returned_to_shell, root_is_agent, stale_report) — each \
              tracks an independent observable the arbitrators + watchdog consume \
              individually. Named-field reads match the per-signal rule-pipeline \
              idiom."
)]
pub struct EvidenceSummary {
    pub raw_state: RawAgentState,
    pub confidence: AgentStatusConfidence,
    pub winner: EvidenceWinner,
    pub rule_id: Option<String>,
    pub foreground_pgid: Option<u32>,
    pub last_output: Option<Instant>,
    pub last_input: Option<Instant>,
    pub child_process_count: u32,
    pub cpu_jiffies_delta: u64,
    pub physics_sampled: bool,
    pub subagents_active: u32,
    pub osc_progress_active: bool,
    pub shell_integration: bool,
    pub visible_blocker: bool,
    pub visible_idle: bool,
    pub visible_working: bool,
    pub process_exited: bool,
    pub foreground_returned_to_shell: bool,
    pub root_is_agent: bool,
    pub stale_report: bool,
    pub notes: Vec<EvidenceNote>,
}

impl Default for EvidenceSummary {
    fn default() -> Self {
        Self {
            raw_state: RawAgentState::Unknown,
            confidence: AgentStatusConfidence::Unknown,
            winner: EvidenceWinner::Unknown,
            rule_id: None,
            foreground_pgid: None,
            last_output: None,
            last_input: None,
            child_process_count: 0,
            cpu_jiffies_delta: 0,
            physics_sampled: false,
            subagents_active: 0,
            osc_progress_active: false,
            shell_integration: false,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            foreground_returned_to_shell: false,
            root_is_agent: false,
            stale_report: false,
            notes: Vec::new(),
        }
    }
}

impl EvidenceSummary {
    pub fn has_note(&self, target: EvidenceNote) -> bool {
        self.notes.contains(&target)
    }
}

/// Which evidence channel authored the arbitrated state. `Authority` carries the
/// winning reporter's source id, so the reported source is a function of the
/// winner alone — a screen/OSC/physics state can never be mis-attributed to a
/// reporter, and an authority-won state always knows its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceWinner {
    ProcessExit,
    Freeze,
    /// A blocking dialog matched on the live screen (not authority-sourced).
    Blocked,
    Authority {
        source_id: String,
    },
    StrongVisualOrOsc,
    Physics,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceNote {
    WatchdogDemoted,
    AuthorityExpired,
    AuthorityIdentityMismatch,
    StopSuppressed,
    ProcessExited,
    ForegroundReturnedToShell,
}

#[cfg(test)]
mod tests;
