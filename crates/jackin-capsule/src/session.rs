//! Per-agent PTY session: spawn, resize, write input, read output, and track
//! session state for the daemon.
//!
//! Not responsible for: attach-client I/O, socket framing, or daemon
//! multiplexing logic.
//!
//! Key invariant: the session's `DamageGrid` is the single source of truth
//! for re-rendering on tab/pane switch and client reattach.

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

use crate::protocol::AgentState;
use crate::pull_request::PullRequestInfo;
use crate::tui::render::RowSnapshot;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Lines of scrollback every PTY session retains. ~1.5 MB worst-case
/// per session at 200 cols. Empty cells cost less. Operators need
/// scrollback to read Codex / Claude responses that exceed one
/// viewport, so this stays generous.
pub const SCROLLBACK_LEN: usize = 10_000;

pub const SESSION_ENV_PASSTHROUGH: &[&str] = &[
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GH_TOKEN",
    "JACKIN_DEBUG",
    "JACKIN_GIT_COAUTHOR_TRAILER",
    "JACKIN_GIT_DCO",
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
fn osc8_uri_is_safe(uri: &str) -> bool {
    if uri.is_empty() {
        return true;
    }
    let lower = uri.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Parse an `OSC 7` payload into a local-filesystem path. `OSC 7`
/// canonically arrives as `file://<host>/<percent-encoded-path>`;
/// `url::Url` does the percent-decoding and host-stripping in one
/// pass. Returns `None` for any payload that does not parse as a
/// `file://` URL — silently trusting arbitrary text would let an
/// agent overwrite the pane title with whatever it pleased.
fn parse_osc7(payload: &str) -> Option<String> {
    let url = url::Url::parse(payload).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Per-OSC operator opt-out switches. All default to `allow`; the
/// values `deny`, `off`, `no` (case-sensitive) turn the matching
/// passthrough off when the operator runs an untrusted role. tmux
/// exposes the same family as `set-clipboard on|off` plus
/// `allow-passthrough` for OSC; jackin keeps the surface per-OSC so
/// the operator can leave the agent's terminal title alone but block
/// notification spam, or vice versa.
const ENV_OSC52: &str = "JACKIN_OSC52";
const ENV_OSC_TITLE: &str = "JACKIN_OSC_TITLE";
const ENV_OSC_NOTIFY: &str = "JACKIN_OSC_NOTIFY";
const ENV_OSC_HYPERLINK: &str = "JACKIN_OSC_HYPERLINK";

#[derive(Debug, Clone, Copy)]
pub struct OscPolicy {
    allow_title: bool,
    allow_osc52: bool,
    allow_notify: bool,
    allow_hyperlink: bool,
}

impl Default for OscPolicy {
    fn default() -> Self {
        Self {
            allow_title: true,
            allow_osc52: true,
            allow_notify: true,
            allow_hyperlink: true,
        }
    }
}

impl OscPolicy {
    /// Read policy from environment. Cached at `Session::spawn` time so a
    /// background pane cannot toggle the gate at runtime by `export`ing
    /// into a focused shell.
    pub fn from_env() -> Self {
        Self {
            allow_title: !is_env_deny(ENV_OSC_TITLE),
            allow_osc52: !is_env_deny(ENV_OSC52),
            allow_notify: !is_env_deny(ENV_OSC_NOTIFY),
            allow_hyperlink: !is_env_deny(ENV_OSC_HYPERLINK),
        }
    }

    pub fn allow_title(self) -> bool {
        self.allow_title
    }
    pub fn allow_osc52(self) -> bool {
        self.allow_osc52
    }
    pub fn allow_notify(self) -> bool {
        self.allow_notify
    }
    pub fn allow_hyperlink(self) -> bool {
        self.allow_hyperlink
    }

    /// Test-only constructor with every passthrough gate closed.
    /// Production code must call `from_env()`; the `#[doc(hidden)]`
    /// attribute hides this from rustdoc and the `for_test_` prefix
    /// flags intent to readers. Cargo cannot list a crate in its own
    /// `[dev-dependencies]` with a feature flag, so a `#[cfg(feature
    /// = "test-helpers")]` gate would break the default `cargo test`
    /// invocation that integration tests rely on.
    #[doc(hidden)]
    pub fn for_test_deny_all() -> Self {
        Self {
            allow_title: false,
            allow_osc52: false,
            allow_notify: false,
            allow_hyperlink: false,
        }
    }
}

fn is_env_deny(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("deny" | "off" | "no"))
}

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

#[expect(
    missing_debug_implementations,
    reason = "Session owns PTY and child-killer trait objects; capsule logs expose session identity and state."
)]
pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub provider: Option<SessionProvider>,
    pub state: AgentState,
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
    pub fn spawn(
        label: impl Into<String>,
        agent: Option<String>,
        provider: Option<SessionProvider>,
        cmd: CommandBuilder,
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

        let sid = next_id();
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
    /// readline/TUI cursor state does not desynchronise from jackin's
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
        crate::tui::render::pane_content_from_damagegrid(&self.shadow_grid, viewport_cols)
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

    pub fn send_input(&self, data: &[u8]) {
        // Debug-only: log every byte chunk forwarded to a PTY. Pairs
        // with the `rx ClientFrame::Input` line on the receive side so
        // a `--debug` trace shows the full path from operator keystroke
        // to slave fd write.
        crate::cdebug!(
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
        if let Err(e) = self.input_tx.send(data.to_vec()) {
            crate::clog!(
                "session send_input: writer task gone ({} bytes dropped): {e}",
                data.len()
            );
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
        crate::cdebug!(
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
/// intentionally advertises jackin's 24-bit color path without tying the pane
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
