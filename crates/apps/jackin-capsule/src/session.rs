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

pub(crate) use agent_cmd::inject_status_env;
pub use agent_cmd::{AgentSpawnSpec, build_agent_command};
pub use osc_policy::{OscPolicy, osc8_uri_is_safe, parse_osc7};
use pty_exit::{error_type as pty_exit_error_type, reason as pty_exit_reason};
pub use shell_cmd::build_shell_command;
pub use spawn::validate_spawn_token_syntax;
pub use types::{
    BranchName, EXPLICIT_CAPABILITY_ENV_NAMES, GitContext, InputOwnership, Oid,
    PullRequestLookupOutcome, SCROLLBACK_LEN, SESSION_ENV_PASSTHROUGH, SessionEvent,
    SessionProvider, SessionSpawnSpec, SessionTerminal, StatusTick, StatusTransition, next_id,
};

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
    pub input_ownership: InputOwnership,
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
