// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-agent PTY session: spawn, resize, write input, read output, and track
//! session state for the daemon.
//! Not responsible for: attach-client I/O, socket framing, or daemon
//! multiplexing logic (`SessionSupervisor` + the Multiplexer shell own that).
//! Key invariant: the session's `DamageGrid` is the single source of truth
//! for re-rendering on tab/pane switch and client reattach.

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

/// PTY session: one PTY + one `DamageGrid` + state-inference timer.
///
/// Each session owns a PTY pair, a child process (agent or shell), and
/// the `DamageGrid` whose cells mirror the agent's view. The grid is the
/// source of truth for re-rendering on tab switch, pane switch, and
/// client reattach.
///
/// The grid emits typed `PassthroughEvent`s for OSC and unhandled-CSI
/// sequences as the agent produces them. After each PTY chunk the
/// session applies its `OscPolicy` to those events, retains the parsed
/// title / cwd / icon, and queues the bytes the daemon forwards to the
/// attached client *only* when the session owns the focused pane in the
/// active tab — the routing rule the roadmap calls out under "OSC
/// passthrough". Without this layer the grid would silently consume OSC,
/// so agent desktop notifications (OSC 9), clipboard writes (OSC 52),
/// window titles (OSC 0/1/2), hyperlinks (OSC 8), kitty-keyboard protocol
/// switches (`\x1b[>{n}u`), synchronised output markers (`\x1b[?2026h/l`),
/// and every other terminal extension the operator's outer terminal
/// understands would vanish at the multiplexer boundary.
#[expect(
    missing_debug_implementations,
    reason = "Session owns PTY and child-killer trait objects; debug formatting would expose session identity and state."
)]
pub struct Session {
    pub label: String,
    /// Instance config ID (`"claude-work"`), or `None` for shell sessions.
    /// The admitted instance, not a runtime slug: several instances may
    /// share one agent runtime. Resolved authoritatively at spawn time.
    pub agent: Option<String>,
    /// Owning account ID for this session's instance, or `None` for shell
    /// sessions and sessions spawned before account stamping. Splits inherit
    /// this from the source pane's instance.
    pub account_id: Option<String>,
    /// Exact host usage-broker capability for this instance. Surface and
    /// configured-account labels are insufficient when two accounts share a
    /// provider, so refreshes must carry this authority unchanged.
    pub usage_capability: Option<jackin_protocol::usage_broker::UsageAccountCapability>,
    /// Kernel identity assigned to this session. Control-socket authorization
    /// compares the peer UID with this value; it is not inferred from a wire
    /// session id supplied by the caller.
    pub identity: jackin_protocol::SessionIdentity,
    /// Random bearer capability for this exact PTY session. Duplicate panes
    /// may intentionally share an instance UID, so UID alone cannot authorize
    /// a target-scoped control RPC.
    pub(crate) control_capability: String,
    pub conversation_id: Option<String>,
    pub provider: Option<SessionProvider>,
    /// Published effective state. Authored solely by evidence arbitration on the
    /// daemon tick (see `agent_status`); kept in sync with `status.effective`.
    pub state: AgentState,
    /// Per-session evidence-arbitration status (raw state, confidence, seen,
    /// revision, last evidence summary). The single source of `state`.
    pub status: SessionStatus,
    /// Debounce bookkeeping for the inferred working→idle hold.
    pub pending_transition: crate::agent_status::policy::PendingTransition,
    status_transition_times: std::collections::VecDeque<std::time::Instant>,
    status_flapping: bool,
    /// Per-source gate state for runtime-event reporters (one per hook/plugin
    /// source addressing this session).
    pub gate_states:
        std::collections::HashMap<String, crate::agent_status::gating::SourceGateState>,
    /// Current semantic authority derived from runtime events, consumed by
    /// arbitration. `None` until a state-authoring event arrives (Claude/Codex
    /// are identity-only and never set this — Decision 0a).
    pub authority: Option<crate::agent_status::evidence::AuthorityEvidence>,
    /// Active descendant/subagent count from gating, surfaced in evidence.
    pub subagents_active: u32,
    /// PID of the spawned child (agent or shell), anchor for `/proc` physics.
    /// `None` for test sessions with no real process.
    pub child_pid: Option<u32>,
    /// Rolling CPU-jiffies sample for the watchdog's busy/quiet delta.
    cpu_sample: Option<crate::agent_status::process::ProcessCpuSample>,
    /// `true` once the agent has been seen owning the pane foreground — gates
    /// the foreground-returned-to-shell exit edge (only meaningful after the
    /// agent was actually in front).
    saw_agent_foreground: bool,
    /// Terminal-protocol evidence captured from the PTY parse and fed into the
    /// evidence snapshot. OSC signals are TTL-bounded or cleared with authority
    /// so a stale terminal edge cannot pin session state indefinitely.
    osc: crate::agent_status::evidence::OscEvidence,
    /// Persistent status framing across arbitrary PTY packet boundaries.
    osc_status_decoder: crate::agent_status::OscStatusDecoder,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child_killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    termination_requested: Arc<AtomicBool>,
    pub last_output_at: std::time::Instant,
    /// Last time the operator sent explicit keyboard input to this pane.
    /// Recency evidence only — never authors state (see the agent runtime
    /// status authority; the watchdog uses output, not input).
    pub last_input_at: std::time::Instant,
    /// `true` once the PTY has produced any output. Stays `false`
    /// during the brief window between `Session::spawn` and the
    /// child's first write — when the grid's cursor sits at (0, 0)
    /// of a blank primary screen with no agent UI drawn yet. The
    /// daemon gates `\x1b[?25h` (cursor visible) on this so a
    /// freshly-split pane does not paint a stray blinking cursor
    /// inside an otherwise empty rectangle.
    pub received_output: bool,
    /// Terminal model: `DamageGrid` is the sole renderer.
    pub shadow_grid: Box<termpane::DamageGrid>,
    /// OSC passthrough policy captured at spawn from the environment.
    /// A backgrounded pane cannot flip the gate at runtime.
    osc_policy: OscPolicy,
    /// Most recent `OSC 2` / `OSC 0` window title, if any.
    title: Option<String>,
    /// Most recent `OSC 1` window icon name, if any.
    icon_name: Option<String>,
    /// Most recently announced working directory, parsed from `OSC 7`
    /// (`\x1b]7;file://<host>/<path>\x07`). Modern shells emit this on
    /// every prompt; the daemon surfaces it as the pane box title when
    /// the agent has not set an `OSC 2` of its own. The raw `OSC 7` is
    /// NEVER forwarded — see `apply_passthrough_policy` for the
    /// host-pollution rationale.
    cwd: Option<String>,
    /// Bytes queued for the attached client after `OscPolicy` filtering.
    /// The daemon drains these via `drain_passthrough` and forwards them
    /// only when this session owns the focused pane.
    pending_passthrough: Vec<Vec<u8>>,
    /// Xterm modifyOtherKeys level requested by the focused program
    /// (`CSI > 4 ; <n> m`). Full-screen agents may leave this enabled
    /// when they return to a shell, making plain text arrive as CSI-u
    /// fragments. Track it so alternate-screen exit can reset it.
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
