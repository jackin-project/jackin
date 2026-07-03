// Per-agent PTY session: spawn, resize, write input, read output, and track
// session state for the daemon.

#[path = "session/osc_policy.rs"]
mod osc_policy;

#[allow(unused_imports, unreachable_pub)]
pub use osc_policy::{OscPolicy, osc8_uri_is_safe, parse_osc7};

//
// Not responsible for: attach-client I/O, socket framing, or daemon
// multiplexing logic.
//
// Key invariant: the session's `DamageGrid` is the single source of truth
// for re-rendering on tab/pane switch and client reattach.

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
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
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
    "GH_TOKEN",
    "JACKIN_DEBUG",
    "JACKIN_GIT_COAUTHOR_TRAILER",
    "JACKIN_GIT_DCO",
    "TZ",
    // Per-tab provider injection — Anthropic-compatible backends (Claude Code).
    // Listed here so env_for_spawn's allowlist accepts them as overrides when the
    // operator picks an alternative provider in the AgentPicker flow.
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    // Model-tier mapping so Claude Code maps its internal tiers to provider model names.
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    // Provider operational env vars.
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    // MiniMax key forwarded into Codex so its config.toml `env_key = "MINIMAX_API_KEY"` resolves.
    "MINIMAX_API_KEY",
    // Codex v2 profile name injected by the capsule when the operator picks an
    // alt provider (e.g. "minimax"). The entrypoint passes it as --profile <name>.
    "JACKIN_CODEX_PROFILE",
    // Kimi key — serves both the Kimi Code runtime agent and the Kimi Claude Code provider.
    "KIMI_CODE_API_KEY",
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
}

#[expect(
    missing_debug_implementations,
    reason = "Session owns PTY and child-killer trait objects; capsule logs expose session identity and state."
)]
pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub provider: Option<SessionProvider>,
    /// Published effective state. Authored solely by evidence arbitration on the
    /// daemon tick (see `agent_status`); kept in sync with `status.effective`.
    pub state: AgentState,
    /// Per-session evidence-arbitration status (raw state, confidence, seen,
    /// revision, last evidence summary). The single source of `state`.
    pub status: SessionStatus,
    /// Debounce bookkeeping for the inferred working→idle hold.
    pub pending_transition: crate::agent_status::policy::PendingTransition,
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
    /// evidence snapshot. The agent-authored signals (title, OSC 9;4 progress)
    /// are wiped when the foreground is no longer the agent; the shell-authored
    /// OSC 133 `shell_state` persists (it belongs to the shell, not the agent).
    osc: crate::agent_status::evidence::OscEvidence,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child_killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
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
    pub shadow_grid: Box<jackin_term::DamageGrid>,
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
    pub fn branch_name(&self) -> Option<&BranchName> {
        match self {
            Self::Branch { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn head(&self) -> Option<&Oid> {
        match self {
            Self::Detached { head } => Some(head),
            Self::Branch {
                head: Some(head), ..
            } => Some(head),
            _ => None,
        }
    }

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
    pub fn parse(value: &str) -> Option<Self> {
        if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

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
    pub row_arena: jackin_term::RowArena,
    /// Attach client's terminal default colors; the grid reports these to
    /// agent OSC 10/11 queries. `None` leaves the grid's dark-theme default.
    pub default_fg: Option<(u8, u8, u8)>,
    pub default_bg: Option<(u8, u8, u8)>,
}

impl Session {
    #[allow(
        clippy::excessive_nesting,
        reason = "Session spawn wires PTY + child handle + agent + env into the \
                  multiplexer state. The nested `is_err` + `crate::clog!` + state- \
                  update branches are the per-stage error-reporting protocol."
    )]
    #[allow(
        clippy::too_many_lines,
        reason = "Same justification as the too_many_lines + excessive_nesting \
              allows: session spawn wires PTY + child handle + agent + env into \
              the multiplexer state. Inline shape preserves captured-runtime \
              state across the per-stage error-reporting branches."
    )]
    pub fn spawn(
        label: impl Into<String>,
        agent: Option<String>,
        provider: Option<SessionProvider>,
        mut cmd: CommandBuilder,
        terminal: SessionTerminal,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
    ) -> Result<(Self, u64)> {
        let label = label.into();
        // Per-tab trace: each pane/agent spawn is its own short trace on the
        // session timeline (shares the resource session.id).
        jackin_diagnostics::record_capsule_activity(&label, agent.as_deref());
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
        inject_status_env(&mut cmd, sid, agent.as_deref());

        let mut child = slave
            .spawn_command(cmd)
            .context("failed to spawn session process")?;
        let child_pid = child.process_id();
        if let Some(pid) = child_pid {
            crate::pid1::register_managed_child(pid);
        }
        let child_killer = Arc::new(Mutex::new(child.clone_killer()));
        drop(slave);

        let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(master));
        let master_for_read = Arc::clone(&master);
        let master_for_write = Arc::clone(&master);

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let event_tx_output = event_tx.clone();
        let event_tx_exit = event_tx.clone();
        let event_tx_writer_err = event_tx.clone();

        // PTY writer task. take_writer / lock failures emit Exited so the
        // daemon reaps the half-initialised session instead of leaving a
        // tab whose input keystrokes silently vanish. blocking_recv is
        // used instead of Handle::current().block_on(rx.recv()) because
        // the latter panics inside spawn_blocking on a current-thread
        // runtime ("Cannot block the current thread from within a runtime").
        tokio::task::spawn_blocking(move || {
            let writer = match master_for_write.lock() {
                Err(_) => {
                    crate::clog!("session {sid}: PTY master mutex poisoned; aborting writer task");
                    None
                }
                Ok(guard) => match guard.take_writer() {
                    Ok(w) => Some(w),
                    Err(e) => {
                        crate::clog!(
                            "session {sid}: take_writer failed: {e}; aborting writer task"
                        );
                        None
                    }
                },
            };
            let Some(mut writer) = writer else {
                if event_tx_writer_err
                    .send(SessionEvent::Exited {
                        session_id: sid,
                        reason: Some("session PTY writer failed to initialize".to_owned()),
                    })
                    .is_err()
                {
                    crate::clog!(
                        "session {sid}: event channel closed — daemon will not reap this half-initialised session"
                    );
                }
                return;
            };
            while let Some(data) = input_rx.blocking_recv() {
                if let Err(e) = std::io::Write::write_all(&mut writer, &data) {
                    crate::clog!(
                        "session {sid}: PTY write error: {e} (errno={:?}); aborting writer",
                        e.raw_os_error()
                    );
                    if event_tx_writer_err
                        .send(SessionEvent::Exited {
                            session_id: sid,
                            reason: Some(format!("session PTY write failed: {e}")),
                        })
                        .is_err()
                    {
                        crate::clog!(
                            "session {sid}: event channel closed — daemon will not reap this dead writer"
                        );
                    }
                    return;
                }
            }
        });

        let event_tx_reader_err = event_tx.clone();
        tokio::task::spawn_blocking(move || {
            let reader = match master_for_read.lock() {
                Err(_) => {
                    crate::clog!("session {sid}: PTY master mutex poisoned; aborting reader task");
                    None
                }
                Ok(guard) => match guard.try_clone_reader() {
                    Ok(r) => Some(r),
                    Err(e) => {
                        crate::clog!(
                            "session {sid}: try_clone_reader failed: {e}; aborting reader task"
                        );
                        None
                    }
                },
            };
            let Some(mut reader) = reader else {
                if event_tx_reader_err
                    .send(SessionEvent::Exited {
                        session_id: sid,
                        reason: Some("session PTY reader failed to initialize".to_owned()),
                    })
                    .is_err()
                {
                    crate::clog!(
                        "session {sid}: event channel closed — daemon will not reap this half-initialised session"
                    );
                }
                return;
            };
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => {
                        crate::clog!("session {sid}: PTY read EOF");
                        break;
                    }
                    Err(e) => {
                        crate::clog!(
                            "session {sid}: PTY read error: {e} (errno={:?})",
                            e.raw_os_error()
                        );
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if event_tx_output
                            .send(SessionEvent::Output {
                                session_id: sid,
                                data,
                            })
                            .is_err()
                        {
                            crate::clog!(
                                "session {sid}: event channel closed before PTY output drained; reader exiting"
                            );
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
        tokio::task::spawn_blocking(move || {
            let status = child.wait();
            if let Some(pid) = child_pid {
                crate::pid1::unregister_managed_child(pid);
                crate::pid1::reap_zombies();
            }
            crate::clog!("session {sid}: child reaped: {status:?}");
            if event_tx_exit
                .send(SessionEvent::Exited {
                    session_id: sid,
                    reason: child_exit_reason(status.as_ref()),
                })
                .is_err()
            {
                crate::clog!(
                    "session {sid}: event channel closed — daemon will not see this child exit"
                );
            }
        });

        Ok((
            Session {
                label,
                agent,
                provider,
                state: AgentState::Unknown,
                status: SessionStatus::new(),
                pending_transition: crate::agent_status::policy::PendingTransition::default(),
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
                last_output_at: std::time::Instant::now(),
                last_input_at: std::time::Instant::now(),
                received_output: false,
                shadow_grid: {
                    let mut grid = Box::new(jackin_term::DamageGrid::with_row_arena(
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
        let mut tail = jackin_tui::scroll::TailScroll::new(before);
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
        self.send_input(b"\x0c");
    }

    /// Number of scrollback lines currently retained for this pane.
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

    pub fn hyperlink_target_at_content_row(&self, row: usize, col: u16) -> Option<&str> {
        self.shadow_grid.hyperlink_target_at_content_row(row, col)
    }

    pub fn send_input(&self, data: &[u8]) -> bool {
        // Debug-only: log every byte chunk forwarded to a PTY. Pairs
        // with the `rx ClientFrame::Input` line on the receive side so
        // a `--debug` trace shows the full path from operator keystroke
        // to slave fd write.
        crate::ctrace_payload!(
            "session send_input: agent={:?} label={} bytes={:02x?}",
            self.agent,
            self.label,
            data
        );
        // SendError fires when the writer task has exited (it owns the
        // receiver). The writer task emits SessionEvent::Exited before
        // dropping, so the daemon will reap this Session on the next
        // event tick — keystrokes accepted between writer death and
        // reap are lost, but observability remains: clog records both
        // halves of the failure chain.
        match self.input_tx.send(data.to_vec()) {
            Ok(()) => true,
            Err(e) => {
                crate::clog!(
                    "session send_input: writer task gone ({} bytes dropped): {e}",
                    data.len()
                );
                false
            }
        }
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
        now: std::time::Instant,
    ) {
        use crate::agent_status::evidence::AuthorityEvidence;
        use crate::agent_status::gating::{GateEffect, RuntimeEvent, map_event};

        let gate = self.gate_states.entry(source_id.to_owned()).or_default();
        let effect = map_event(&RuntimeEvent { runtime, event }, gate);
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
            GateEffect::Ignore => {
                // An event this build does not map (runtime/version skew renamed
                // it). The reporter's authority silently goes dark; leave a
                // firehose breadcrumb so JACKIN_DEBUG=1 surfaces the drift.
                crate::cdebug!(
                    "agent-status: unmapped runtime event runtime={runtime} event={event} \
                     source={source_id}"
                );
            }
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
    pub fn osc_evidence(&self) -> &crate::agent_status::evidence::OscEvidence {
        &self.osc
    }

    /// Plain-text rows of the current visible viewport (top to bottom), for the
    /// screen rule-pack engine. Operator scrollback never affects detection —
    /// only the live screen is read.
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
        use crate::agent_status::evidence::ProcessEvidence;
        use crate::agent_status::process::{
            self, descendant_process_count, detect_foreground_agent, physics_available,
            read_process_info, sample_cpu_jiffies_delta,
        };

        let Some(pid) = self.child_pid else {
            return ProcessEvidence::default();
        };
        if !physics_available() {
            return ProcessEvidence::default();
        }
        let Some(info) = read_process_info(pid) else {
            // Linux + PID gone = a real process exit.
            self.cpu_sample = None;
            return ProcessEvidence {
                process_exited: true,
                physics_sampled: true,
                ..ProcessEvidence::default()
            };
        };

        let foreground = detect_foreground_agent(&info);
        let foreground_is_agent = foreground.is_agent();
        let foreground_pgid = foreground.pgid();
        let child_process_count = descendant_process_count(pid);
        let cpu_jiffies_delta = sample_cpu_jiffies_delta(pid, &mut self.cpu_sample, now);
        let root_is_agent = process::identify_agent(&info).is_some();

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
        use crate::agent_status::arbitrate::arbitrate;
        use crate::agent_status::evidence::{
            ActivityEvidence, EvidenceNote, EvidenceSnapshot, ScreenEvidence,
        };
        use crate::agent_status::policy::{apply_watchdog, debounce};
        use crate::agent_status::rules::VirtualRegions;

        let process = self.sample_process_evidence(now);
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
            }
        }
        if exiting {
            self.clear_runtime_authority();
        }
        StatusTick { transition, stuck }
    }

    /// True when the session's program has enabled any mouse protocol
    /// mode. Used by the daemon to decide whether selection gestures
    /// belong to jackin or to the pane. Actual PTY mouse forwarding
    /// also consults `mouse_protocol_mode()` so press-only programs
    /// do not receive motion events.
    pub fn mouse_enabled(&self) -> bool {
        !matches!(
            self.shadow_grid.mouse_protocol_mode(),
            jackin_term::MouseProtocolMode::None
        )
    }

    pub fn mouse_protocol_encoding(&self) -> jackin_term::MouseProtocolEncoding {
        self.shadow_grid.mouse_protocol_encoding()
    }

    pub fn mouse_protocol_mode(&self) -> jackin_term::MouseProtocolMode {
        self.shadow_grid.mouse_protocol_mode()
    }

    /// True when the session enabled DEC private mode `?1004` (focus
    /// event reporting).
    pub fn focus_events_enabled(&self) -> bool {
        self.shadow_grid.focus_events()
    }

    /// True when the terminal is in the alternate screen.
    pub fn alternate_screen(&self) -> bool {
        self.shadow_grid.alternate_screen()
    }

    /// True when the foreground program has bracketed-paste enabled.
    pub fn bracketed_paste(&self) -> bool {
        self.shadow_grid.bracketed_paste()
    }

    /// True when the foreground program has application-cursor-keys mode on.
    pub fn application_cursor(&self) -> bool {
        self.shadow_grid.application_cursor()
    }

    /// Feed PTY bytes into the grid and update activity timestamps.
    pub fn feed_pty(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.received_output = true;
        }
        crate::ctrace_payload!(
            "session feed_pty bytes: agent={:?} label={} len={} bytes={:02x?}",
            self.agent,
            self.label,
            bytes.len(),
            bytes
        );

        // Single batch feed — the grid's persistent vte parser handles
        // sequences split across PTY read boundaries internally.
        let was_alternate = self.shadow_grid.alternate_screen();
        let was_scrolled = self.scrollback_offset() != 0;
        let scrollback_before = self.shadow_grid.scrollback_len();
        let debug_enabled = crate::logging::debug_enabled();
        let parse_started = debug_enabled.then(std::time::Instant::now);
        self.shadow_grid.process(bytes);
        let parse_duration_us = parse_started.map(|started| started.elapsed().as_micros());
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

        if debug_enabled {
            let (grid_rows, grid_cols) = self.shadow_grid.size();
            let (cursor_row, cursor_col) = self.shadow_grid.cursor_position();
            crate::cdebug!(
                "session feed_pty: agent={:?} label={} bytes={} t_parse_us={} alt_screen={} mouse_enabled={} screen={}x{} cursor={}x{} scrollback={} scrollback_offset={}",
                self.agent,
                self.label,
                bytes.len(),
                parse_duration_us.unwrap_or_default(),
                is_alternate,
                self.mouse_enabled(),
                grid_rows,
                grid_cols,
                cursor_row,
                cursor_col,
                self.shadow_grid.scrollback_len(),
                self.scrollback_offset(),
            );
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
        use jackin_term::PassthroughEvent;
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
                    // raw stream in `feed_pty` — jackin-term does not surface it
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
                // Default-denied CSI (§3.6): never forwarded. Logged so a
                // `--debug` run shows the exact dropped bytes — the triage
                // trail for "agent feature X stopped working" and the input
                // for allowlist additions.
                PassthroughEvent::DroppedCsi(ref raw) => {
                    crate::cdebug!(
                        "dropped unhandled CSI (agent={:?}): {}",
                        self.agent.as_deref(),
                        raw.escape_ascii(),
                    );
                }
                // Device/mode query the emulator answered itself. The reply
                // goes back to the agent's own PTY stdin — never the outer
                // terminal — so the agent's capability detection reflects the
                // grid, not the host. (Root fix for the alt-screen corruption:
                // the host was answering DA/DSR/DECRQM with its own caps.)
                PassthroughEvent::Reply(bytes) => {
                    crate::cdebug!(
                        "query reply to agent={:?}: {}",
                        self.agent.as_deref(),
                        bytes.escape_ascii(),
                    );
                    if let Err(e) = self.input_tx.send(bytes) {
                        crate::clog!(
                            "session query reply (agent={:?} label={}): writer task gone: {e}",
                            self.agent,
                            self.label,
                        );
                    }
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
        crate::cdebug!(
            "forwarding allowlisted CSI to client (agent={:?}): {}",
            self.agent.as_deref(),
            raw.escape_ascii(),
        );
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

    pub fn allow_frame_hyperlinks(&self) -> bool {
        self.osc_policy.allow_hyperlink()
    }

    pub fn terminate(&self) {
        match self.child_killer.lock() {
            Ok(mut killer) => {
                if let Err(e) = killer.kill() {
                    crate::clog!("session terminate: child kill failed: {e}");
                }
            }
            Err(e) => crate::clog!("session terminate: child killer mutex poisoned: {e}"),
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Most recently announced working directory (OSC 7), if any.
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        // A pane collapsed below its border height yields a 0-row inner rect.
        // Never hand the agent PTY a 0×0 window size (programs expect ≥1) nor the
        // shadow grid a degenerate geometry. `DamageGrid::set_size` clamps too;
        // this keeps TIOCSWINSZ and the model in agreement on the floor.
        if rows == 0 || cols == 0 {
            // A clamp here means a layout bug upstream collapsed a pane; log it
            // so a soak run can pin the offending frame rather than silently
            // running the agent with a collapsed dimension. Each axis is floored
            // independently, so `0x80` becomes `1x80`, not `1x1`.
            crate::cdebug!(
                "resize-clamp: degenerate geometry {rows}x{cols} floored to {}x{}",
                rows.max(1),
                cols.max(1),
            );
        }
        let rows = rows.max(1);
        let cols = cols.max(1);
        // TIOCSWINSZ failure leaves the agent drawing at the old size
        // while the screen renders at the new geometry — the operator
        // sees mis-wrapped lines with no explanation. Log so --debug
        // surfaces the divergence. Lock failure is logged too: a
        // poisoned PTY mutex means an earlier writer/reader task
        // panicked while holding it, and the session is effectively
        // dead even if no Exited event has fired yet.
        match self.pty_master.lock() {
            Ok(master) => {
                if let Err(e) = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                }) {
                    crate::clog!("session resize: TIOCSWINSZ failed for {rows}x{cols}: {e}");
                }
            }
            Err(e) => crate::clog!("session resize: PTY mutex poisoned: {e}"),
        }
        self.shadow_grid.set_size(rows, cols);
        // Re-clamp through the grid: set_size may have shrunk the filled
        // scrollback the offset was clamped against.
        self.shadow_grid.set_scrollback(self.scrollback_offset());
    }
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
    #[allow(clippy::too_many_arguments)]
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
            provider,
            state: AgentState::Unknown,
            status: SessionStatus::new(),
            pending_transition: crate::agent_status::policy::PendingTransition::default(),
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
            last_output_at: std::time::Instant::now(),
            last_input_at: std::time::Instant::now(),
            received_output: true,
            shadow_grid: Box::new(jackin_term::DamageGrid::new(size.0, size.1, scrollback_len)),
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

/// Reject agent-slug strings that are flags (start with `-`), empty,
/// contain whitespace / control characters, or — when the launch
/// config lists supported agents — do not appear in that allowlist.
/// Shared by the PID-1 argv path, the
/// `jackin-capsule new <agent>` client path, and the daemon's
/// `Hello.spawn` decode path so all three trust boundaries
/// apply the same gate.
pub fn validate_agent_slug<'a>(
    raw: &'a str,
    supported_agents: &[String],
) -> Result<&'a str, &'static str> {
    if raw.is_empty() {
        return Err("empty value");
    }
    if raw.starts_with('-') {
        return Err("looks like a flag");
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("contains whitespace or control characters");
    }
    if !supported_agents.is_empty() && !supported_agents.iter().any(|a| a == raw) {
        return Err("not in launch config allowlist");
    }
    Ok(raw)
}

/// Inject the agent-status reporter environment into a session's command,
/// keyed on the session id assigned at spawn. Agent panes get the full set so
/// hook/plugin reporters can address this session; shell panes get only the
/// socket var (no runtime to report for). State is never authored from these —
/// reporters forward events, the daemon maps and gates them.
fn inject_status_env(cmd: &mut CommandBuilder, session_id: u64, agent: Option<&str>) {
    cmd.env("JACKIN_STATUS_SOCKET", crate::socket::SOCKET_PATH);
    if let Some(runtime) = agent {
        cmd.env("JACKIN_SESSION_ID", session_id.to_string());
        cmd.env("JACKIN_AGENT_RUNTIME", runtime);
        cmd.env(
            "JACKIN_STATUS_SOURCE",
            format!("hook-{runtime}-{session_id}"),
        );
    } else {
        cmd.env_remove("JACKIN_SESSION_ID");
        cmd.env_remove("JACKIN_AGENT_RUNTIME");
        cmd.env_remove("JACKIN_STATUS_SOURCE");
    }
}

/// Authority grade for a runtime's semantic source. `opencode` ships a complete
/// lifecycle event stream (Complete); `amp` and other event sources have partial
/// coverage. Claude/Codex are identity-only (Decision 0a) and never reach this.
fn grade_for_runtime(runtime: &str) -> crate::agent_status::evidence::AuthorityGrade {
    use crate::agent_status::evidence::AuthorityGrade;
    match runtime {
        "opencode" => AuthorityGrade::Complete,
        _ => AuthorityGrade::Partial,
    }
}

/// Build a `CommandBuilder` for an agent session.
///
/// Entrypoint is `/jackin/runtime/entrypoint.sh` with `JACKIN_AGENT=<slug>`.
/// `cwd` is the workspace workdir from the Capsule launch config. It must be
/// passed explicitly: `portable_pty`'s `CommandBuilder`
/// defaults the child's cwd to `$HOME` when none is set — it does not
/// inherit the daemon's cwd — so omitting this would land every agent in
/// `/home/agent` regardless of the workspace.
pub fn build_agent_command(
    agent: &str,
    model: Option<&str>,
    env_passthrough: &[(String, String)],
    cwd: &Path,
    codename: &str,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/jackin/runtime/entrypoint.sh");
    for arg in agent_model_args(agent, model) {
        cmd.arg(arg);
    }
    for (k, v) in env_passthrough {
        cmd.env(k, v);
    }
    cmd.env("JACKIN_AGENT", agent);
    cmd.env("JACKIN_AGENT_CODENAME", codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(cwd);
    cmd
}

fn agent_model_args<'a>(agent: &str, model: Option<&'a str>) -> Vec<&'a str> {
    let Some(model) = model else {
        return Vec::new();
    };
    match agent {
        "claude" | "kimi" => vec!["--model", model],
        "codex" | "opencode" | "grok" => vec!["-m", model],
        _ => Vec::new(),
    }
}

/// Build a `CommandBuilder` for an interactive shell session.
///
/// See `build_agent_command` for the `cwd` rationale.
pub fn build_shell_command(
    env_passthrough: &[(String, String)],
    cwd: &Path,
    codename: &str,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/zsh");
    for (k, v) in env_passthrough {
        cmd.env(k, v);
    }
    cmd.env_remove("JACKIN_AGENT");
    cmd.env("JACKIN_AGENT_CODENAME", codename);
    apply_terminal_env(&mut cmd);
    cmd.cwd(cwd);
    cmd
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
