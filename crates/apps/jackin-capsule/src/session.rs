// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod agent_cmd;
mod control;
mod osc_policy;
mod pty;
mod pty_exit;
mod shell_cmd;
mod spawn;
mod status;
mod telemetry;
mod types;
mod viewport;

#[cfg(test)]
pub(crate) use agent_cmd::agent_model_args;
pub(crate) use agent_cmd::inject_status_env;
pub use agent_cmd::{AgentSpawnSpec, build_agent_command};
pub use osc_policy::{OscPolicy, osc8_uri_is_safe, parse_osc7};
use pty_exit::{error_type as pty_exit_error_type, reason as pty_exit_reason};
pub use shell_cmd::build_shell_command;
pub use spawn::validate_spawn_token_syntax;
pub use types::{
    BranchName, EXPLICIT_CAPABILITY_ENV_NAMES, GitContext, Oid, PullRequestLookupOutcome,
    SCROLLBACK_LEN, SESSION_ENV_PASSTHROUGH, SessionEvent, SessionProvider, SessionSpawnSpec,
    SessionTerminal, StatusTick, StatusTransition, next_id,
};

#[cfg(test)]
pub(crate) use shell_cmd::isolated_wrapper_args;
pub(crate) use shell_cmd::{
    apply_account_env, apply_terminal_env, is_explicit_capability_env, isolated_command,
    remove_ambient_capability_env,
};

pub(crate) use telemetry::{
    capture_pty_fixture_bytes, child_exit_reason, emit_pty_exit, emit_pty_spawn,
    record_terminal_bytes,
};
pub(crate) use types::{OSC_EVIDENCE_MAX_CHARS, STATUS_FLAP_THRESHOLD, STATUS_FLAP_WINDOW};

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use portable_pty::{ChildKiller, MasterPty};
use tokio::sync::mpsc;

use crate::agent_status::SessionStatus;
use crate::protocol::AgentState;

#[expect(
    missing_debug_implementations,
    reason = "Session owns PTY and child-killer trait objects; debug formatting would expose session identity and state."
)]
pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub account_id: Option<String>,
    pub usage_capability: Option<jackin_protocol::usage_broker::UsageAccountCapability>,
    pub identity: jackin_protocol::SessionIdentity,
    pub(crate) control_capability: String,
    pub conversation_id: Option<String>,
    pub provider: Option<SessionProvider>,
    pub state: AgentState,
    pub status: SessionStatus,
    pub pending_transition: crate::agent_status::policy::PendingTransition,
    status_transition_times: std::collections::VecDeque<std::time::Instant>,
    status_flapping: bool,
    pub gate_states:
        std::collections::HashMap<String, crate::agent_status::gating::SourceGateState>,
    pub authority: Option<crate::agent_status::evidence::AuthorityEvidence>,
    pub subagents_active: u32,
    pub child_pid: Option<u32>,
    cpu_sample: Option<crate::agent_status::process::ProcessCpuSample>,
    saw_agent_foreground: bool,
    osc: crate::agent_status::evidence::OscEvidence,
    osc_status_decoder: crate::agent_status::OscStatusDecoder,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child_killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    termination_requested: Arc<AtomicBool>,
    pub last_output_at: std::time::Instant,
    pub last_input_at: std::time::Instant,
    pub received_output: bool,
    pub shadow_grid: Box<termpane::DamageGrid>,
    osc_policy: OscPolicy,
    title: Option<String>,
    icon_name: Option<String>,
    cwd: Option<String>,
    pending_passthrough: Vec<Vec<u8>>,
    modify_other_keys: Option<u16>,
}

fn lock_or_record_poison<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    if let Ok(guard) = mutex.lock() {
        Some(guard)
    } else {
        let _event =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::Panic);
        None
    }
}

#[cfg(test)]
impl Session {
    #[expect(
        clippy::too_many_arguments,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    pub(crate) fn new_for_test(
        label: String,
        agent: Option<String>,
        provider: Option<SessionProvider>,
        size: (u16, u16),
        scrollback_len: usize,
        input_tx: mpsc::UnboundedSender<Vec<u8>>,
        pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
        child_killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    ) -> Self {
        Self {
            label,
            agent,
            account_id: None,
            usage_capability: None,
            identity: jackin_protocol::SessionIdentity {
                uid: 65_534,
                gid: 65_534,
            },
            control_capability: uuid::Uuid::new_v4().to_string(),
            conversation_id: None,
            provider,
            state: AgentState::Unknown,
            status: SessionStatus::new(),
            pending_transition: crate::agent_status::policy::PendingTransition::default(),
            status_transition_times: std::collections::VecDeque::new(),
            status_flapping: false,
            gate_states: std::collections::HashMap::new(),
            authority: None,
            subagents_active: 0,
            child_pid: None,
            cpu_sample: None,
            saw_agent_foreground: false,
            osc: crate::agent_status::evidence::OscEvidence::default(),
            osc_status_decoder: crate::agent_status::OscStatusDecoder::default(),
            input_tx,
            pty_master,
            child_killer,
            termination_requested: Arc::new(AtomicBool::new(false)),
            last_output_at: std::time::Instant::now(),
            last_input_at: std::time::Instant::now(),
            received_output: true,
            shadow_grid: Box::new(termpane::DamageGrid::new(size.0, size.1, scrollback_len)),
            osc_policy: OscPolicy::default(),
            title: None,
            icon_name: None,
            cwd: None,
            pending_passthrough: Vec::new(),
            modify_other_keys: None,
        }
    }
}

#[cfg(test)]
mod tests;
