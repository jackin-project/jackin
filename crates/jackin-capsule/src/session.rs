// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-agent PTY session: spawn, resize, write input, read output, and track
//! session state for the daemon.
//! Not responsible for: attach-client I/O, socket framing, or daemon
//! multiplexing logic (`SessionSupervisor` + the Multiplexer shell own that).
//! Key invariant: the session's `DamageGrid` is the single source of truth
//! for re-rendering on tab/pane switch and client reattach.

mod osc_policy;
mod pty_exit;

pub use osc_policy::{OscPolicy, osc8_uri_is_safe, parse_osc7};
use pty_exit::{error_type as pty_exit_error_type, reason as pty_exit_reason};

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
use jackin_core::container_paths;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use jackin_telemetry::ResultTelemetryExt as _;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::agent_status::SessionStatus;
use crate::protocol::AgentState;
use crate::pull_request::PullRequestInfo;
use crate::tui::pane_snapshot::RowSnapshot;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Lines of scrollback every PTY session retains. ~1.5 MB worst-case
/// per session at 200 cols. Empty cells cost less. Operators need
/// scrollback to read Codex / Claude responses that exceed one
/// viewport, so this stays generous.
pub const SCROLLBACK_LEN: usize = 10_000;

/// Cap on retained OSC-evidence string payloads (e.g. the window title). OSC
/// content is untrusted model output; retaining unbounded text would let an
/// agent grow capsule memory by spamming long titles.
const OSC_EVIDENCE_MAX_CHARS: usize = 256;

pub const SESSION_ENV_PASSTHROUGH: &[&str] = &[
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "JACKIN_GIT_COAUTHOR_TRAILER",
    "JACKIN_GIT_DCO",
    "TZ",
];

/// Host credentials that are capabilities, not session ambient state.
///
/// These names may be resolved only through the operator-approved
/// `jackin-exec` path. They are removed from both the inherited process
/// environment and the session override allowlist before an agent or shell
/// starts. Keeping this list separate from account credentials is deliberate:
/// GitHub access is an on-demand capability, not an account selected for an
/// agent pane.
pub const EXPLICIT_CAPABILITY_ENV_NAMES: &[&str] = &[
    jackin_core::GH_TOKEN_ENV_NAME,
    jackin_core::GITHUB_TOKEN_ENV_NAME,
    jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME,
];

/// True when an OSC 8 `URI` payload is safe to forward to the
/// operator's host terminal. The empty URI is a terminator (closing
/// a hyperlink range), so it always passes; otherwise the scheme
/// must be `http`, `https`, or `mailto`. `javascript:`, `data:`,
/// `file://`, and anything else are dropped — a compromised agent
/// could otherwise script the operator's terminal emulator or
/// reference operator-side files on click.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Resolved provider a session was spawned with. Label and env overrides
/// travel together (both derived from one `jackin_protocol::Provider` at
/// spawn time) so a split can faithfully inherit the source pane's provider
/// without the label drifting from its redirect env.
#[derive(Debug, Clone)]
pub struct SessionProvider {
    pub label: String,
    pub env_overrides: Vec<(String, String)>,
}

/// Inputs that identify a session at PTY spawn time.
#[derive(Debug, Clone)]
pub struct SessionSpawnSpec {
    /// Display label shown in the tab and pane chrome.
    pub label: String,
    /// Configured agent instance, or `None` for a shell session.
    pub agent: Option<String>,
    /// Owning account for the configured agent instance.
    pub account_id: Option<String>,
    /// Kernel identity assigned to the child process.
    pub identity: jackin_protocol::SessionIdentity,
    /// Provider routing and environment inherited by this session.
    pub provider: Option<SessionProvider>,
    /// Per-instance XDG cache root. Shells and legacy configs use the
    /// private PTY-session cache instead.
    pub cache_dir: Option<String>,
}

/// A published public-state change emitted by [`Session::advance_status`].
#[derive(Debug, Clone)]
pub struct StatusTransition {
    pub previous: AgentState,
    pub effective: AgentState,
    pub winner: crate::agent_status::evidence::EvidenceWinner,
}

/// Outcome of one [`Session::advance_status`] tick for the daemon to react to:
/// `transition` is `Some` when a public state change published; `stuck` flags a
/// watchdog demotion for telemetry.
#[derive(Debug, Clone)]
pub struct StatusTick {
    pub transition: Option<StatusTransition>,
    pub stuck: bool,
    pub flap: bool,
}

const STATUS_FLAP_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
const STATUS_FLAP_THRESHOLD: usize = 3;

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

#[derive(Debug)]
pub enum SessionEvent {
    Output {
        session_id: u64,
        data: Vec<u8>,
    },
    Exited {
        session_id: u64,
        reason: Option<String>,
    },
    GitBranchContextRefreshRequested,
    GitBranchContextLoaded {
        request_id: u64,
        context: GitContext,
    },
    PullRequestContextLoaded {
        request_id: u64,
        branch: Option<BranchName>,
        /// HEAD captured at spawn so the cache entry is keyed on what
        /// the worker actually queried, not on mux state at apply time.
        head: Option<Oid>,
        outcome: PullRequestLookupOutcome,
    },
}

/// Resolved git state for the workspace workdir. Three meaningful
/// variants — `Absent` (no readable git metadata), `Branch` (on a
/// named branch, head resolves when the tip exists), `Detached`
/// (HEAD points directly at an OID with no branch ref). The old
/// `{branch: Option<String>, head: Option<String>}` shape allowed a
/// fourth nonsense state (`branch=None, head=Some` with no detached
/// context); the sum type removes it at the type level.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitContext {
    #[default]
    Absent,
    Detached {
        head: Oid,
    },
    Branch {
        name: BranchName,
        /// `None` while the branch ref hasn't resolved (unborn HEAD on
        /// a fresh `git init`, or a packed-refs miss before the next
        /// poll). The PR-context cache treats `None` and `Some` as
        /// distinct cache keys so cache busts on first-tip arrival.
        head: Option<Oid>,
    },
}

impl GitContext {
    #[must_use]
    pub fn branch_name(&self) -> Option<&BranchName> {
        match self {
            Self::Branch { name, .. } => Some(name),
            _ => None,
        }
    }

    #[must_use]
    pub fn head(&self) -> Option<&Oid> {
        match self {
            Self::Detached { head } => Some(head),
            Self::Branch {
                head: Some(head), ..
            } => Some(head),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }
}

/// Validated git object id. Constructed via `Oid::parse`, which
/// accepts the two on-disk hex lengths git uses today (40 = SHA-1,
/// 64 = SHA-256 via `git init --object-format=sha256`, opt-in since
/// git 2.29). All hex digits must be ASCII case-insensitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Oid(String);

impl Oid {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Oid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Oid {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validated short branch name (no `refs/heads/` prefix, no
/// whitespace, non-empty). Constructed via `BranchName::parse`,
/// which strips a leading `refs/heads/` if present so callers can
/// pass either the symref target or the short name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(value: &str) -> Option<Self> {
        let stripped = value.strip_prefix("refs/heads/").unwrap_or(value);
        if stripped.is_empty() || stripped.chars().any(char::is_whitespace) {
            None
        } else {
            Some(Self(stripped.to_owned()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for BranchName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for BranchName {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for BranchName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BranchName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Outcome of a background `gh pr` lookup. The `Resolved` variant carries
/// the authoritative answer from `gh` — either the PR shape or `None`
/// meaning "no open PR on this head". `TransientFailure` means the
/// lookup itself failed (gh missing, auth not configured, timeout, JSON
/// parse error) and the previous cached value should be preserved.
/// Without this distinction every transient gh hiccup poisoned the
/// 60s cache with a fake "no PR" answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestLookupOutcome {
    Resolved(Option<Arc<PullRequestInfo>>),
    TransientFailure,
}

#[derive(Clone, Debug)]
pub struct SessionTerminal {
    pub rows: u16,
    pub cols: u16,
    pub row_arena: termpane::RowArena,
    /// Attach client's terminal default colors; the grid reports these to
    /// agent OSC 10/11 queries. `None` leaves the grid's dark-theme default.
    pub default_fg: Option<(u8, u8, u8)>,
    pub default_bg: Option<(u8, u8, u8)>,
}

impl Session {
    #[expect(
        clippy::excessive_nesting,
        reason = "Session spawn wires PTY + child handle + agent + env into the \
                  multiplexer state. The nested `is_err` + governed INFO event + state- \
                  update branches are the per-stage error-reporting protocol."
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "Same justification as the too_many_lines + excessive_nesting \
              allows: session spawn wires PTY + child handle + agent + env into \
              the multiplexer state. Inline shape preserves captured-runtime \
              state across the per-stage error-reporting branches."
    )]
    /// # Errors
    ///
    /// Returns an error when the PTY cannot be opened or the session process
    /// cannot be spawned.
    pub fn spawn(
        spec: SessionSpawnSpec,
        mut cmd: CommandBuilder,
        terminal: SessionTerminal,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
    ) -> Result<(Self, u64)> {
        let SessionSpawnSpec {
            label,
            agent,
            account_id,
            identity,
            provider,
            cache_dir,
        } = spec;
        let conversation_id = agent.as_ref().map(|_| uuid::Uuid::new_v4().to_string());
        // Per-tab trace: each pane/agent spawn is its own short trace on the
        // session timeline (shares the resource session.id).
        let rows = terminal.rows;
        let cols = terminal.cols;
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY")?;

        let master = pair.master;
        let slave = pair.slave;

        // Session id must exist before the child spawns so the agent-status
        // reporter env can carry it. (Assigned here, used for the Session below.)
        let sid = next_id();
        let control_capability = uuid::Uuid::new_v4().to_string();
        inject_status_env(
            &mut cmd,
            sid,
            agent.as_deref(),
            cache_dir.as_deref(),
            &control_capability,
        );

        let mut child = slave
            .spawn_command(cmd)
            .context("failed to spawn session process")?;
        let child_pid = child.process_id();
        if let Some(pid) = child_pid {
            crate::pid1::register_managed_child(pid);
        }
        let child_killer = Arc::new(Mutex::new(child.clone_killer()));
        let termination_requested = Arc::new(AtomicBool::new(false));
        drop(slave);

        let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(master));
        let master_for_read = Arc::clone(&master);
        let master_for_write = Arc::clone(&master);

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let event_tx_output = event_tx.clone();
        let event_tx_exit = event_tx.clone();
        let event_tx_writer_err = event_tx.clone();
        emit_pty_spawn(agent.as_deref(), conversation_id.as_deref());

        // PTY writer task. take_writer / lock failures emit Exited so the
        // daemon reaps the half-initialised session instead of leaving a
        // tab whose input keystrokes silently vanish. blocking_recv is
        // used instead of Handle::current().block_on(rx.recv()) because
        // the latter panics inside spawn_blocking on a current-thread
        // runtime ("Cannot block the current thread from within a runtime").
        jackin_telemetry::spawn::stream_blocking("pty.reader", move || {
            let writer = match lock_or_record_poison(&master_for_write) {
                None => None,
                Some(guard) => guard
                    .take_writer()
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                    .ok(),
            };
            let Some(mut writer) = writer else {
                drop(event_tx_writer_err.send(SessionEvent::Exited {
                    session_id: sid,
                    reason: Some("session PTY writer failed to initialize".to_owned()),
                }));
                return;
            };
            while let Some(data) = input_rx.blocking_recv() {
                if std::io::Write::write_all(&mut writer, &data)
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                    .is_err()
                {
                    drop(event_tx_writer_err.send(SessionEvent::Exited {
                        session_id: sid,
                        reason: Some("session PTY write failed".to_owned()),
                    }));
                    return;
                }
                record_terminal_bytes(
                    jackin_telemetry::schema::enums::StreamDirection::Input,
                    data.len(),
                );
            }
        });

        let event_tx_reader_err = event_tx.clone();
        jackin_telemetry::spawn::stream_blocking("pty.writer", move || {
            let reader = match lock_or_record_poison(&master_for_read) {
                None => None,
                Some(guard) => guard
                    .try_clone_reader()
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                    .ok(),
            };
            let Some(mut reader) = reader else {
                drop(event_tx_reader_err.send(SessionEvent::Exited {
                    session_id: sid,
                    reason: Some("session PTY reader failed to initialize".to_owned()),
                }));
                return;
            };
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => break,
                    Err(error) => {
                        drop(Err::<(), _>(error).record_telemetry_error(
                            jackin_telemetry::schema::enums::ErrorType::IoError,
                        ));
                        break;
                    }
                    Ok(n) => {
                        record_terminal_bytes(
                            jackin_telemetry::schema::enums::StreamDirection::Output,
                            n,
                        );
                        capture_pty_fixture_bytes(&buf[..n]);
                        let data = buf[..n].to_vec();
                        if event_tx_output
                            .send(SessionEvent::Output {
                                session_id: sid,
                                data,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        // Child-reaper task: blocks on `child.wait()` and emits the
        // Exited event the moment the child process is reaped, even
        // if the PTY master never returns EOF.
        //
        // Why this is separate from the reader task: when the
        // foreground process exec'd into another binary and that
        // binary forks subprocesses (Claude Code spawning git, npm,
        // background watchers), those subprocesses inherit the slave
        // PTY fd. The slave only fully closes once *all* fd holders
        // exit, so the master read blocks indefinitely after the
        // foreground agent quits while the lingering subprocess
        // keeps the fd alive. The reader-EOF-only design left the
        // pane stuck in this case.
        //
        // `child.wait()` blocks until the foreground process is
        // reaped — the exact moment the operator's perspective says
        // "the agent exited." Sending Exited here lets the daemon
        // remove the pane immediately; the reader task (still
        // blocked on master) becomes a leak that ends when the
        // multiplexer process itself exits.
        let exit_agent = agent.clone();
        let exit_conversation_id = conversation_id.clone();
        let exit_termination_requested = Arc::clone(&termination_requested);
        jackin_telemetry::spawn::stream_blocking("pty.wait", move || {
            let status = child.wait();
            emit_pty_exit(
                exit_agent.as_deref(),
                exit_conversation_id.as_deref(),
                status.as_ref(),
                exit_termination_requested.load(Ordering::Acquire),
            );
            if let Some(pid) = child_pid {
                crate::pid1::unregister_managed_child(pid);
                crate::pid1::reap_zombies();
            }
            drop(event_tx_exit.send(SessionEvent::Exited {
                session_id: sid,
                reason: child_exit_reason(status.as_ref()),
            }));
        });

        Ok((
            Session {
                label,
                agent,
                account_id,
                usage_capability: None,
                identity,
                control_capability,
                conversation_id,
                provider,
                state: AgentState::Unknown,
                status: SessionStatus::new(),
                pending_transition: crate::agent_status::policy::PendingTransition::default(),
                status_transition_times: std::collections::VecDeque::new(),
                status_flapping: false,
                gate_states: std::collections::HashMap::new(),
                authority: None,
                subagents_active: 0,
                child_pid,
                cpu_sample: None,
                saw_agent_foreground: false,
                osc: crate::agent_status::evidence::OscEvidence::default(),
                input_tx,
                pty_master: master,
                child_killer,
                termination_requested,
                last_output_at: std::time::Instant::now(),
                last_input_at: std::time::Instant::now(),
                received_output: false,
                shadow_grid: {
                    let mut grid = Box::new(termpane::DamageGrid::with_row_arena(
                        rows,
                        cols,
                        SCROLLBACK_LEN,
                        terminal.row_arena,
                    ));
                    grid.set_reported_colors(terminal.default_fg, terminal.default_bg);
                    grid
                },
                osc_policy: OscPolicy::from_env(),
                title: None,
                icon_name: None,
                cwd: None,
                pending_passthrough: Vec::new(),
                modify_other_keys: None,
            },
            sid,
        ))
    }

    /// Scroll the view by `delta` lines. Positive = scroll up (into
    /// history); negative = scroll down (toward live tail).
    ///
    /// Up-scroll is clamped to the **actual filled scrollback** at
    /// call time so scrolling past the top does not inflate the
    /// offset past what the grid will render.
    pub fn scroll_by(&mut self, delta: i32) -> bool {
        // Capsule panes scroll a PTY/DamageGrid tail view, not a top-offset
        // ratatui panel. `TailScroll` is the shared adapter for this shape;
        // the `scrollable_panel` offset helpers remain for ordinary widgets.
        let filled = self.scrollback_filled();
        let before = self.scrollback_offset();
        let mut tail = termrock::scroll::TailScroll::new(before);
        tail.scroll_by(filled, delta as isize);
        if tail.offset() == before {
            return false;
        }
        self.shadow_grid.set_scrollback(tail.offset());
        true
    }

    /// Jump the scrollback view to an absolute tail-relative offset
    /// (`0` = live). Used by scrollbar click-to-jump; wheel deltas go
    /// through `scroll_by`.
    pub fn set_scrollback_offset(&mut self, offset: usize) -> bool {
        let before = self.scrollback_offset();
        self.shadow_grid.set_scrollback(offset);
        self.scrollback_offset() != before
    }

    /// Tail-relative scrollback view offset. The grid is the single owner
    /// (D12); the session only delegates.
    #[must_use]
    pub fn scrollback_offset(&self) -> usize {
        self.shadow_grid.scrollback()
    }

    /// Drop scrollback view, return to the live tail.
    pub fn scroll_to_live(&mut self) {
        self.reset_scrollback_view();
    }

    /// Clear this pane's saved scrollback and ask the foreground
    /// program to redraw its visible screen via the standard form-feed
    /// key (`Ctrl+L`). The visible grid is left to the PTY program so
    /// readline/TUI cursor state does not desynchronise from jackin❯'s
    /// local grid mirror.
    pub fn clear_scrollback_and_request_screen_clear(&mut self) {
        self.scroll_to_live();
        self.shadow_grid.clear_scrollback();
        let _sent = self.send_input(b"\x0c");
    }

    /// Number of scrollback lines currently retained for this pane.
    #[must_use]
    pub fn scrollback_filled(&self) -> usize {
        self.shadow_grid.scrollback_len()
    }

    /// Scrollback counts as `(grid_filled, inline_filled)`. The grid is
    /// the only scrollback source now, so the second element is always
    /// `0`; the tuple shape is kept for the debug-log call sites that
    /// still split the two for the `--debug` scrollbar trace.
    pub fn scrollback_counts(&mut self) -> (usize, usize) {
        (self.shadow_grid.scrollback_len(), 0)
    }

    fn reset_scrollback_view(&mut self) {
        self.shadow_grid.set_scrollback(0);
    }

    pub(crate) fn render_content_snapshot(&self, viewport_cols: u16) -> Vec<RowSnapshot> {
        crate::tui::pane_snapshot::pane_content_from_damagegrid(&self.shadow_grid, viewport_cols)
    }

    /// Content-coordinate snapshot of `content_rows` only (half-open).
    /// Element `i` is absolute content row `content_rows.start + i` (after clamp).
    pub(crate) fn render_content_snapshot_range(
        &self,
        viewport_cols: u16,
        content_rows: std::ops::Range<usize>,
    ) -> Vec<RowSnapshot> {
        crate::tui::pane_snapshot::pane_content_range_from_damagegrid(
            &self.shadow_grid,
            viewport_cols,
            content_rows,
        )
    }

    pub(crate) fn diagnostic_tail(&self, max_rows: usize) -> Option<String> {
        if max_rows == 0 {
            return None;
        }
        let (_, cols) = self.shadow_grid.size();
        let mut lines: Vec<String> = self
            .render_content_snapshot(cols)
            .into_iter()
            .rev()
            .filter_map(|row| {
                let line = row.text_range(0, cols).trim_end().to_owned();
                (!line.trim().is_empty()).then_some(line)
            })
            .take(max_rows)
            .collect();
        lines.reverse();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    #[must_use]
    pub fn hyperlink_target_at_content_row(&self, row: usize, col: u16) -> Option<&str> {
        self.shadow_grid.hyperlink_target_at_content_row(row, col)
    }

    #[must_use]
    pub fn send_input(&self, data: &[u8]) -> bool {
        // SendError fires when the writer task has exited (it owns the
        // receiver). The writer task emits SessionEvent::Exited before
        // dropping, so the daemon will reap this Session on the next
        // event tick. The writer boundary owns the originating failure.
        self.input_tx.send(data.to_vec()).is_ok()
    }

    /// Mark that the operator sent an explicit keyboard payload to this pane.
    /// Returns true when this clears a previously latched blocked state.
    pub fn mark_operator_input(&mut self) -> bool {
        let was_blocked = self.state == AgentState::Blocked;
        // Operator input updates recency evidence only. It never authors state
        // (that was the old flap bug: a keystroke in a blocked dialog flipped
        // Blocked→Working). State comes from evidence arbitration.
        self.last_input_at = std::time::Instant::now();
        was_blocked
    }

    /// Apply a forwarded runtime hook/plugin event from an in-container reporter.
    /// Maps the event through the daemon-owned gating table and updates this
    /// session's semantic authority (consumed by arbitration). Reporters forward
    /// events only — all mapping/gating lives here, never in the reporter.
    /// `seq` is assigned in arrival order per session.
    pub fn apply_runtime_event(
        &mut self,
        source_id: &str,
        runtime: &str,
        event: &str,
        payload: Option<&str>,
        now: std::time::Instant,
    ) {
        use crate::agent_status::evidence::AuthorityEvidence;
        use crate::agent_status::gating::{GateEffect, RuntimeEvent, enrich_event_name, map_event};

        let enriched = enrich_event_name(runtime, event, payload);
        let gate = self.gate_states.entry(source_id.to_owned()).or_default();
        let effect = map_event(
            &RuntimeEvent {
                runtime,
                event: enriched.as_str(),
            },
            gate,
        );
        let refresh_matching = |authority: &mut Option<AuthorityEvidence>| {
            if let Some(a) = authority
                && a.source_id == source_id
            {
                a.last_event = now;
            }
        };
        match effect {
            GateEffect::Authority {
                state,
                pending_permission,
                subagents_active,
                notes,
            } => {
                self.subagents_active = subagents_active;
                self.authority = Some(AuthorityEvidence {
                    source_id: source_id.to_owned(),
                    grade: grade_for_runtime(runtime),
                    mapped_state: state,
                    pending_permission,
                    last_event: now,
                    notes,
                });
            }
            GateEffect::CounterOnly { subagents_active } => {
                self.subagents_active = subagents_active;
                refresh_matching(&mut self.authority);
            }
            GateEffect::Heartbeat => refresh_matching(&mut self.authority),
            GateEffect::Clear => {
                self.gate_states.remove(source_id);
                if self
                    .authority
                    .as_ref()
                    .is_some_and(|a| a.source_id == source_id)
                {
                    self.authority = None;
                    self.subagents_active = 0;
                }
            }
            GateEffect::Ignore => {}
        }
    }

    /// Clear runtime-event authority and per-source gate state after an exit /
    /// foreground-returned-to-shell transition has been published, so a stale
    /// semantic report cannot outlive the process it described.
    pub fn clear_runtime_authority(&mut self) {
        self.authority = None;
        self.gate_states.clear();
        self.saw_agent_foreground = false;
        self.subagents_active = 0;
        // A new foreground process must not inherit the previous agent's
        // title/progress evidence.
        self.osc.clear_agent_signals();
    }

    /// Agent-authored terminal-protocol evidence for the evidence snapshot.
    #[must_use]
    pub fn osc_evidence(&self) -> &crate::agent_status::evidence::OscEvidence {
        &self.osc
    }

    /// Plain-text rows of the current visible viewport (top to bottom), for the
    /// screen rule-pack engine. Operator scrollback never affects detection —
    /// only the live screen is read.
    #[must_use]
    pub fn visible_screen_rows(&self) -> Vec<String> {
        let (_, cols) = self.shadow_grid.size();
        self.render_content_snapshot(cols)
            .iter()
            .map(|row| row.text_range(0, cols))
            .collect()
    }

    /// Sample `/proc` physics for this session's child, producing the
    /// `ProcessEvidence` arbitration consumes. Off-Linux (or with no child PID)
    /// returns default evidence with `physics_sampled = false` — "no evidence",
    /// never "quiet", so the watchdog cannot false-demote. On Linux a missing
    /// process is a real exit.
    pub fn sample_process_evidence(
        &mut self,
        now: std::time::Instant,
    ) -> crate::agent_status::evidence::ProcessEvidence {
        let mut sampler = crate::agent_status::process::ProcfsProcessSampler;
        self.sample_process_evidence_with(&mut sampler, now)
    }

    pub(crate) fn sample_process_evidence_with(
        &mut self,
        sampler: &mut impl crate::agent_status::process::ProcessSampler,
        now: std::time::Instant,
    ) -> crate::agent_status::evidence::ProcessEvidence {
        use crate::agent_status::evidence::ProcessEvidence;

        let Some(pid) = self.child_pid else {
            return ProcessEvidence::default();
        };
        if !sampler.physics_available() {
            return ProcessEvidence::default();
        }
        let Some(info) = sampler.read_process_info(pid) else {
            // Linux + PID gone = a real process exit.
            self.cpu_sample = None;
            return ProcessEvidence {
                process_exited: true,
                physics_sampled: true,
                ..ProcessEvidence::default()
            };
        };

        let foreground = sampler.foreground_group(&info);
        let foreground_is_agent = foreground.is_agent();
        let foreground_pgid = foreground.pgid();
        let child_process_count = sampler.descendant_process_count(pid);
        let cpu_jiffies_delta = sampler.sample_cpu_jiffies_delta(pid, &mut self.cpu_sample, now);
        let root_is_agent = crate::agent_status::process::identify_agent(&info).is_some();

        if foreground_is_agent {
            self.saw_agent_foreground = true;
        }
        // Returned to shell: the agent owned the pane earlier, the child is still
        // alive, the foreground group is now a non-agent (shell), and no
        // descendant work remains.
        let foreground_returned_to_shell = self.saw_agent_foreground
            && !foreground_is_agent
            && foreground.has_group()
            && child_process_count == 0;

        ProcessEvidence {
            process_exited: false,
            foreground_returned_to_shell,
            child_alive: true,
            root_is_agent,
            foreground_is_agent,
            foreground_pgid,
            child_process_count,
            cpu_jiffies_delta,
            physics_sampled: true,
        }
    }

    /// Advance the agent-status state machine by one tick: sample evidence,
    /// run the screen rule pack, arbitrate, debounce, and publish. This is the
    /// sole path that authors public agent state — the daemon only reacts to the
    /// returned [`StatusTick`] (redraw + telemetry). Exit clears runtime
    /// authority only after the exit transition has published, so a stale
    /// semantic report can never outlive the process it described.
    pub fn advance_status(
        &mut self,
        rule_registry: Option<&crate::agent_status::rules::RulePackRegistry>,
        now: std::time::Instant,
    ) -> StatusTick {
        let mut sampler = crate::agent_status::process::ProcfsProcessSampler;
        self.advance_status_with_process_sampler(rule_registry, &mut sampler, now)
    }

    pub(crate) fn advance_status_with_process_sampler(
        &mut self,
        rule_registry: Option<&crate::agent_status::rules::RulePackRegistry>,
        sampler: &mut impl crate::agent_status::process::ProcessSampler,
        now: std::time::Instant,
    ) -> StatusTick {
        use crate::agent_status::arbitrate::arbitrate;
        use crate::agent_status::evidence::{
            ActivityEvidence, EvidenceNote, EvidenceSnapshot, ScreenEvidence,
        };
        use crate::agent_status::policy::{apply_watchdog, debounce};
        use crate::agent_status::rules::VirtualRegions;

        let process = self.sample_process_evidence_with(sampler, now);
        let exiting = process.process_exited || process.foreground_returned_to_shell;
        // Screen rule-pack evaluation over the live viewport: the universal
        // detector and the sole state source for identity-only runtimes
        // (Claude/Codex) and Kimi.
        let screen = rule_registry
            .and_then(|registry| {
                let rows = self.visible_screen_rows();
                let osc = self.osc_evidence();
                let virtuals = VirtualRegions {
                    osc_title: osc.title.as_deref(),
                    osc_progress: osc.progress_raw.as_deref(),
                };
                registry.evaluate_with_virtuals(self.agent.as_deref(), &rows, virtuals)
            })
            .map_or_else(ScreenEvidence::default, |m| ScreenEvidence {
                state: m.state,
                rule_id: Some(m.rule_id),
                strong: m.strong,
                freeze: m.freeze,
            });
        let snapshot = EvidenceSnapshot {
            authority: self.authority.clone(),
            subagents_active: self.subagents_active,
            osc: self.osc_evidence().clone(),
            screen,
            process,
            activity: ActivityEvidence {
                last_output: Some(self.last_output_at),
                last_input: Some(self.last_input_at),
            },
        };
        let candidate = apply_watchdog(arbitrate(&snapshot, self.status.raw, now), now);
        // Stuck telemetry: a watchdog demotion means a witness claimed `working`
        // while physics went quiet (the interrupt hole / a hung authority).
        let stuck = candidate
            .notes
            .iter()
            .any(|n| matches!(n, EvidenceNote::WatchdogDemoted));
        // Debounce gates whether the candidate becomes a public transition
        // (immediate for blocked/working/exit/strong-idle; inferred idle needs
        // confirmation + CPU/OSC-quiet). Only commit through SessionStatus when
        // it permits.
        let mut transition = None;
        let mut flap = false;
        if debounce(self.state, &candidate, &mut self.pending_transition, now).is_some() {
            let previous = self.state;
            // Clone the winner only on the committing tick — most ticks debounce
            // suppresses the transition, and the winner now carries a String.
            let winner = candidate.winner.clone();
            if let Some(effective) = self.status.publish_raw(candidate) {
                self.state = effective;
                transition = Some(StatusTransition {
                    previous,
                    effective,
                    winner,
                });
                flap = self.record_status_transition(now);
            }
        }
        if exiting {
            self.clear_runtime_authority();
        }
        StatusTick {
            transition,
            stuck,
            flap,
        }
    }

    fn record_status_transition(&mut self, now: std::time::Instant) -> bool {
        while self
            .status_transition_times
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > STATUS_FLAP_WINDOW)
        {
            self.status_transition_times.pop_front();
        }
        self.status_transition_times.push_back(now);
        let flapping = self.status_transition_times.len() >= STATUS_FLAP_THRESHOLD;
        let started = flapping && !self.status_flapping;
        self.status_flapping = flapping;
        started
    }

    /// True when the session's program has enabled any mouse protocol
    /// mode. Used by the daemon to decide whether selection gestures
    /// belong to jackin or to the pane. Actual PTY mouse forwarding
    /// also consults `mouse_protocol_mode()` so press-only programs
    /// do not receive motion events.
    #[must_use]
    pub fn mouse_enabled(&self) -> bool {
        !matches!(
            self.shadow_grid.mouse_protocol_mode(),
            termpane::MouseProtocolMode::None
        )
    }

    #[must_use]
    pub fn mouse_protocol_encoding(&self) -> termpane::MouseProtocolEncoding {
        self.shadow_grid.mouse_protocol_encoding()
    }

    #[must_use]
    pub fn mouse_protocol_mode(&self) -> termpane::MouseProtocolMode {
        self.shadow_grid.mouse_protocol_mode()
    }

    /// True when the session enabled DEC private mode `?1004` (focus
    /// event reporting).
    #[must_use]
    pub fn focus_events_enabled(&self) -> bool {
        self.shadow_grid.focus_events()
    }

    /// True when the terminal is in the alternate screen.
    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.shadow_grid.alternate_screen()
    }

    /// True when the foreground program has bracketed-paste enabled.
    #[must_use]
    pub fn bracketed_paste(&self) -> bool {
        self.shadow_grid.bracketed_paste()
    }

    /// True when the foreground program has application-cursor-keys mode on.
    #[must_use]
    pub fn application_cursor(&self) -> bool {
        self.shadow_grid.application_cursor()
    }

    /// Feed PTY bytes into the grid and update activity timestamps.
    pub fn feed_pty(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.received_output = true;
        }
        jackin_diagnostics::incr_terminal_bytes_received(bytes.len() as u64);

        // Single batch feed — the grid's persistent vte parser handles
        // sequences split across PTY read boundaries internally.
        let was_alternate = self.shadow_grid.alternate_screen();
        let was_scrolled = self.scrollback_offset() != 0;
        let scrollback_before = self.shadow_grid.scrollback_len();
        self.shadow_grid.process(bytes);
        let is_alternate = self.shadow_grid.alternate_screen();
        if was_alternate && !is_alternate {
            self.clear_transient_keyboard_modes();
        }

        // The grid records semantic scroll operations, but the scroll-region
        // (DECSTBM) emission optimizer that would consume them is deferred (see
        // the Ratatui modernization roadmap). Clear each chunk so they cannot
        // grow unbounded on a long scroll-heavy session (retaining capacity to
        // avoid per-chunk reallocation); the optimizer will consume them at
        // frame compose when it lands.
        self.shadow_grid.clear_scroll_ops();

        self.apply_passthrough_policy();

        if was_scrolled {
            // Anchor the view to content: rows evicted into scrollback during
            // this feed grow the tail-relative offset by the same amount, so
            // the rows under the reader hold still while the agent streams
            // (D3). An ED3 during the feed already reset the grid's offset to
            // 0; the guard keeps the view live in that case. At scrollback
            // capacity the delta is 0 and the view slides — clamping at
            // `filled` (inside `set_scrollback`) is the existing contract.
            let delta = self
                .shadow_grid
                .scrollback_len()
                .saturating_sub(scrollback_before);
            let current = self.scrollback_offset();
            if current != 0 && delta != 0 {
                self.shadow_grid
                    .set_scrollback(current.saturating_add(delta));
            }
        } else {
            self.scroll_to_live();
        }

        // PTY output updates recency evidence only. It never authors state
        // (the old flap bug: any byte flipped Idle→Working, and a blocked
        // dialog repaint flipped Blocked→Working). State comes from evidence
        // arbitration over the rule pack / OSC / authority / physics.
        self.last_output_at = std::time::Instant::now();

        // OSC 133 shell-integration marks (emitted by the container shell rc,
        // not by agents) are strong shell-state evidence: PreExec → working,
        // PromptEnd / CommandFinished → idle. Captured here as evidence, never
        // authoring state directly.
        if let Some(mark) = crate::agent_status::scan_osc133(bytes) {
            use crate::agent_status::OscShellMark;
            use crate::agent_status::evidence::RawAgentState;
            let shell_state = match mark {
                OscShellMark::PreExec => Some(RawAgentState::Working),
                OscShellMark::PromptEnd | OscShellMark::CommandFinished { .. } => {
                    Some(RawAgentState::Idle)
                }
                OscShellMark::PromptStart => None,
            };
            if let Some(state) = shell_state {
                self.osc.shell_state_marked_at = Some(std::time::Instant::now());
                self.osc.shell_state = Some(state);
            }
        }

        // OSC 9;4 (ConEmu progress): state 0 = clear (done-ish hint), 1/2/3 =
        // active, 4 = paused. Not surfaced as a passthrough event, so scanned
        // from the raw stream. Progress-active is never working-proof (Claude
        // animates it during approval prompts); arbitration treats the clear
        // edge as a hint only.
        if let Some(state) = crate::agent_status::scan_osc9_progress(bytes) {
            self.osc.progress_raw = Some(format!("4;{state}"));
            if state == 0 {
                self.osc.progress_active = false;
                self.osc.progress_cleared_at = Some(std::time::Instant::now());
            } else {
                self.osc.progress_active = true;
            }
        }
    }

    /// Drain the grid's typed `PassthroughEvent`s, apply the session's
    /// `OscPolicy`, retain title / cwd / icon, and queue forwardable
    /// bytes in `pending_passthrough`.
    ///
    /// OSC 7 (cwd) is parsed for the pane-title surface and then
    /// dropped: forwarding it would let the operator's outer terminal
    /// remember the container's path, breaking `Cmd+T new tab` on the
    /// host (host-state pollution, forbidden by CLAUDE.md "Never mutate
    /// the host machine silently"). OSC 8 hyperlinks are gated through
    /// `osc8_uri_is_safe` so a compromised agent cannot smuggle a
    /// `javascript:` or `file://` URI to the host terminal.
    fn apply_passthrough_policy(&mut self) {
        use termpane::PassthroughEvent;
        let events = self.shadow_grid.drain_passthrough();
        for event in events {
            match event {
                PassthroughEvent::TitleChanged(ref title) => {
                    self.title = Some(title.clone());
                    // Agent-status evidence: retain the title (capped — OSC
                    // content is untrusted model output). The rule pack's
                    // `osc_title` virtual region reads this.
                    let capped: String = title.chars().take(OSC_EVIDENCE_MAX_CHARS).collect();
                    self.osc.title = Some(capped);
                    if self.osc_policy.allow_title()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::IconNameChanged(ref name) => {
                    self.icon_name = Some(name.clone());
                    if self.osc_policy.allow_title()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::CwdChanged(uri) => {
                    if let Some(path) = parse_osc7(&uri) {
                        self.cwd = Some(path);
                    }
                }
                PassthroughEvent::ClipboardWrite(_) => {
                    if self.osc_policy.allow_osc52()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::Notification(_) => {
                    // Plain OSC 9 desktop notification is forwarded to the host
                    // per policy. OSC 9;4 progress is decoded separately from the
                    // raw stream in `feed_pty` — termpane does not surface it
                    // here.
                    if self.osc_policy.allow_notify()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::Hyperlink { ref uri, .. } => {
                    if self.osc_policy.allow_hyperlink()
                        && osc8_uri_is_safe(uri)
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::UnhandledCsi(ref raw) => {
                    self.handle_unhandled_csi(raw);
                }
                PassthroughEvent::DroppedCsi(_) => {}
                // BEL is deliberately absorbed: the grid never forwarded a
                // byte for it before the event became typed (it was
                // swallowed), and the capsule owns every byte that reaches
                // the outer terminal. Tests assert on the event instead.
                PassthroughEvent::Bell => {}
                // Device/mode query the emulator answered itself. The reply
                // goes back to the agent's own PTY stdin — never the outer
                // terminal — so the agent's capability detection reflects the
                // grid, not the host. (Root fix for the alt-screen corruption:
                // the host was answering DA/DSR/DECRQM with its own caps.)
                PassthroughEvent::Reply(bytes) => {
                    drop(self.input_tx.send(bytes));
                }
                // ScrollbackClear is a grid-internal instruction with no
                // outer-terminal byte form; the grid already cleared its
                // own scrollback in `erase_display`. Reset the view offset.
                PassthroughEvent::ScrollbackClear => {
                    self.reset_scrollback_view();
                }
                // Mode toggles (focus, application cursor, bracketed paste)
                // round-trip to the outer terminal verbatim. The agent's
                // `?2026` toggles are absorbed in the grid — the capsule's
                // own frame brackets supersede them.
                PassthroughEvent::FocusEvents(_)
                | PassthroughEvent::ApplicationCursorKeys(_)
                | PassthroughEvent::BracketedPaste(_) => {
                    if let Some(bytes) = event.encode() {
                        self.pending_passthrough.push(bytes);
                    }
                }
            }
        }
    }

    /// Forward an allowlisted CSI the grid passed through. Only the
    /// documented allowlist arrives here — kitty keyboard push/pop
    /// (`\x1b[>{n}u` / `\x1b[<{n}u`, tracked by the grid and re-asserted by
    /// the per-frame mode reconciliation) and xterm modifyOtherKeys
    /// (`\x1b[>4;{n}m`, tracked so alternate-screen exit can reset it).
    /// Everything else is default-denied in the grid (§3.6).
    fn handle_unhandled_csi(&mut self, raw: &[u8]) {
        if let Some(level) = parse_modify_other_keys(raw) {
            self.modify_other_keys = (level != 0).then_some(level);
        }
        self.pending_passthrough.push(raw.to_vec());
    }

    fn clear_transient_keyboard_modes(&mut self) {
        if self.shadow_grid.kitty_kb_flags() != 0 {
            self.shadow_grid.clear_kitty_kb_stack();
            self.pending_passthrough.push(b"\x1b[<u".to_vec());
        }
        if self.modify_other_keys.take().is_some() {
            self.pending_passthrough.push(b"\x1b[>4;0m".to_vec());
        }
    }

    /// Drain the OSC / unhandled-CSI byte sequences captured during the
    /// last `feed_pty` call. The daemon forwards these to the attached
    /// client only when this session owns the focused pane in the active
    /// tab — backgrounded panes' notifications, clipboard writes, and
    /// titles must not reach the operator's outer terminal.
    pub fn drain_passthrough(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_passthrough)
    }

    #[must_use]
    pub fn allow_frame_hyperlinks(&self) -> bool {
        self.osc_policy.allow_hyperlink()
    }

    pub fn terminate(&self) {
        self.termination_requested.store(true, Ordering::Release);
        if let Some(mut killer) = lock_or_record_poison(&self.child_killer) {
            drop(
                killer
                    .kill()
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError),
            );
        }
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Most recently announced working directory (OSC 7), if any.
    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        // A pane collapsed below its border height yields a 0-row inner rect.
        // Never hand the agent PTY a 0×0 window size (programs expect ≥1) nor the
        // shadow grid a degenerate geometry. `DamageGrid::set_size` clamps too;
        // this keeps TIOCSWINSZ and the model in agreement on the floor.
        let rows = rows.max(1);
        let cols = cols.max(1);
        if let Some(master) = lock_or_record_poison(&self.pty_master) {
            drop(
                master
                    .resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError),
            );
        }
        self.shadow_grid.set_size(rows, cols);
        // Re-clamp through the grid: set_size may have shrunk the filled
        // scrollback the offset was clamped against.
        self.shadow_grid.set_scrollback(self.scrollback_offset());
    }
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

fn capture_pty_fixture_bytes(bytes: &[u8]) {
    use std::io::Write as _;
    use std::sync::OnceLock;

    static CAPTURE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let capture = CAPTURE.get_or_init(|| {
        let path = std::env::var_os("JACKIN_PTY_FIXTURE_CAPTURE")?;
        let file = std::fs::File::create(path).ok()?;
        Some(Mutex::new(file))
    });
    if let Some(capture) = capture
        && let Ok(mut file) = capture.lock()
    {
        drop(file.write_all(bytes));
        drop(file.flush());
    }
}

fn record_terminal_bytes(
    direction: jackin_telemetry::schema::enums::StreamDirection,
    bytes: usize,
) {
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::STREAM_DIRECTION,
        value: jackin_telemetry::Value::Str(direction.as_str()),
    }];
    let amount = u64::try_from(bytes).unwrap_or(u64::MAX);
    let _counter_result =
        jackin_telemetry::counter(&jackin_telemetry::metric::TERMINAL_BYTES).add(amount, &attrs);
}

fn emit_pty_spawn(agent: Option<&str>, conversation_id: Option<&str>) {
    use jackin_telemetry::{Attr, FieldSet, Value};
    let mut attrs = Vec::with_capacity(2);
    if let Some(agent) = agent {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        });
    }
    if let Some(conversation_id) = conversation_id {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID,
            value: Value::Str(conversation_id),
        });
    }
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::PTY_SPAWN,
        FieldSet::new(&attrs, None),
    );
}

fn emit_pty_exit(
    agent: Option<&str>,
    conversation_id: Option<&str>,
    status: Result<&portable_pty::ExitStatus, &std::io::Error>,
    cancelled: bool,
) {
    use jackin_telemetry::{Attr, FieldSet, Value};
    let reason = pty_exit_reason(status, cancelled);
    let mut attrs = vec![Attr {
        key: jackin_telemetry::schema::attrs::PTY_EXIT_REASON,
        value: Value::Str(reason.as_str()),
    }];
    if let Some(error_type) = pty_exit_error_type(reason) {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error_type.as_str()),
        });
    }
    if let Some(status) = status.ok()
        && !status.success()
        && status.signal().is_none()
    {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
            value: Value::I64(i64::from(status.exit_code())),
        });
    }
    if let Some(agent) = agent {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        });
    }
    if let Some(conversation_id) = conversation_id {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID,
            value: Value::Str(conversation_id),
        });
    }
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::PTY_EXIT,
        FieldSet::new(&attrs, None),
    );
}

fn child_exit_reason(status: Result<&portable_pty::ExitStatus, &std::io::Error>) -> Option<String> {
    match status {
        Ok(status) if status.success() => None,
        Ok(status) => match status.signal() {
            Some(signal) => Some(format!("session process exited after signal {signal}")),
            None => Some(format!(
                "session process exited with code {}",
                status.exit_code()
            )),
        },
        Err(err) => Some(format!("session process wait failed: {err}")),
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

/// Parse the xterm modifyOtherKeys level from a `CSI > 4 ; <n> m`
/// sequence's raw bytes. Returns the level only for that exact shape;
/// any other CSI returns `None`.
fn parse_modify_other_keys(raw: &[u8]) -> Option<u16> {
    let body = raw.strip_prefix(b"\x1b[")?.strip_suffix(b"m")?;
    let body = body.strip_prefix(b">")?;
    let mut parts = body.split(|&b| b == b';');
    let first = parts.next()?;
    if first != b"4" {
        return None;
    }
    let level = parts.next().unwrap_or(b"0");
    std::str::from_utf8(level).ok()?.parse::<u16>().ok()
}

/// Reject spawn-target strings that are flags (start with `-`), empty, or
/// contain whitespace / control characters. Syntax only: membership is
/// resolved separately via `CapsuleConfig::resolve_instance`, which maps
/// an instance config ID (or an unambiguous agent-slug shorthand) to its
/// admitted instance. Shared by the PID-1 argv path and the
/// `jackin-capsule new <target>` client path; the daemon re-resolves
/// authoritatively at spawn time.
/// # Errors
///
/// Returns an error when the value is empty, looks like a flag, or contains
/// whitespace or control characters.
pub fn validate_spawn_token_syntax(raw: &str) -> Result<&str, &'static str> {
    if raw.is_empty() {
        return Err("empty value");
    }
    if raw.starts_with('-') {
        return Err("looks like a flag");
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("contains whitespace or control characters");
    }
    Ok(raw)
}

/// Inject session-scoped environment into a command. The isolation wrapper
/// receives its private numeric identity for every child; only agent panes
/// receive the public runtime/status identity used by hook reporters. State is
/// never authored from these — reporters forward events, the daemon maps and
/// gates them.
fn inject_status_env(
    cmd: &mut CommandBuilder,
    session_id: u64,
    agent: Option<&str>,
    cache_dir: Option<&str>,
    control_capability: &str,
) {
    let session_root = session_root_path(session_id);
    let session_state = session_root.join("state");
    let session_tmp = session_root.join("tmp");
    let session_runtime = session_root.join("runtime");
    let session_cache = session_root.join("cache");
    // These paths are allocated by the root wrapper before Landlock is
    // installed. Every mutable setup/cache path is therefore private to this
    // PTY, never the capsule-wide state or host /tmp.
    cmd.env("JACKIN_SESSION_ROOT", &session_root);
    cmd.env(jackin_protocol::SESSION_STATE_DIR_ENV, &session_state);
    cmd.env("TMPDIR", &session_tmp);
    cmd.env("TMP", &session_tmp);
    cmd.env("TEMP", &session_tmp);
    cmd.env("XDG_RUNTIME_DIR", &session_runtime);
    if let Some(cache_dir) = cache_dir {
        cmd.env("XDG_CACHE_HOME", cache_dir);
    } else {
        cmd.env("XDG_CACHE_HOME", &session_cache);
    }
    cmd.env("GIT_CONFIG_GLOBAL", session_root.join("gitconfig"));
    cmd.env(jackin_protocol::SESSION_CAPABILITY_ENV, control_capability);
    cmd.env(
        jackin_protocol::ISOLATION_SESSION_ID_ENV,
        session_id.to_string(),
    );
    cmd.env_remove(jackin_protocol::SESSION_ID_ENV);
    cmd.env("JACKIN_STATUS_SOCKET", crate::socket::SOCKET_PATH);
    if let Some(runtime) = agent {
        cmd.env(jackin_protocol::SESSION_ID_ENV, session_id.to_string());
        cmd.env("JACKIN_AGENT_RUNTIME", runtime);
        cmd.env(
            "JACKIN_STATUS_SOURCE",
            format!("hook-{runtime}-{session_id}"),
        );
    } else {
        cmd.env_remove("JACKIN_AGENT_RUNTIME");
        cmd.env_remove("JACKIN_STATUS_SOURCE");
    }
}

/// Canonical private root for one daemon-assigned session id. The wrapper
/// derives the same path from the trusted numeric id rather than accepting a
/// caller-supplied filesystem path.
pub(crate) fn session_root_path(session_id: u64) -> PathBuf {
    Path::new(container_paths::SESSION_ROOTS_DIR).join(session_id.to_string())
}

/// Authority grade for a runtime's semantic source. `opencode` and the flagged
/// Codex app-server prototype ship complete lifecycle streams; `amp` and other
/// event sources have partial coverage.
fn grade_for_runtime(runtime: &str) -> crate::agent_status::evidence::AuthorityGrade {
    use crate::agent_status::evidence::AuthorityGrade;
    match runtime {
        "opencode" | "codex-app-server" => AuthorityGrade::Complete,
        _ => AuthorityGrade::Partial,
    }
}

/// Per-instance facts for one agent spawn, resolved from the Capsule
/// launch config. `home_dir` is the folder-var target
/// (`/home/agent/.claude` for primary slots,
/// `/home/agent/.claude-<suffix>` for secondary same-agent slots);
/// `forwarded_dir` is the host-forwarded credential dir.
#[derive(Debug)]
pub struct AgentSpawnSpec<'a> {
    pub agent: &'a str,
    pub instance: &'a str,
    pub home_dir: &'a str,
    pub forwarded_dir: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub auth_mode: Option<&'a str>,
    pub env_passthrough: &'a [(String, String)],
    pub cwd: &'a Path,
    pub codename: &'a str,
    /// Identity admitted by the host for this instance.
    pub identity: jackin_protocol::SessionIdentity,
}

/// Whether `name` is an agent config-folder env var
/// (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, …). Folder vars are owned by the
/// spawned instance, never by passthrough.
fn is_folder_env(name: &str) -> bool {
    jackin_core::Agent::ALL.iter().any(|agent| {
        agent
            .runtime()
            .state_paths()
            .folder_env_var
            .is_some_and(|var| var.name == name)
    })
}

/// Build a `CommandBuilder` for an agent session.
///
/// Entrypoint is `/jackin/runtime/entrypoint.sh` with `JACKIN_AGENT=<slug>`.
/// `cwd` is the workspace workdir from the Capsule launch config. It must be
/// passed explicitly: `portable_pty`'s `CommandBuilder`
/// defaults the child's cwd to `$HOME` when none is set — it does not
/// inherit the daemon's cwd — so omitting this would land every agent in
/// `/home/agent` regardless of the workspace.
///
/// Every agent folder var (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, …) is
/// scrubbed and rejected from passthrough, then this instance's folder
/// var is set to its own home: a stale or foreign value can never leak
/// this pane into another account's credentials or history. `HOME` is
/// account-owned the same way (`account_env` strips the ambient value),
/// so it is re-pointed at the instance home below: the slot root is the
/// only mutable account path outside the private session root, while
/// `/home/agent` itself is Landlock read-only.
#[must_use]
pub fn build_agent_command(spec: &AgentSpawnSpec<'_>) -> CommandBuilder {
    let mut cmd = isolated_command(
        spec.identity,
        Some(spec.instance),
        container_paths::ENTRYPOINT,
    );
    remove_ambient_capability_env(&mut cmd);
    for arg in agent_model_args(spec.agent, spec.model) {
        cmd.arg(arg);
    }
    for name in jackin_core::account_env_names() {
        cmd.env_remove(name);
    }
    for agent in jackin_core::Agent::ALL {
        if let Some(var) = agent.runtime().state_paths().folder_env_var {
            cmd.env_remove(var.name);
        }
    }
    for (k, v) in spec.env_passthrough {
        if !jackin_core::is_account_env(k) && !is_folder_env(k) && !is_explicit_capability_env(k) {
            cmd.env(k, v);
        }
    }
    apply_lane_env(&mut cmd, spec.agent, spec.model, spec.effort);
    if let Some(agent) = jackin_core::Agent::from_slug(spec.agent)
        && let Some(var) = agent.runtime().state_paths().folder_env_var
    {
        // Claude atomically replaces onboarding metadata; keep it inside the
        // durable directory mount rather than a file mounted at the home root.
        cmd.env(var.name, spec.home_dir);
    }
    // `HOME` was stripped with the other account-owned roots above; point it
    // at this instance's home so shells, hooks, and `$HOME`-relative tool
    // state land in the pane's own writable slot root — never in another
    // account's home and never in the read-only `/home/agent`.
    cmd.env("HOME", spec.home_dir);
    cmd.env("JACKIN_AGENT", spec.agent);
    cmd.env(jackin_protocol::INSTANCE_ENV, spec.instance);
    cmd.env(
        jackin_protocol::INSTANCE_FORWARDED_DIR_ENV,
        spec.forwarded_dir,
    );
    if let Some(auth_mode) = spec.auth_mode {
        cmd.env(jackin_protocol::AUTH_MODE_ENV, auth_mode);
    } else {
        cmd.env_remove(jackin_protocol::AUTH_MODE_ENV);
    }
    cmd.env("JACKIN_AGENT_CODENAME", spec.codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(spec.cwd);
    cmd
}

fn agent_model_args<'a>(agent: &str, model: Option<&'a str>) -> Vec<&'a str> {
    let Some(model) = model else {
        return Vec::new();
    };
    match agent {
        "claude" | "kimi" | "omp" | "hermes" => vec!["--model", model],
        "codex" | "opencode" | "grok" => vec!["-m", model],
        _ => Vec::new(),
    }
}

/// Inject model and reasoning settings for this instance only. The host launch
/// env is intentionally not used: two same-agent slots may route to different
/// endpoints/models, so a process-wide value would make the hook and child
/// command disagree.
fn apply_lane_env(
    cmd: &mut CommandBuilder,
    agent: &str,
    model: Option<&str>,
    effort: Option<&str>,
) {
    for name in [
        jackin_core::CODEX_LANE_MODEL_ENV_NAME,
        jackin_core::CODEX_LANE_EFFORT_ENV_NAME,
        jackin_core::CLAUDE_MODEL_ENV_NAME,
        jackin_core::CLAUDE_EFFORT_ENV_NAME,
    ] {
        cmd.env_remove(name);
    }
    let (model_env, effort_env) = match agent {
        "codex" => (
            jackin_core::CODEX_LANE_MODEL_ENV_NAME,
            jackin_core::CODEX_LANE_EFFORT_ENV_NAME,
        ),
        "claude" => (
            jackin_core::CLAUDE_MODEL_ENV_NAME,
            jackin_core::CLAUDE_EFFORT_ENV_NAME,
        ),
        _ => return,
    };
    if let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) {
        cmd.env(model_env, model);
    }
    if let Some(effort) = effort {
        cmd.env(effort_env, effort);
    }
}

/// Build a `CommandBuilder` for an interactive shell session.
///
/// See `build_agent_command` for the `cwd` rationale.
#[must_use]
pub fn build_shell_command(
    env_passthrough: &[(String, String)],
    cwd: &Path,
    codename: &str,
    identity: jackin_protocol::SessionIdentity,
) -> CommandBuilder {
    let shell = shell_executable();
    let mut cmd = isolated_command(identity, None, &shell);
    remove_ambient_capability_env(&mut cmd);
    for name in jackin_core::account_env_names() {
        cmd.env_remove(name);
    }
    // Shells have no instance home: restore the daemon's container `HOME`
    // (`/home/agent`, container-controlled rather than operator-controlled)
    // so the shell and its tools resolve the shared container home instead
    // of running with no home at all.
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    for (k, v) in env_passthrough {
        if !jackin_core::is_account_env(k) && !is_explicit_capability_env(k) {
            cmd.env(k, v);
        }
    }
    cmd.env_remove("JACKIN_AGENT");
    cmd.env("JACKIN_AGENT_CODENAME", codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(cwd);
    cmd
}

fn is_explicit_capability_env(name: &str) -> bool {
    EXPLICIT_CAPABILITY_ENV_NAMES.contains(&name)
}

fn remove_ambient_capability_env(cmd: &mut CommandBuilder) {
    for name in EXPLICIT_CAPABILITY_ENV_NAMES {
        cmd.env_remove(name);
    }
}

/// Build the internal root-supervisor wrapper command. The wrapper validates
/// the identity against the launch config, installs Landlock, drops to the
/// slot UID, and only then executes the requested program.
fn isolated_command(
    identity: jackin_protocol::SessionIdentity,
    instance: Option<&str>,
    program: impl AsRef<std::ffi::OsStr>,
) -> CommandBuilder {
    #[cfg(test)]
    {
        // Session unit tests run on the host, where the container-only capsule
        // binary and entrypoint paths do not exist. The production path below
        // is exercised by the dedicated process-isolation boundary tests.
        let _ = (identity, instance);
        CommandBuilder::new(program)
    }
    #[cfg(not(test))]
    {
        let mut cmd = CommandBuilder::new(container_paths::CAPSULE_BIN);
        cmd.args(isolated_wrapper_args(identity, instance, program));
        cmd
    }
}

/// Exact argv passed to the capsule root supervisor for one session. Kept
/// pure so tests can prove the admitted identity is actually wired into the
/// production spawn command even though host-side PTY tests bypass the
/// container-only wrapper.
fn isolated_wrapper_args(
    identity: jackin_protocol::SessionIdentity,
    instance: Option<&str>,
    program: impl AsRef<std::ffi::OsStr>,
) -> Vec<std::ffi::OsString> {
    vec![
        "__isolated-exec".into(),
        instance.unwrap_or("-").into(),
        identity.uid.to_string().into(),
        identity.gid.to_string().into(),
        program.as_ref().to_owned(),
    ]
}

#[cfg(not(test))]
fn shell_executable() -> std::ffi::OsString {
    "/bin/zsh".into()
}

#[cfg(test)]
fn shell_executable() -> std::ffi::OsString {
    "/bin/sh".into()
}

/// Apply the stable pane terminal environment. The active outer terminal is
/// reported per attach through the Capsule protocol; pane PTYs keep a
/// conservative baseline so a running session can be reattached from Ghostty,
/// Kitty, iTerm, Warp, or any other xterm-compatible client without retaining
/// assumptions from the terminal that launched the container. `COLORTERM`
/// intentionally advertises jackin❯'s 24-bit color path without tying the pane
/// to a host-specific terminfo entry.
fn apply_terminal_env(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    for key in ["LANG", "LC_ALL"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

#[cfg(test)]
mod tests;

/// Inject only the account selected for this pane — the env of one instance
/// config ID — after ambient credentials were stripped.
pub(crate) fn apply_account_env(
    command: &mut CommandBuilder,
    instance: &str,
    auth_mode: Option<&str>,
    credentials: &jackin_protocol::AgentCredentialEnv,
) {
    if !matches!(auth_mode, Some("api_key" | "oauth_token")) {
        return;
    }
    if let Some(env) = credentials.for_instance(instance) {
        for (name, value) in env {
            command.env(name, value);
        }
    }
}
