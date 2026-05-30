/// The multiplexer daemon — runs as PID 1, manages sessions and clients.
///
/// Architecture:
///   - One active attach client at a time. A new `Hello` from a second
///     client sends `Shutdown` to the old one and aborts the old
///     client's reader task (see `attached_task`).
///   - Attach traffic uses the binary tag+length protocol in
///     `protocol::attach`. The hot path forwards raw PTY bytes without
///     base64 or JSON nesting.
///   - The control channel still speaks length-prefixed JSON for one-shot
///     `status` queries from the host CLI. Channel dispatch is by first
///     byte: `0x00` → control (length prefix), anything else → attach.
///   - Lifecycle: the daemon exits when the last session ends so the
///     container reaps cleanly. SIGTERM also triggers shutdown.
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::process::Command;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use portable_pty::CommandBuilder;

use crate::action::Action;
use crate::attach_protocol::{
    AttachHandshake, detach_attached_task, detach_client, drain_and_exit, handle_attach_client,
    initial_spawn_request, perform_handshake, spawn_request_label,
};
#[cfg(test)]
use crate::branch_context_bar::{
    BRANCH_CONTEXT_BAR_HOVER_BG, BRANCH_CONTEXT_BAR_HOVER_FG, branch_context_bar_layout,
};
use crate::branch_context_bar::{
    BRANCH_CONTEXT_BAR_ROWS, BranchContextBarHit, branch_context_bar_hit, render_branch_context_bar,
};
use crate::dialog::{
    ConfirmKind, Dialog, DialogAction, GithubContextView, PaletteCloseLabel, PaletteCommand,
    PickerIntent, PullRequestStatus, SplitDirection,
};
#[cfg(test)]
use crate::git_context::{
    PACKED_REFS_CACHE_MAX_ENTRIES, PACKED_REFS_MAX_BYTES, read_branch_from_git_head,
    read_context_from_git_metadata, read_git_ref_oid, read_packed_git_ref_oid,
    with_packed_refs_cache,
};
use crate::git_context::{
    WorkdirContext, git_current_context, resolve_default_branch, start_git_context_watcher,
};
use crate::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};
use crate::layout::{Direction, Rect, SplitOrient, SplitPosition, Tab};
use crate::mouse_protocol::{
    encode_mouse_for_protocol, encode_wheel_cursor_fallback, is_wheel_button,
    mouse_event_encoding_for_session, pane_wheel_cursor_fallback_reason,
};
#[cfg(test)]
use crate::mouse_protocol::{mouse_event_allowed_for_mode, push_xterm_mouse_number};
use crate::mux_mode::MuxMode;
use crate::pr_context::gh_pull_request_info;
use crate::protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_server,
};
use crate::protocol::control::{AgentState, SessionInfo};
use crate::render::{PaneBodyCache, PaneBodyDim, PaneBodyRenderMode, draw_scrollbar, fill_screen};
use crate::selection::{SelectionState, paint_selection_highlight, selection_text};
use crate::session::{
    BranchName, GitContext, Oid, PullRequestInfo, PullRequestLookupOutcome,
    SESSION_ENV_PASSTHROUGH, Session, SessionEvent, build_agent_command, build_shell_command,
};
use crate::socket;
use crate::statusbar::{STATUS_BAR_ROWS, StatusBar, draw_pane_box};
use crate::terminal_geometry::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};
use crate::title::{
    append_osc_window_title, capitalize, compose_outer_terminal_title, display_title,
    session_agent_label,
};

mod compositor;
mod context_mgmt;
mod dialog_mgmt;
mod input_dispatch;
mod mouse_input;
mod session_lifecycle;

struct SessionLaunch {
    label: String,
    cmd: CommandBuilder,
}

pub struct Multiplexer {
    sessions: HashMap<u64, Session>,
    tabs: Vec<Tab>,
    active_tab: usize,
    term_rows: u16,
    term_cols: u16,
    status_bar: StatusBar,
    /// LIFO stack of open dialogs. The top of stack is the live one
    /// the renderer paints and the input dispatcher routes keys to;
    /// older dialogs sit underneath waiting for an Esc-pop to surface
    /// them again. Sub-dialogs (Menu → New tab → AgentPicker,
    /// Menu → Split pane → SplitDirectionPicker → AgentPicker,
    /// Menu → Close → CloseTargetPicker / ConfirmClose, …) push onto
    /// this stack so Esc walks the operator back one step at a time
    /// instead of nuking the whole flow. The empty stack means "no
    /// dialog open" — every consumer treats `dialog_top()` as the
    /// canonical "is a dialog visible" check.
    dialog_stack: Vec<Dialog>,
    content_rows: u16,
    available_agents: Vec<String>,
    launch_config: CapsuleConfig,
    env_passthrough: Vec<(String, String)>,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    zoomed: Option<u64>,
    input_parser: InputParser,
    detach_requested: bool,
    pub(crate) attached_out: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// Latched true on the first `send_to_client` after `attached_out`
    /// was set: once the receiver drops mid-attach, every subsequent
    /// frame send into the same channel will also fail. Without this
    /// latch the per-tick redraw + per-PTY output + per-OSC repaint
    /// would write one `clog!` line each, swamping `multiplexer.log`.
    /// Cleared whenever `attached_out` is reassigned (next attach).
    pub(crate) attached_out_dead_logged: bool,
    /// JoinHandle of the spawned `handle_attach_client` task for the
    /// currently-attached client. Tracked so a takeover (second `Hello`)
    /// can abort the old task's reader loop — without the abort, the
    /// old client's stale Input / Resize / Detach frames keep flowing
    /// into the shared `cmd_tx` until its socket finally closes.
    pub(crate) attached_task: Option<tokio::task::JoinHandle<()>>,
    /// Records the previous tab-cell click so a second click on the
    /// same tab within `DOUBLE_CLICK_WINDOW` is treated as a
    /// double-click (open the rename modal).
    last_tab_click: Option<(usize, std::time::Instant)>,
    /// Active mouse-drag resize, if any. Populated when the operator
    /// presses the left button on a shared pane border; updated on
    /// every motion event; cleared on release.
    drag: Option<DragState>,
    /// Active mouse text selection on a pane whose program ignored
    /// the mouse. Updated on every motion event; copied to the
    /// outer clipboard via OSC 52 on release.
    selection: Option<SelectionState>,
    /// Last visible pane-body snapshot per session. PTY output can
    /// then repaint only rows whose vt100 cells changed.
    pane_body_caches: HashMap<u64, PaneBodyCache>,
    /// Pane bodies dirtied by PTY output. The render ticker drains
    /// this at most once per frame, preserving the existing coalescing
    /// behavior while avoiding broad body redraws.
    dirty_panes: HashSet<u64>,
    /// Named full-frame invalidation, used whenever partial pane-body
    /// repainting would be unsafe or when chrome/status/dialog/layout
    /// changed outside the pane body.
    pending_full_redraw: Option<FullRedrawReason>,
    /// Last pointer shape emitted through OSC 22. Stored so passive
    /// mouse motion does not spam the outer terminal with duplicate
    /// pointer-shape updates.
    pointer_shape: PointerShape,
    /// True only for outer terminals eligible for OSC 22 pointer-shape
    /// hints. Unsupported terminals keep normal cursor behavior.
    pointer_shapes_supported: bool,
    /// Terminal identity reported by the active attach client. Refreshed
    /// on every attach/takeover so daemon-owned output enhancements can
    /// follow the terminal the operator is using now rather than the
    /// terminal that launched the container.
    attached_terminal: ClientTerminal,
    /// Hash of the last multiplexer-owned OSC 2 title sent to the
    /// outer terminal. Gates re-emission on inequality: without the
    /// diff, every full frame would reassert the workspace/PR title
    /// and override per-pane agent-set titles in the outer terminal's
    /// tab list on every redraw. Reset to `None` when a child pane
    /// updates its own title so the next full frame re-asserts.
    last_outer_terminal_title: Option<String>,
    hover_target: Option<HoverTarget>,
    /// Deadline for hiding the transient "Copied!" badge in whichever
    /// dialog most recently performed a jackin-owned OSC 52 copy.
    dialog_copy_feedback_deadline: Option<Instant>,
    /// Branch rendered in the status bar; paired with
    /// `pull_request_context_head` as the cache key in
    /// `PullRequestContextCacheEntry::is_fresh`.
    pull_request_context_branch: Option<BranchName>,
    /// Resolved HEAD OID for `pull_request_context_branch` (or the
    /// detached-HEAD SHA when no symref). Same-branch HEAD movement
    /// (commit, rebase, force-push follow-up) flips this and busts any
    /// cached PR answer keyed on the prior head.
    pull_request_context_head: Option<Oid>,
    pull_request_context: Option<Arc<PullRequestInfo>>,
    /// State of the fast local git context lookup (`git_current_context`):
    /// monotonic request id, in-flight gate, last-run instant for the
    /// cooldown check. The result lands on `pull_request_context_branch`
    /// and `pull_request_context_head`.
    git_branch_lookup: LookupState,
    /// State of the 60 s `gh` PR-info lookup. Uses `request_id` +
    /// `in_flight`; `last_run` is unused (per-branch freshness lives
    /// in `pull_request_context_cache` instead because the operator
    /// can flip between branches with cached results in flight).
    pull_request_lookup: LookupState,
    pull_request_context_cache: HashMap<BranchName, PullRequestContextCacheEntry>,
    /// Workspace workdir read from `/jackin/run/agent.toml` at daemon startup.
    /// Every spawned PTY (agent or shell) receives this as its `cwd`
    /// so the operator's panes open in the workspace they configured
    /// instead of `$HOME` (portable_pty's CommandBuilder default).
    workdir: PathBuf,
    /// Resolved Z.AI API key from the operator env. `Some` when `ZAI_API_KEY`
    /// was set at launch time; drives the provider picker for supported agents.
    zai_key: Option<String>,
    /// Cached at construction for the hot polling path. The only
    /// mutation after that is `gh_available` flipping false → true when
    /// a background PR lookup succeeds, so a startup PATH /
    /// tool-availability race does not freeze PR discovery for the
    /// daemon lifetime.
    workdir_context: WorkdirContext,
    /// Ratatui terminal backed by [`SocketBackend`].
    ///
    /// Chrome widgets (status bar, pane boxes, dialogs) will progressively
    /// migrate from raw ANSI to this terminal so they can use shared
    /// `jackin-tui` components. For now it is initialized alongside the
    /// daemon and kept in sync with resize events but not yet used for
    /// frame output — the raw ANSI path in `compose_full_frame` /
    /// `compose_partial_frame` still owns all rendering.
    ratatui_terminal: ratatui::Terminal<crate::socket_backend::SocketBackend>,
}

/// Three book-keeping fields for a background context lookup. They
/// MUST move together: `begin_spawn` bumps `request_id`, stamps
/// `last_run`, and flips `in_flight`; `invalidate_in_flight` bumps
/// `request_id` and clears `in_flight`. Open-coding any subset
/// re-opens the race where a stale response carrying an old
/// `request_id` overwrites a fresh branch's cache slot.
#[derive(Default)]
struct LookupState {
    request_id: u64,
    in_flight: bool,
    last_run: Option<Instant>,
}

impl LookupState {
    /// Atomic spawn-state transition: bump `request_id`, stamp
    /// `last_run`, set `in_flight=true`. The three fields move together
    /// or not at all; open-coding any subset is the symmetric-variant
    /// drift this struct exists to prevent.
    fn begin_spawn(&mut self, now: Instant) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.last_run = Some(now);
        self.in_flight = true;
        self.request_id
    }

    /// Invalidate any in-flight worker without consuming the spawn slot.
    /// Used on branch flips so a stale response carrying the old
    /// `request_id` fails the equality guard in the apply path.
    fn invalidate_in_flight(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        self.in_flight = false;
    }

    fn cooldown_active(&self, now: Instant, interval: Duration) -> bool {
        self.last_run
            .is_some_and(|last| now.duration_since(last) < interval)
    }
}

#[derive(Clone)]
struct PullRequestContextCacheEntry {
    checked_at: Instant,
    head: Option<Oid>,
    pull_request: Option<Arc<PullRequestInfo>>,
}

impl PullRequestContextCacheEntry {
    fn is_fresh(&self, head: Option<&Oid>, now: Instant) -> bool {
        self.head.as_ref() == head
            && now.duration_since(self.checked_at) < PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL
    }

    fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.checked_at) >= PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PullRequestLookupMode {
    RespectCache,
    ForceRefresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FullRedrawReason {
    FirstAttach,
    Resize,
    TabSwitch,
    LayoutChange,
    SplitClose,
    ZoomChange,
    ScrollbackMovement,
    DialogChange,
    SelectionRepaint,
    PaletteOverlay,
    FocusChange,
    SessionExit,
    PaneClear,
    ExplicitRedraw,
    StatusChange,
    PaneCacheMiss,
    UnsafePartial,
}

impl FullRedrawReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::FirstAttach => "first-attach",
            Self::Resize => "resize",
            Self::TabSwitch => "tab-switch",
            Self::LayoutChange => "layout-change",
            Self::SplitClose => "split-close",
            Self::ZoomChange => "zoom-change",
            Self::ScrollbackMovement => "scrollback-movement",
            Self::DialogChange => "dialog-change",
            Self::SelectionRepaint => "selection-repaint",
            Self::PaletteOverlay => "palette-overlay",
            Self::FocusChange => "focus-change",
            Self::SessionExit => "session-exit",
            Self::PaneClear => "pane-clear",
            Self::ExplicitRedraw => "explicit-redraw",
            Self::StatusChange => "status-change",
            Self::PaneCacheMiss => "pane-cache-miss",
            Self::UnsafePartial => "unsafe-partial",
        }
    }
}

/// Stages of the takeover/first-attach burst. Each variant maps to a
/// human-readable label used in the clog line when the send fails so a
/// dropped initial frame is observable in the multiplexer log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialFrameKind {
    Welcome,
    ClientOwnedModes,
    FocusedPaneModes,
    FirstAttach,
    SpawnFailureBanner,
}

impl InitialFrameKind {
    fn label(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::ClientOwnedModes => "client-owned mode state",
            Self::FocusedPaneModes => "focused-pane mode state",
            Self::FirstAttach => "first-attach frame",
            Self::SpawnFailureBanner => "spawn-failure banner",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointerShape {
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    Grabbing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverTarget {
    Tab(usize),
    Menu,
    BranchContext,
    Container,
    DialogCopyTarget,
}

impl PointerShape {
    fn as_osc22_name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::EwResize => "ew-resize",
            Self::NsResize => "ns-resize",
            Self::Grabbing => "grabbing",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VisiblePane {
    id: u64,
    outer: Rect,
    inner: Rect,
    focused: bool,
    body_dim: PaneBodyDim,
}

#[derive(Debug, Clone)]
struct DragState {
    tab_idx: usize,
    /// Tree path from the tab's root to the split node being resized
    /// (`0` = left/top child, `1` = right/bottom). Empty path = root
    /// split.
    path: Vec<u8>,
    orient: SplitOrient,
    /// Outer rectangle of the split — stable for the duration of the
    /// drag because spawns / closes block on dialog input and the
    /// daemon does not reflow during a drag.
    rect: Rect,
}

/// Mouse-driven text selection on a pane whose program never asked
/// for a mouse protocol (shells, post-exit agents). Modelled on
/// zellij's behaviour: drag inside the pane body paints an inverse
/// highlight; release base64-encodes the selected text and writes it
/// to the operator's clipboard via OSC 52. Cleared on any focus
/// change, tab swap, or dialog open.
const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Hard cap on simultaneous tabs. 32 is well past any operator
/// workflow but small enough that an accidental loop of new-tab
/// requests cannot drive the container OOM.
const MAX_TABS: usize = 32;

/// Hard cap on simultaneous sessions (panes). Splits within tabs
/// can grow the session count past the tab count; cap separately
/// for the same memory-bounding reason.
const MAX_SESSIONS: usize = 64;

/// `JACKIN_ESCAPE_TIME` env var — operator-tunable in milliseconds.
const ENV_ESCAPE_TIME: &str = "JACKIN_ESCAPE_TIME";

/// 50 ms matches tmux's default. Below human perception while
/// surviving slow ssh / paste chunks.
const DEFAULT_ESCAPE_TIME: std::time::Duration = std::time::Duration::from_millis(50);

/// XTerm SGR any-event mouse tracking reports passive motion as
/// button code 35 (`32` motion bit + `3` no-button code).
const SGR_NO_BUTTON_MOTION: u8 = 35;

const DIALOG_COPY_FEEDBACK_DURATION: std::time::Duration = std::time::Duration::from_secs(2);
/// One second is quick enough for operator-visible title/chrome updates after
/// `git checkout` while avoiding a 10Hz daemon wake-up just to inspect local
/// branch state.
const GIT_BRANCH_CONTEXT_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// 60 s keeps the CI-status freshness within one PR turn while
/// staying well under `gh`'s default secondary-rate-limit budget.
/// The bar is operator-facing chrome, not a live feed.
const PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL: Duration = Duration::from_secs(60);

impl Multiplexer {
    pub fn new(rows: u16, cols: u16, launch_config: CapsuleConfig) -> Self {
        let (rows, cols) = normalize_size(rows, cols);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = rows
            .saturating_sub(STATUS_BAR_ROWS)
            .saturating_sub(BRANCH_CONTEXT_BAR_ROWS);
        let agents = launch_config.supported_agents();
        let zai_key = std::env::var("ZAI_API_KEY")
            .ok()
            .filter(|value| !value.is_empty());

        let env_passthrough: Vec<(String, String)> = SESSION_ENV_PASSTHROUGH
            .iter()
            .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
            .collect();

        let input_parser = InputParser::default();
        let workdir = PathBuf::from(&launch_config.workdir);
        let workdir_context = WorkdirContext::resolve(&workdir);
        crate::clog!(
            "workdir-context: git_available={} gh_available={} is_git_repo={} default_branch={:?}",
            workdir_context.git_available,
            workdir_context.gh_available,
            workdir_context.is_git_repo,
            workdir_context.default_branch
        );
        let mut status_bar = StatusBar::new_with_role(launch_config.role.clone());
        status_bar.set_prefix_enabled(input_parser.prefix_enabled());

        Self {
            sessions: HashMap::new(),
            tabs: Vec::new(),
            active_tab: 0,
            term_rows: rows,
            term_cols: cols,
            status_bar,
            dialog_stack: Vec::new(),
            content_rows,
            available_agents: agents,
            launch_config,
            env_passthrough,
            event_tx,
            event_rx,
            zoomed: None,
            input_parser,
            detach_requested: false,
            attached_out: None,
            attached_out_dead_logged: false,
            attached_task: None,
            last_tab_click: None,
            drag: None,
            selection: None,
            pane_body_caches: HashMap::new(),
            dirty_panes: HashSet::new(),
            pending_full_redraw: None,
            pointer_shape: PointerShape::Default,
            pointer_shapes_supported: false,
            attached_terminal: ClientTerminal::default(),
            last_outer_terminal_title: None,
            hover_target: None,
            dialog_copy_feedback_deadline: None,
            pull_request_context_branch: None,
            pull_request_context_head: None,
            pull_request_context: None,
            git_branch_lookup: LookupState::default(),
            pull_request_lookup: LookupState::default(),
            pull_request_context_cache: HashMap::new(),
            workdir,
            workdir_context,
            zai_key,
            ratatui_terminal: ratatui::Terminal::new(crate::socket_backend::SocketBackend::new(
                cols, rows,
            ))
            .expect("SocketBackend::new never fails"),
        }
    }

    fn send_to_client(&mut self, frame: ServerFrame) {
        if let Some(tx) = &self.attached_out
            && tx.send(encode_server(frame)).is_err()
            && !self.attached_out_dead_logged
        {
            self.attached_out_dead_logged = true;
            crate::clog!(
                "send_to_client: client receiver dropped; frame discarded (this attach is dead)"
            );
        }
    }

    fn send_output(&mut self, bytes: Vec<u8>) {
        self.send_to_client(ServerFrame::Output(bytes));
    }

    fn env_for_spawn(&self, overrides: &[(String, String)]) -> Vec<(String, String)> {
        let mut env = self.env_passthrough.clone();
        for (key, value) in overrides {
            if !SESSION_ENV_PASSTHROUGH.iter().any(|allowed| allowed == key) {
                crate::clog!("spawn env: rejected non-allowlisted key {key:?}");
                continue;
            }
            if let Some((_, existing)) =
                env.iter_mut().find(|(existing_key, _)| existing_key == key)
            {
                *existing = value.clone();
            } else {
                env.push((key.clone(), value.clone()));
            }
        }
        env
    }

    fn palette_close_label(&self) -> PaletteCloseLabel {
        if self.active_tab_pane_count() == 1 {
            PaletteCloseLabel::CloseTab
        } else {
            PaletteCloseLabel::ChooseTarget
        }
    }

    fn open_command_palette(&mut self) {
        let close_label = self.palette_close_label();
        self.dialog_push(Dialog::new_command_palette(close_label));
    }

    fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.launch_config.model_for_agent(agent)
    }

    /// Providers selectable for `agent`. An empty vec means only the
    /// default provider is available and no picker step is needed; a
    /// non-empty vec always has 2+ entries (enforced by the catalog).
    fn providers_for_agent(&self, agent: Option<&str>) -> Vec<jackin_protocol::Provider> {
        jackin_protocol::Provider::available_for(agent.unwrap_or_default(), self.zai_key.is_some())
    }

    /// Bound the per-container surface for any path that allocates a
    /// new PTY (top-level spawn, split, etc.). All such paths must
    /// route through here so `MAX_TABS` / `MAX_SESSIONS` are enforced
    /// uniformly — runaway-mis-click defence. `add_tab=true` enforces
    /// both caps; `add_tab=false` enforces only `MAX_SESSIONS` because
    /// the caller is reusing an existing tab.
    fn ensure_capacity_for_new_session(&self, add_tab: bool) -> Result<()> {
        if add_tab && self.tabs.len() >= MAX_TABS {
            anyhow::bail!("tab limit reached ({MAX_TABS}); close one before spawning another");
        }
        if self.sessions.len() >= MAX_SESSIONS {
            anyhow::bail!(
                "pane limit reached ({MAX_SESSIONS}); close some panes before opening more"
            );
        }
        Ok(())
    }

    /// Split the focused pane and spawn a session of the operator's
    /// choice inside it. `agent_slug = None` opens a shell. Used by
    /// the AgentPicker → Split flow so the operator picks the new
    /// pane's identity instead of cloning the source pane's agent.
    fn split_focused_into(
        &mut self,
        direction: SplitDirection,
        agent_slug: Option<String>,
        env_overrides: &[(String, String)],
        provider_label: Option<&str>,
    ) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        // Any selection / drag-resize is anchored to a specific pane
        // rect that this reflow is about to invalidate.
        self.cancel_drag();
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Ok(());
        };
        let from_id = tab.focused_id;
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let from_rect = tab
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == from_id)
            .map(|(_, r)| r)
            .unwrap_or(content_rect);
        let (spawn_rows, spawn_cols) = match direction {
            SplitDirection::Left | SplitDirection::Right => (
                from_rect.rows.saturating_sub(2),
                (from_rect.cols / 2).saturating_sub(2),
            ),
            SplitDirection::Above | SplitDirection::Below => (
                (from_rect.rows / 2).saturating_sub(2),
                from_rect.cols.saturating_sub(2),
            ),
        };
        let env_passthrough = self.env_for_spawn(env_overrides);
        let launch = self.session_launch(agent_slug.as_deref(), provider_label, &env_passthrough);
        let agent_for_log = agent_slug.clone();
        let (session, new_id) = Session::spawn(
            &launch.label,
            agent_slug,
            provider_label.map(|label| crate::session::SessionProvider {
                label: label.to_string(),
                env_overrides: env_overrides.to_vec(),
            }),
            launch.cmd,
            spawn_rows,
            spawn_cols,
            self.event_tx.clone(),
        )?;
        self.sessions.insert(new_id, session);
        let tab = &mut self.tabs[self.active_tab];
        let placed = match direction {
            SplitDirection::Left => tab.tree.split_h(from_id, new_id, SplitPosition::Before),
            SplitDirection::Right => tab.tree.split_h(from_id, new_id, SplitPosition::After),
            SplitDirection::Above => tab.tree.split_v(from_id, new_id, SplitPosition::Before),
            SplitDirection::Below => tab.tree.split_v(from_id, new_id, SplitPosition::After),
        };
        if !placed {
            // from_id vanished between split intent and dispatch
            // (e.g. the source pane exited mid-action). Undo the
            // session insert so the spawned PTY + child + tasks do
            // not leak as an orphan that no tab tree references.
            if let Some(orphan) = self.sessions.remove(&new_id) {
                orphan.terminate();
            }
            crate::clog!(
                "action: split aborted — from_id={from_id} no longer in tab tree; reaped orphan id={new_id}",
            );
            return Ok(());
        }
        tab.focused_id = new_id;
        self.resize_panes();
        self.synthesise_focus_swap(Some(from_id), Some(new_id));
        crate::clog!(
            "action: split id={new_id} from={from_id} dir={direction:?} agent={agent_for_log:?} label={label}",
            label = launch.label,
        );
        Ok(())
    }

    /// Split the focused pane and clone the source pane's agent into
    /// the new pane. Used by the `Ctrl+B %` / `Ctrl+B "` prefix
    /// bindings so split-and-spawn skips the agent picker and inherits
    /// the source pane's runtime.
    fn split_focused(&mut self, direction: SplitDirection) -> Result<()> {
        self.ensure_capacity_for_new_session(false)?;
        let (agent_slug, provider_env_overrides, provider_label) = self.focused_spawn_metadata();
        self.split_focused_into(
            direction,
            agent_slug,
            &provider_env_overrides,
            provider_label.as_deref(),
        )
    }

    fn focused_spawn_metadata(&self) -> (Option<String>, Vec<(String, String)>, Option<String>) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return (None, Vec::new(), None);
        };
        let from_id = tab.focused_id;
        self.sessions
            .get(&from_id)
            .map_or((None, Vec::new(), None), |session| {
                let (env, label) = session.provider.as_ref().map_or_else(
                    || (Vec::new(), None),
                    |provider| (provider.env_overrides.clone(), Some(provider.label.clone())),
                );
                (session.agent.clone(), env, label)
            })
    }

    fn close_focused_pane(&mut self) {
        self.cancel_drag();
        let prev_focused = self.active_focused_id();
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let id = tab.focused_id;
        let all = tab.tree.all_ids();
        let next_focus = all.iter().find(|&&sid| sid != id).copied();
        crate::clog!(
            "action: close_focused_pane id={id} tab_idx={} siblings_remaining={}",
            self.active_tab,
            next_focus.is_some()
        );
        tab.tree.remove(id);
        if let Some(session) = self.sessions.remove(&id) {
            session.terminate();
        }
        self.pane_body_caches.remove(&id);
        // Drop the zoomed reference when the killed pane was the zoom
        // target so the next `compose_frame` does not paint a stale
        // zoom area until the operator manually unzooms.
        self.zoomed = self.zoomed.filter(|&zid| zid != id);
        if let Some(nf) = next_focus {
            tab.focused_id = nf;
        } else {
            self.tabs.remove(self.active_tab);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
        }
        self.resize_panes();
        self.synthesise_focus_swap(prev_focused, self.active_focused_id());
    }

    fn resize_panes(&mut self) {
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            let inner = content_rect.shrink(1);
            if let Some(session) = self.sessions.get_mut(&zoom_id) {
                session.resize(inner.rows, inner.cols);
            }
            return;
        }
        for tab in &self.tabs {
            let leaves = tab.tree.leaves(content_rect);
            for (id, rect) in leaves {
                let inner = rect.shrink(1);
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.resize(inner.rows, inner.cols);
                }
            }
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = normalize_size(rows, cols);
        // Outer-terminal resize invalidates the drag's saved rect.
        self.cancel_drag();
        self.term_rows = rows;
        self.term_cols = cols;
        self.content_rows = self.available_content_rows();
        self.resize_panes();
        self.ratatui_terminal.backend_mut().resize(cols, rows);
    }

    fn available_content_rows(&self) -> u16 {
        self.term_rows
            .saturating_sub(STATUS_BAR_ROWS)
            .saturating_sub(BRANCH_CONTEXT_BAR_ROWS)
    }

    fn reconcile_content_rows(&mut self) -> bool {
        let next = self.available_content_rows();
        if next == self.content_rows {
            return false;
        }
        self.content_rows = next;
        self.pane_body_caches.clear();
        self.resize_panes();
        true
    }

    fn active_focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|t| t.focused_id)
    }

    /// `self.zoomed` narrowed to "only when the zoomed session belongs
    /// to the active tab." The zoom field is global (one value across
    /// all tabs), but render / input / scroll / mouse paths must
    /// behave as if zoom is per-tab — switching tabs has to surface
    /// the new tab's panes normally even when a different tab still
    /// has a zoomed session pinned, otherwise opening a new tab paints
    /// the previously-zoomed pane full-screen. Returning `None` from
    /// the active-tab check routes every consumer of zoom state
    /// through the normal multi-pane path for tabs that don't hold
    /// the zoom.
    fn active_zoomed_id(&self) -> Option<u64> {
        let zoom_id = self.zoomed?;
        let tab = self.tabs.get(self.active_tab)?;
        if tab.tree.all_ids().contains(&zoom_id) {
            Some(zoom_id)
        } else {
            None
        }
    }

    fn active_focused_outer_rect(&self) -> Option<Rect> {
        let focused = self.active_focused_id()?;
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.active_zoomed_id() {
            return (zoom_id == focused).then_some(content_rect);
        }
        self.tabs
            .get(self.active_tab)?
            .tree
            .leaves(content_rect)
            .into_iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, rect)| rect)
    }

    fn active_focused_inner_rect(&self) -> Option<Rect> {
        self.active_focused_outer_rect().map(|rect| rect.shrink(1))
    }

    /// Derive the label that should appear in the tab strip for `tab`
    /// given the current pane contents. Operator's mental model is
    /// "what kinds of things am I running in here?", so the label
    /// tracks the kind makeup instead of pinning to the first
    /// session spawned: a single-agent tab carries that agent's
    /// name; shells-only is `Shell`; two distinct agents is
    /// `Agents`; any agent + any shell is `Mix`.
    fn tab_display_label(&self, tab: &Tab) -> String {
        let ids = tab.tree.all_ids();
        let pane_count = ids.len();
        let mut agent_labels: Vec<String> = Vec::new();
        let mut has_shell = false;
        for id in ids {
            if let Some(s) = self.sessions.get(&id) {
                match &s.agent {
                    Some(_) => {
                        let label = session_agent_label(s);
                        if !agent_labels.iter().any(|existing| existing == &label) {
                            agent_labels.push(label);
                        }
                    }
                    None => has_shell = true,
                }
            }
        }
        let base = match (agent_labels.len(), has_shell) {
            (0, _) => "Shell".to_string(),
            (1, false) => agent_labels[0].clone(),
            (_, false) => "Agents".to_string(),
            (_, true) => "Mix".to_string(),
        };
        if pane_count > 1 {
            format!("{base} ({pane_count})")
        } else {
            base
        }
    }

    /// Rewrite each tab's auto-label after a spawn / split / remove.
    /// `Tab::label()` reads `custom_label` first, so operator-typed
    /// names survive this refresh automatically. Cheap (clones a few
    /// short strings) and easier to reason about than dispatching
    /// incremental updates from every mutation site.
    fn refresh_tab_labels(&mut self) {
        let mut new_labels = Vec::with_capacity(self.tabs.len());
        for tab in &self.tabs {
            new_labels.push(self.tab_display_label(tab));
        }
        for (tab, label) in self.tabs.iter_mut().zip(new_labels) {
            tab.set_auto_label(label);
        }
    }

    /// True when there are no sessions left.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    fn no_live_sessions(&self) -> bool {
        self.sessions.is_empty()
    }

    fn request_pane_body_redraw(&mut self, session_id: u64) {
        if self.pending_full_redraw.is_none() {
            self.dirty_panes.insert(session_id);
        }
    }

    fn request_full_redraw(&mut self, reason: FullRedrawReason) {
        self.pending_full_redraw = Some(reason);
        self.dirty_panes.clear();
    }

    fn has_pending_render(&self) -> bool {
        self.pending_full_redraw.is_some() || !self.dirty_panes.is_empty()
    }

    fn visible_panes(&self) -> Vec<VisiblePane> {
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let focused_id = self.active_focused_id();
        if let Some(zoom_id) = self.active_zoomed_id() {
            let outer = content_rect;
            return vec![VisiblePane {
                id: zoom_id,
                outer,
                inner: outer.shrink(1),
                focused: Some(zoom_id) == focused_id,
                body_dim: PaneBodyDim::Normal,
            }];
        }
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Vec::new();
        };
        let leaves = tab.tree.leaves(content_rect);
        let multi_pane = leaves.len() > 1;
        leaves
            .into_iter()
            .map(|(id, outer)| {
                let focused = Some(id) == focused_id;
                VisiblePane {
                    id,
                    outer,
                    inner: outer.shrink(1),
                    focused,
                    body_dim: if multi_pane && !focused {
                        PaneBodyDim::Inactive
                    } else {
                        PaneBodyDim::Normal
                    },
                }
            })
            .collect()
    }

    /// Adjust the split that contains the focused pane along `dir` by
    /// 5% of the parent rectangle. Triggered by `Alt+Shift+Arrow`.
    fn resize_focused(&mut self, dir: ArrowDir) {
        let Some(tab_idx) = self.tabs.get(self.active_tab).map(|_| self.active_tab) else {
            return;
        };
        let focused = self.tabs[tab_idx].focused_id;
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        if self.tabs[tab_idx].tree.resize(focused, d, 0.05) {
            self.resize_panes();
        }
    }

    fn move_focus(&mut self, dir: ArrowDir) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        let prev = tab.focused_id;
        if let Some(next_id) = tab.tree.adjacent(content_rect, tab.focused_id, d) {
            self.tabs[self.active_tab].focused_id = next_id;
            self.synthesise_focus_swap(Some(prev), Some(next_id));
        }
    }

    /// Synthesise `\x1b[O` / `\x1b[I` to track which pane the operator
    /// is actually looking at. Agents that watch focus events use them
    /// to pause polling / animations; without synthesis, a backgrounded
    /// pane thinks it is still focused.
    ///
    /// Also re-emits the newly focused session's mode state
    /// (bracketed paste, etc.) so the outer terminal matches what
    /// the now-visible agent wants. Each agent owns its own mode
    /// state and switching tabs must not leak the previous agent's
    /// setup to the new one.
    fn synthesise_focus_swap(&mut self, old: Option<u64>, new: Option<u64>) {
        if old == new {
            return;
        }
        // Synthetic `\x1b[I` / `\x1b[O` to the agent's PTY only
        // when the agent enabled focus-event reporting (DEC ?1004).
        // Shells and pre-mount agents leave it off; writing the
        // bytes into their PTY would surface as literal `[I` /
        // `[O` text at the prompt.
        if let Some(o) = old
            && let Some(s) = self.sessions.get(&o)
            && s.focus_events_enabled()
        {
            s.send_input(b"\x1b[O");
        }
        if let Some(n) = new
            && let Some(s) = self.sessions.get(&n)
        {
            if s.focus_events_enabled() {
                s.send_input(b"\x1b[I");
            }
            // Reset the outer terminal to a known baseline, then
            // re-emit every mode the new pane wants live.
            // Mouse/focus are reasserted as client-owned modes so a
            // pane cannot downgrade the multiplexer's input channel
            // to legacy X10 after a focus-changing close/split.
            if self.attached_out.is_some() {
                let mut frames: Vec<Vec<u8>> = Vec::new();
                frames.push(crate::session::Session::focus_swap_reset().to_vec());
                frames.push(crate::session::Session::client_owned_mode_state().to_vec());
                for bytes in s.current_mode_state() {
                    frames.push(bytes);
                }
                for bytes in frames {
                    self.send_output(bytes);
                }
            }
        }
    }

    /// Zoomed tabs never produce a drag — there are no shared
    /// borders to grab on a single visible pane.
    fn detect_drag_start(&self, row: u16, col: u16) -> Option<DragState> {
        if row < STATUS_BAR_ROWS || self.active_zoomed_id().is_some() {
            return None;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let tab = self.tabs.get(self.active_tab)?;
        let (path, orient, rect) = tab.tree.border_at(content_rect, row, col)?;
        Some(DragState {
            tab_idx: self.active_tab,
            path,
            orient,
            rect,
        })
    }

    /// Apply a drag motion at `(row, col)` against the active drag's
    /// split. Recomputes the ratio from the mouse position relative
    /// to the saved split rectangle, clamps to `[0.05, 0.95]`, then
    /// reflows the panes so the agent PTYs resize in step.
    fn drag_motion(&mut self, row: u16, col: u16) -> Option<Vec<u8>> {
        let drag = self.drag.clone()?;
        let new_ratio = match drag.orient {
            SplitOrient::Horizontal => {
                let off = col.saturating_sub(drag.rect.col);
                (off as f32 / drag.rect.cols as f32).clamp(0.05, 0.95)
            }
            SplitOrient::Vertical => {
                let off = row.saturating_sub(drag.rect.row);
                (off as f32 / drag.rect.rows as f32).clamp(0.05, 0.95)
            }
        };
        let tab = self.tabs.get_mut(drag.tab_idx)?;
        if !tab.tree.set_ratio_at(&drag.path, new_ratio) {
            return None;
        }
        self.resize_panes();
        Some(self.compose_full_frame(FullRedrawReason::LayoutChange))
    }

    /// Switch focus to the pane the operator clicked on, if it differs
    /// from the current focus. Returns `true` when the focus actually
    /// changed so the caller can trigger a redraw.
    ///
    /// Honours the zoomed-pane state: when a pane is zoomed it fills
    /// the entire content rect, so clicks inside that rect must
    /// resolve to the zoomed pane even if a sibling pane's unzoomed
    /// rect happens to cover the click point. Walking `tab.tree.leaves`
    /// without this guard sends focus (and subsequent keystrokes) to
    /// a hidden pane while the zoomed pane stays painted as focused.
    fn focus_pane_at(&mut self, row: u16, col: u16) -> bool {
        if row < STATUS_BAR_ROWS {
            return false;
        }
        let content_rect = Rect::new(STATUS_BAR_ROWS, 0, self.content_rows, self.term_cols);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let prev = tab.focused_id;
        if let Some(zoom_id) = self.active_zoomed_id() {
            // Click outside the content rect (header chrome, status
            // bar) cannot affect zoom focus; otherwise the only
            // candidate is the zoomed pane itself.
            if row < content_rect.row + content_rect.rows
                && col < content_rect.col + content_rect.cols
                && zoom_id != prev
            {
                self.tabs[self.active_tab].focused_id = zoom_id;
                self.synthesise_focus_swap(Some(prev), Some(zoom_id));
                return true;
            }
            return false;
        }
        let leaves = tab.tree.leaves(content_rect);
        for (id, rect) in leaves {
            if row >= rect.row
                && row < rect.row + rect.rows
                && col >= rect.col
                && col < rect.col + rect.cols
                && id != prev
            {
                self.tabs[self.active_tab].focused_id = id;
                self.synthesise_focus_swap(Some(prev), Some(id));
                return true;
            }
        }
        false
    }

    fn clear_focused_pane(&mut self) {
        self.cancel_drag();
        if let Some(id) = self.active_focused_id()
            && let Some(session) = self.sessions.get_mut(&id)
        {
            session.clear_scrollback_and_request_screen_clear();
            self.pane_body_caches.remove(&id);
            self.dirty_panes.remove(&id);
        }
    }

    fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.sessions
            .iter()
            .map(|(&id, s)| SessionInfo {
                id,
                label: s.label.clone(),
                agent: s.agent.clone(),
                state: s.state,
                active: Some(id) == focused,
            })
            .collect()
    }

    /// Switch the active tab to whichever tab contains the leaf
    /// carrying `session_id`, and set that tab's `focused_id` to
    /// `session_id`. Returns `true` when the search succeeded;
    /// `false` when no tab references the id, leaving state
    /// untouched.
    fn focus_session_globally(&mut self, session_id: u64) -> bool {
        use crate::layout::Rect;
        let probe_rect = Rect::new(0, 0, self.term_rows, self.term_cols);
        let prev_focused = self.active_focused_id();
        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            let leaf_ids: Vec<u64> = tab
                .tree
                .leaves(probe_rect)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            if leaf_ids.contains(&session_id) {
                self.active_tab = tab_idx;
                self.tabs[tab_idx].focused_id = session_id;
                self.synthesise_focus_swap(prev_focused, Some(session_id));
                return true;
            }
        }
        false
    }

    /// Build a tab/pane tree snapshot for the host console's preview
    /// pane. The leaf order matches `PaneTree::leaves` so the operator
    /// sees panes in the same left-to-right / top-to-bottom order the
    /// multiplexer renders. Missing sessions (race against a kill)
    /// fall back to a placeholder so the snapshot still covers every
    /// leaf the tree references — the host UI can dim those rows.
    fn tab_snapshots(&self) -> Vec<crate::protocol::control::TabSnapshot> {
        use crate::layout::Rect;
        use crate::protocol::control::{PaneSnapshot, TabSnapshot};
        let placeholder_rect = Rect::new(0, 0, self.term_rows, self.term_cols);
        self.tabs
            .iter()
            .map(|tab| {
                let panes = tab
                    .tree
                    .leaves(placeholder_rect)
                    .into_iter()
                    .map(|(id, _)| match self.sessions.get(&id) {
                        Some(session) => PaneSnapshot {
                            session_id: id,
                            label: session.label.clone(),
                            agent: session.agent.clone(),
                            state: session.state,
                        },
                        None => PaneSnapshot {
                            session_id: id,
                            label: "(missing)".to_string(),
                            agent: None,
                            state: crate::protocol::control::AgentState::Idle,
                        },
                    })
                    .collect();
                TabSnapshot {
                    label: tab.label_owned(),
                    focused_pane: tab.focused_id,
                    panes,
                }
            })
            .collect()
    }
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
pub async fn run_daemon(initial_agent: String, launch_config: CapsuleConfig) -> Result<()> {
    crate::pid1::install_sigchld_reaper();

    let rows = std::env::var("JACKIN_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ROWS);
    let cols = std::env::var("JACKIN_COLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_COLS);
    let (rows, cols) = normalize_size(rows, cols);

    // Initialise the file logger before anything else can emit a
    // diagnostic. Failures fall back to stderr-only, so this is safe
    // to call unconditionally.
    crate::logging::init();
    crate::clog!(
        "daemon start: rows={rows} cols={cols} initial_agent={initial_agent:?} workdir={}",
        launch_config.workdir.as_str()
    );

    let initial_spawn =
        initial_spawn_request(&initial_agent, launch_config.initial_provider.as_ref());
    let mut mux = Multiplexer::new(rows, cols, launch_config);
    start_git_context_watcher(mux.workdir.clone(), mux.event_tx.clone());
    // Defer the first pane until the first attach Hello has supplied
    // real outer-terminal dimensions. Later panes already spawn after
    // attach-time resize; routing the first pane through the same
    // path removes first-tab-only scrollback/chrome differences.
    let mut pending_initial_spawn = Some(initial_spawn);

    let mut new_clients = socket::start_listener()?;
    let mut branch_context_ticker = interval(GIT_BRANCH_CONTEXT_POLL_INTERVAL);
    let mut state_ticker = interval(Duration::from_secs(1));
    // Render ticker: ~30 fps. Coalesces PTY-output bursts into one
    // frame per tick. With 4+ panes producing output continuously,
    // composing immediately on every event spent more time emitting
    // SGR bytes than the client could draw, which read as visible
    // multiplexer lag. zellij uses the same coalescing pattern.
    let mut render_ticker = interval(Duration::from_millis(33));
    render_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    // Inbound: attach handler tasks → main loop.
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientFrame>();
    // Inbound: spawned handshake tasks → main loop. The spawned task
    // owns the slow `read_exact` for the first byte + Hello frame so
    // a silent client cannot stall the main `select!`. Validated
    // handshakes ride this channel back to the main loop, which then
    // applies the take-over + spawns the persistent attach task.
    let (handshake_tx, mut handshake_rx) = mpsc::unbounded_channel::<AttachHandshake>();

    // Resolve the operator's escape-time once at startup; the value
    // cannot change after daemon launch, so per-iteration env reads
    // would be wasted syscalls. A present-but-unparseable env var
    // emits a debug line so the operator sees their config rejected
    // rather than silently falling back to the default.
    let escape_time = match std::env::var(ENV_ESCAPE_TIME) {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(ms) => Duration::from_millis(ms),
            Err(_) => {
                crate::clog!(
                    "{ENV_ESCAPE_TIME}={raw:?} ignored (not a positive integer); using default {} ms",
                    DEFAULT_ESCAPE_TIME.as_millis()
                );
                DEFAULT_ESCAPE_TIME
            }
        },
        Err(_) => DEFAULT_ESCAPE_TIME,
    };

    // Persistent escape-time deadline. Set when the parser first
    // enters `EscStart` (one Esc with no follow-up yet). Cleared once
    // the parser leaves `EscStart` (because either the rest of a CSI
    // sequence arrived or `flush_pending_esc` ran).
    //
    // Recomputing this each iteration as `now() + escape_time` is
    // wrong: a chatty PTY (a TUI agent with a spinner) wakes the
    // select loop dozens of times per second, and a fresh deadline
    // each wake-up never lapses before the next PTY output resets it.
    let mut esc_deadline: Option<tokio::time::Instant> = None;
    loop {
        if mux.input_parser.esc_pending() {
            if esc_deadline.is_none() {
                esc_deadline = Some(tokio::time::Instant::now() + escape_time);
            }
        } else {
            esc_deadline = None;
        }
        tokio::select! {
            biased;

            _ = sigterm.recv() => {
                detach_client(&mut mux).await;
                return Ok(());
            }
            _ = sigint.recv() => {
                detach_client(&mut mux).await;
                return Ok(());
            }

            // New socket connection — spawn the handshake off the
            // main loop so a client that connects but never sends the
            // first byte does not stall PTY processing, ticks, or
            // signal handling. The spawned task either handles the
            // control channel inline (one-shot reply, closes the
            // socket) or forwards a validated attach Hello back via
            // `handshake_tx`.
            Some((stream, client_permit)) = new_clients.recv() => {
                let handshake_tx = handshake_tx.clone();
                let sessions_snapshot = mux.session_infos();
                let tabs_snapshot = mux.tab_snapshots();
                let active_tab = u32::try_from(mux.active_tab).unwrap_or(0);
                tokio::spawn(perform_handshake(
                    stream,
                    client_permit,
                    handshake_tx,
                    sessions_snapshot,
                    tabs_snapshot,
                    active_tab,
                ));
            }

            // Validated attach handshake from the spawned handshake task.
            Some(ready) = handshake_rx.recv() => {
                let AttachHandshake {
                    stream,
                    rows,
                    cols,
                    spawn,
                    env,
                    terminal,
                    focus_session,
                    client_permit,
                } = ready;
                mux.resize(rows, cols);
                mux.pointer_shapes_supported = terminal.pointer_shapes_supported();
                mux.attached_terminal = terminal;
                mux.pointer_shape = PointerShape::Default;
                if mux.sessions.is_empty()
                    && let Some(request) = pending_initial_spawn.take()
                    && let Err(err) = mux.spawn_request(request.clone(), &[])
                {
                    crate::clog!(
                        "initial spawn failed (request={}): {err:#}",
                        spawn_request_label(&request)
                    );
                    return Err(err);
                }
                if let Some(target) = focus_session
                    && !mux.focus_session_globally(target)
                {
                    crate::clog!(
                        "attach: ignoring unknown focus_session={target} (no matching pane)"
                    );
                }
                // Honor a spawn intent from `jackin-capsule new
                // <agent>` / `jackin-capsule new` (shell). Spawn
                // failures get clog'd and surfaced to the new client
                // as an Output frame after Welcome so the operator
                // sees the reason in their terminal — silently
                // landing on an empty multiplexer would otherwise be
                // indistinguishable from "no spawn requested".
                let mut spawn_failure: Option<String> = None;
                if let Some(request) = spawn {
                    let label = spawn_request_label(&request);
                    if let Err(err) = mux.spawn_request(request, &env) {
                        crate::clog!("attach: spawn {label} failed: {err:#}");
                        spawn_failure = Some(format!("spawn {label} failed: {err:#}"));
                    }
                }
                // Take over from any existing attach client. The shared
                // helper sends Shutdown, gives the writer side a brief
                // drain window, then aborts the old reader task.
                detach_attached_task(&mut mux, "takeover").await;
                // Drain any stale frames the old client task pushed
                // into cmd_tx before its abort actually took effect —
                // without this drain, the next `cmd_rx.recv()` after
                // the new attach is wired processes Input / Resize /
                // Detach against the NEW mux state. The abort + drain
                // pair must stay single-threaded in this order: by the
                // time `try_recv` runs the old task can no longer be
                // scheduled, so the loop bound is exactly "everything
                // the old task already enqueued." On a first-attach
                // (no prior task) cmd_rx is already empty.
                let mut drained = 0u32;
                while cmd_rx.try_recv().is_ok() {
                    drained = drained.saturating_add(1);
                }
                if drained > 0 {
                    crate::clog!("takeover: drained {drained} stale frame(s) from prior client");
                }
                let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                mux.attached_out = Some(new_out_tx.clone());
                mux.attached_out_dead_logged = false;
                // Build the initial-attach burst as a typed list so a
                // typo at one call site cannot disagree with the clog
                // label. A send failure here means the receiver was
                // closed by a takeover/cancellation race in the same
                // tick; log the first failure so a wedged first-frame
                // queue is observable in the multiplexer log instead
                // of silently leaving the operator's terminal blank.
                let mut initial_frames: Vec<(InitialFrameKind, Vec<u8>)> = Vec::with_capacity(5);
                initial_frames.push((
                    InitialFrameKind::Welcome,
                    encode_server(ServerFrame::Welcome {
                        session_count: mux.sessions.len() as u32,
                    }),
                ));
                // Re-assert the attach-client-owned mouse/focus modes,
                // then restore the focused session's modes (bracketed
                // paste, etc.). Without this, a re-attach loses
                // bracketed-paste and the operator's clipboard arrives
                // unwrapped.
                initial_frames.push((
                    InitialFrameKind::ClientOwnedModes,
                    encode_server(ServerFrame::Output(
                        Session::client_owned_mode_state().to_vec(),
                    )),
                ));
                if let Some(focused) = mux.active_focused_id()
                    && let Some(session) = mux.sessions.get(&focused)
                {
                    for bytes in session.current_mode_state() {
                        initial_frames.push((
                            InitialFrameKind::FocusedPaneModes,
                            encode_server(ServerFrame::Output(bytes)),
                        ));
                    }
                }
                let mut initial = b"\x1b[2J".to_vec();
                initial.extend(mux.compose_full_frame(FullRedrawReason::FirstAttach));
                initial_frames.push((
                    InitialFrameKind::FirstAttach,
                    encode_server(ServerFrame::Output(initial)),
                ));
                if let Some(reason) = spawn_failure {
                    initial_frames.push((
                        InitialFrameKind::SpawnFailureBanner,
                        encode_server(ServerFrame::Output(spawn_failure_banner(&reason))),
                    ));
                }
                let first_failure = initial_frames
                    .into_iter()
                    .find_map(|(kind, bytes)| new_out_tx.send(bytes).err().map(|_| kind));
                if let Some(kind) = first_failure {
                    crate::clog!(
                        "attach: receiver closed before initial frame ({}); operator's terminal will not paint",
                        kind.label()
                    );
                    mux.attached_out_dead_logged = true;
                }
                let cmd_tx_for_task = cmd_tx.clone();
                mux.attached_task = Some(tokio::spawn(async move {
                    handle_attach_client(stream, new_out_rx, cmd_tx_for_task).await;
                    // Hold the concurrency permit alive for the
                    // lifetime of the attach task. Dropping at the
                    // end of the spawned future returns a slot to
                    // the listener's Semaphore.
                    drop(client_permit);
                }));
            }

            // Inbound attach frame from the active client task.
            Some(frame) = cmd_rx.recv() => {
                handle_client_frame(&mut mux, frame).await;
                if mux.detach_requested {
                    mux.detach_requested = false;
                    detach_client(&mut mux).await;
                }
                if mux.no_live_sessions() {
                    drain_and_exit(&mut mux).await;
                    return Ok(());
                }
            }

            // PTY output or exit event from a session.
            Some(event) = mux.event_rx.recv() => {
                match event {
                    SessionEvent::Output { session_id, data } => {
                        let focused_id = mux.active_focused_id();
                        let is_focused = Some(session_id) == focused_id;
                        // Collect any focused-pane output into local
                        // vecs so the `&mut Session` borrow ends before
                        // `mux.send_output` (which takes `&mut Multiplexer`).
                        let mut to_emit: Vec<Vec<u8>> = Vec::new();
                        let mut reassert_outer_terminal_title = false;
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.feed_pty(&data);
                            // Always drain the OSC + unhandled-CSI
                            // passthrough buffer so a backgrounded
                            // agent emitting OSC 7 / OSC 9 / OSC 8 on
                            // every prompt does not grow `pending`
                            // unboundedly until it becomes focused.
                            // Forward the drained bytes ONLY when this
                            // session is the focused pane —
                            // backgrounded panes' notifications,
                            // clipboard writes, and titles must not
                            // reach the operator's outer terminal.
                            let drained = session.drain_passthrough();
                            // Mode-state transitions (bracketed paste,
                            // etc.) round-trip through the outer
                            // terminal. Drain regardless of focus for
                            // the same reason; on focus swap,
                            // `current_mode_state()` restores the
                            // destination pane's full mode set in one
                            // shot, so intermediate transitions of
                            // backgrounded panes do not need to leak
                            // out (and would be silently dropped here
                            // anyway).
                            let mode_transitions = session.drain_mode_transitions();
                            if is_focused {
                                reassert_outer_terminal_title = !drained.is_empty();
                                to_emit.extend(drained);
                                to_emit.extend(mode_transitions);
                            }
                        }
                        for bytes in to_emit {
                            mux.send_output(bytes);
                        }
                        if reassert_outer_terminal_title {
                            mux.last_outer_terminal_title = None;
                        }
                        // Mark the pane body dirty; the render ticker coalesces
                        // bursts of PTY output into one frame per
                        // tick. Dialog-open still invalidates — the
                        // render ticker now paints the dialog overlay
                        // against the latest pane state, so dismiss
                        // doesn't produce a sudden burst of
                        // accumulated frames.
                        mux.request_pane_body_redraw(session_id);
                    }
                    SessionEvent::Exited { session_id } => {
                        // Remove the pane / tab immediately rather than
                        // leaving a stale `○ Done` placeholder behind.
                        // Matches the operator's mental model: "agent
                        // exited → its tab is gone."
                        mux.remove_exited_session(session_id);
                        mux.request_full_redraw(FullRedrawReason::SessionExit);
                        // When the last live session exits — whether
                        // the operator typed `/exit` in the agent or
                        // the agent crashed — there is nothing left to
                        // attach to. Tear down the container so the
                        // host cleanup path fires.
                        if mux.no_live_sessions() {
                            drain_and_exit(&mut mux).await;
                            return Ok(());
                        }
                    }
                    SessionEvent::GitBranchContextRefreshRequested => {
                        mux.force_spawn_git_branch_context_lookup(Instant::now());
                    }
                    SessionEvent::GitBranchContextLoaded {
                        request_id,
                        context,
                    } => {
                        if mux.apply_git_branch_context_loaded(
                            request_id,
                            context,
                            Instant::now(),
                        ) {
                            mux.request_full_redraw(FullRedrawReason::StatusChange);
                        }
                    }
                    SessionEvent::PullRequestContextLoaded {
                        request_id,
                        branch,
                        head,
                        outcome,
                    } => {
                        if mux.apply_pull_request_context_loaded(
                            request_id,
                            branch,
                            head,
                            outcome,
                            Instant::now(),
                        ) {
                            mux.request_full_redraw(FullRedrawReason::StatusChange);
                        }
                    }
                }
            }

            // Escape-time fired: the operator's `\x1b` did not get a
            // follow-up byte in time, so emit it as a bare Data event.
            // Dialogs treat it as dismiss; agents see the lone Esc.
            _ = async {
                match esc_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            }, if esc_deadline.is_some() => {
                esc_deadline = None;
                let events = mux.input_parser.flush_pending_esc();
                for event in events {
                    if let Some(redraw) = mux.handle_input(event) {
                        mux.send_output(redraw);
                    }
                }
            }

            // Render ticker: drain dirty pane bodies or a named full-frame
            // invalidation at ~30 fps. One
            // frame per tick at most, regardless of how many PTY
            // events arrived since the last tick. Full-frame fallback
            // includes the dialog overlay when one is open, so the
            // open-dialog case still composes (and the operator sees
            // dialog content over the latest pane state) instead of
            // accumulating dirty until dismiss — without this the
            // dismiss frame was a sudden jump of N frames' worth of
            // accumulated PTY output that the operator had no way to
            // see coming.
            _ = render_ticker.tick(), if mux.has_pending_render() => {
                let frame_data = mux.compose_pending_frame();
                if !frame_data.is_empty() {
                    mux.send_output(frame_data);
                }
            }

            // Branch changes are directly operator-triggered (`git checkout`)
            // and should surface in chrome immediately. Keep this separate
            // from the heavier 1s state ticker so session state refreshes and
            // GitHub lookups do not need the same fast cadence.
            _ = branch_context_ticker.tick() => {
                mux.maybe_spawn_git_branch_context_lookup(Instant::now());
            }

            // Periodic state refresh: re-render the status bar so the tab
            // strip's state glyph follows the four-state model. The full
            // pane bodies stay where they are.
            //
            // The buffer is wrapped in `DECSC` (`\x1b7`) / `DECRC`
            // (`\x1b8`) so the terminal saves the active pane's
            // cursor position before painting the status bar and
            // restores it afterwards. Without this guard the cursor
            // visibly jumps to the tab strip every tick and parks
            // there as a phantom block until the next pane redraw.
            _ = state_ticker.tick() => {
                mux.maybe_spawn_pull_request_context_lookup(Instant::now());
                for session in mux.sessions.values_mut() {
                    session.refresh_state();
                }
                if mux.expire_dialog_copy_feedback(Instant::now()) {
                    let frame_data =
                        mux.compose_dialog_overlay_frame(FullRedrawReason::DialogChange);
                    mux.send_output(frame_data);
                    continue;
                }
                // A modal owns the whole screen behind an opaque backdrop;
                // repainting the status/branch chrome here would draw it
                // back over the fill. The hidden tab-state glyph has nothing
                // to refresh, so skip the chrome frame while a dialog is open.
                if mux.dialog_open() {
                    continue;
                }
                mux.refresh_tab_labels();
                let sbuf = mux.compose_chrome_hover_frame();
                mux.send_output(sbuf);
            }
        }
    }
}

async fn handle_client_frame(mux: &mut Multiplexer, frame: ClientFrame) {
    match frame {
        ClientFrame::Hello { .. } => {
            // The initial Hello is consumed by the accept handler; any
            // further Hello on the same connection is ignored.
        }
        ClientFrame::Resize { rows, cols } => {
            mux.resize(rows, cols);
            let frame_data = mux.compose_full_frame(FullRedrawReason::Resize);
            mux.send_output(frame_data);
        }
        ClientFrame::Input(bytes) => {
            // Debug-only input-path telemetry: every chunk from the
            // client and every parser event lands in the log when
            // `JACKIN_DEBUG=1`. Production runs stay quiet — the macro
            // skips the format + write entirely. The pair is the
            // canonical trace for "key X did nothing" triage: chunk
            // line proves the byte reached the daemon, event line
            // proves the parser classified it.
            crate::cdebug!(
                "rx ClientFrame::Input len={} bytes={:02x?}",
                bytes.len(),
                bytes
            );
            let events = mux.input_parser.parse(&bytes);
            for event in events {
                let mode = mux.mux_mode();
                crate::cdebug!("  → InputEvent::{:?} mode={mode:?}", event,);
                if let Some(redraw) = mux.handle_input(event) {
                    mux.send_output(redraw);
                }
            }
            let prefix_mode = if matches!(mux.mux_mode(), MuxMode::PrefixAwait) {
                crate::statusbar::PrefixMode::Awaiting
            } else {
                crate::statusbar::PrefixMode::Idle
            };
            if mux.status_bar.prefix_mode != prefix_mode {
                mux.status_bar.set_prefix_mode(prefix_mode);
                let frame_data = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
                mux.send_output(frame_data);
            }
        }
        ClientFrame::Command(_payload) => {
            // Reserved for future structured commands from the host CLI.
        }
        ClientFrame::Detach => {
            mux.detach_requested = true;
        }
        ClientFrame::FocusIn => {
            // Forward only when no dialog is intercepting input AND
            // the focused session actually asked for focus reports
            // (`?1004h`). Without the gate, normal-screen shells
            // surface `[I` as literal text at the prompt.
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[O");
            }
        }
    }
}

const fn hovered_tab(target: Option<HoverTarget>) -> Option<usize> {
    match target {
        Some(HoverTarget::Tab(idx)) => Some(idx),
        _ => None,
    }
}

const fn hovered_menu(target: Option<HoverTarget>) -> bool {
    matches!(target, Some(HoverTarget::Menu))
}

fn prefix_full_redraw_reason(cmd: &PrefixCommand) -> FullRedrawReason {
    match cmd {
        PrefixCommand::NewTab | PrefixCommand::Palette => FullRedrawReason::PaletteOverlay,
        PrefixCommand::NextTab | PrefixCommand::PrevTab | PrefixCommand::JumpTab(_) => {
            FullRedrawReason::TabSwitch
        }
        PrefixCommand::SplitTopBottom | PrefixCommand::SplitSideBySide => {
            FullRedrawReason::LayoutChange
        }
        PrefixCommand::MoveFocus(_) => FullRedrawReason::FocusChange,
        PrefixCommand::ZoomToggle => FullRedrawReason::ZoomChange,
        PrefixCommand::KillPane | PrefixCommand::KillTab => FullRedrawReason::SplitClose,
        PrefixCommand::ClearPane => FullRedrawReason::PaneClear,
        PrefixCommand::Detach | PrefixCommand::Redraw => FullRedrawReason::ExplicitRedraw,
    }
}

#[derive(Default)]
struct PaneScrollbar {
    offset: usize,
    filled: usize,
}

impl PaneScrollbar {
    const fn visible(&self) -> bool {
        self.filled > 0
    }
}

fn pane_scrollbar(session: &mut Session, viewport_rows: u16, viewport_cols: u16) -> PaneScrollbar {
    let debug_enabled = crate::logging::debug_enabled();
    let (filled, vt_filled, inline_filled) = if debug_enabled {
        let (vt_filled, inline_filled) = session.scrollback_counts();
        (
            vt_filled.saturating_add(inline_filled),
            vt_filled,
            inline_filled,
        )
    } else {
        (session.scrollback_filled(), 0, 0)
    };
    let scrollbar = PaneScrollbar {
        offset: session.scrollback_offset,
        filled,
    };
    let metrics = if debug_enabled {
        screen_scroll_affordance_metrics(session.screen(), viewport_rows, viewport_cols)
    } else {
        None
    };
    crate::cdebug!(
        "scrollbar decision: agent={:?} alt_screen={} mouse_enabled={} viewport={}x{} screen={}x{} cursor={}x{} occupied_rows={} first_occupied_row={} last_occupied_row={} vt_scrollback={} inline_scrollback={} scrollback_filled={} visible={} reason={}",
        session.agent,
        session.screen().alternate_screen(),
        session.mouse_enabled(),
        viewport_rows,
        viewport_cols,
        metrics.as_ref().map_or(0, |m| m.screen_rows),
        metrics.as_ref().map_or(0, |m| m.screen_cols),
        metrics.as_ref().map_or(0, |m| m.cursor_row),
        metrics.as_ref().map_or(0, |m| m.cursor_col),
        metrics.as_ref().map_or(0, |m| m.occupied_rows),
        metrics
            .as_ref()
            .and_then(|m| m.first_occupied_row)
            .map_or(-1, i32::from),
        metrics
            .as_ref()
            .and_then(|m| m.last_occupied_row)
            .map_or(-1, i32::from),
        vt_filled,
        inline_filled,
        filled,
        scrollbar.visible(),
        if scrollbar.visible() {
            "retained-scrollback"
        } else {
            "none"
        }
    );
    scrollbar
}

/// Draw the pane box and optional scrollbar for one visible pane.
///
/// Called identically from compose_full_frame and compose_partial_frame;
/// lives here so both compositors stay in lock-step when the chrome rules
/// change.
fn draw_pane_chrome(
    buf: &mut Vec<u8>,
    pane: &VisiblePane,
    title: &str,
    scrollbar: PaneScrollbar,
    zoomed: bool,
    multi_pane: bool,
) {
    // Focused-border highlight: show the bright focus ring when the
    // operator must look at this pane to understand scroll state.
    let highlight_focus = if zoomed {
        scrollbar.visible()
    } else {
        multi_pane || scrollbar.visible()
    };
    draw_pane_box(
        buf,
        pane.outer.row,
        pane.outer.col,
        pane.outer.rows,
        pane.outer.cols,
        title,
        pane.focused && highlight_focus,
    );
    draw_scrollbar(
        buf,
        pane.outer.row,
        pane.outer.col,
        pane.outer.rows,
        pane.outer.cols,
        scrollbar.offset,
        scrollbar.filled,
        pane.focused && highlight_focus,
    );
}

struct ScrollAffordanceMetrics {
    screen_rows: u16,
    screen_cols: u16,
    cursor_row: u16,
    cursor_col: u16,
    occupied_rows: usize,
    first_occupied_row: Option<u16>,
    last_occupied_row: Option<u16>,
}

fn screen_scroll_affordance_metrics(
    screen: &vt100::Screen,
    viewport_rows: u16,
    viewport_cols: u16,
) -> Option<ScrollAffordanceMetrics> {
    let (screen_rows, screen_cols) = screen.size();
    let rows = viewport_rows.min(screen_rows);
    let cols = viewport_cols.min(screen_cols);
    if rows == 0 || cols == 0 {
        return None;
    }

    let mut occupied_rows = 0usize;
    let mut first_occupied_row = None;
    let mut last_occupied_row = None;
    for row in 0..rows {
        if (0..cols).any(|col| screen.cell(row, col).is_some_and(|c| c.has_contents())) {
            occupied_rows += 1;
            first_occupied_row.get_or_insert(row);
            last_occupied_row = Some(row);
        }
    }
    let (cursor_row, cursor_col) = screen.cursor_position();

    Some(ScrollAffordanceMetrics {
        screen_rows,
        screen_cols,
        cursor_row,
        cursor_col,
        occupied_rows,
        first_occupied_row,
        last_occupied_row,
    })
}

/// Format a spawn-failure banner: save cursor → jump to row 1, col 1
/// → bold red text → clear to end of line → restore cursor. The
/// save/restore wrap prevents the banner from scrolling whichever
/// pane the composed frame left the cursor in.
fn spawn_failure_banner(reason: &str) -> Vec<u8> {
    format!("\x1b7\x1b[1;1H\x1b[1;31mjackin: {reason}\x1b[0m\x1b[K\x1b8").into_bytes()
}

/// Forwarded to the operator's outer terminal via `send_output` from the
/// `CopyToClipboard` dialog action. The OSC 52 byte encoding and terminal
/// compatibility notes live with the canonical implementation in
/// `jackin_tui::ansi::encode_osc52_clipboard_write`; keeping that detail in
/// one place stops the two copies from drifting.
fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
    jackin_tui::ansi::encode_osc52_clipboard_write(payload)
}

fn osc22_pointer_shape(shape: PointerShape) -> Vec<u8> {
    format!("\x1b]22;{}\x1b\\", shape.as_osc22_name()).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::pr_context::{command_output_or_lookup_error, command_stdout_trimmed};
    use portable_pty::{ChildKiller, MasterPty, PtySize};

    #[derive(Debug)]
    struct NullChildKiller;

    impl ChildKiller for NullChildKiller {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(Self)
        }
    }

    struct NullMasterPty;

    impl MasterPty for NullMasterPty {
        fn resize(&self, _size: PtySize) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_size(&self) -> anyhow::Result<PtySize> {
            Ok(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
        }

        fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
            Ok(Box::new(std::io::empty()))
        }

        fn take_writer(&self) -> anyhow::Result<Box<dyn std::io::Write + Send>> {
            Ok(Box::new(std::io::sink()))
        }

        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<nix::libc::pid_t> {
            None
        }

        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
            None
        }

        #[cfg(unix)]
        fn tty_name(&self) -> Option<std::path::PathBuf> {
            None
        }
    }

    #[test]
    fn spawn_failure_banner_wraps_in_save_restore_and_carries_reason() {
        let bytes = spawn_failure_banner("boom: agent slug rejected");
        assert!(bytes.starts_with(b"\x1b7\x1b[1;1H"));
        assert!(bytes.ends_with(b"\x1b8"));
        assert!(
            bytes
                .windows(b"boom: agent slug rejected".len())
                .any(|w| w == b"boom: agent slug rejected"),
            "reason missing from banner: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        assert!(
            bytes.windows(2).any(|w| w == b"\x1b["),
            "missing SGR opener"
        );
    }

    fn test_mux(rows: u16, cols: u16) -> Multiplexer {
        Multiplexer::new(
            rows,
            cols,
            CapsuleConfig {
                role: "test-role".to_string(),
                workdir: "/workspace".to_string(),
                agents: Vec::new(),
                models: std::collections::BTreeMap::new(),
                initial_provider: None,
            },
        )
    }

    fn single_pane_tab_mux() -> Multiplexer {
        single_pane_tab_mux_with_size(24, 80)
    }

    fn single_pane_tab_mux_with_size(rows: u16, cols: u16) -> Multiplexer {
        let mut mux = test_mux(24, 80);
        mux.resize(rows, cols);
        mux.tabs.push(Tab::new_single("Shell", 1));
        mux
    }

    fn pull_request_fixture(number: u64) -> PullRequestInfo {
        PullRequestInfo {
            number,
            title: "Surface PR context in Capsule".to_string(),
            url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
            is_draft: false,
            checks: None,
        }
    }

    /// Build a 40-char SHA-1-shaped OID from a single hex nibble
    /// repeated 40 times. Tests want distinguishable OIDs ("H1", "H2",
    /// "H3") without the eye-strain of typing 40 hex digits inline.
    fn oid(nibble: char) -> Oid {
        assert!(nibble.is_ascii_hexdigit(), "nibble must be 0-9/a-f");
        Oid::parse(&nibble.to_string().repeat(40)).expect("40 hex chars is a valid Oid")
    }

    fn branch(name: &str) -> BranchName {
        BranchName::parse(name).expect("test branch names must parse")
    }

    /// Lay out a fake worktree under `temp` and return the
    /// (`workdir`, `common_git_dir`) paths the test can then write
    /// into. The `workdir/.git` pointer file is written so
    /// `read_context_from_git_metadata` discovers the per-worktree
    /// gitdir; the caller is responsible for writing HEAD + any
    /// `commondir` / `refs/heads/*` ref files specific to the
    /// scenario under test.
    fn make_worktree_layout(temp: &Path, worktree_name: &str) -> (PathBuf, PathBuf) {
        let workdir = temp.join("workdir");
        let common_git = temp.join("repo/.git");
        let wt_git = common_git.join(format!("worktrees/{worktree_name}"));
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::write(
            workdir.join(".git"),
            format!("gitdir: {}\n", wt_git.display()),
        )
        .unwrap();
        (workdir, common_git)
    }

    /// Construct the state production would land in after
    /// `maybe_spawn_pull_request_context_lookup` actually spawned a
    /// worker for `branch` (without shelling out to `gh`):
    /// `request_id` is the id the worker carries, `in_flight = true`
    /// gates the next spawn, `pull_request_context_branch` is the
    /// branch the worker was started for, and a `GitHubContext`
    /// dialog is open so apply-path redraw decisions exercise the
    /// dialog-open code path.
    fn arm_pending_pr_lookup(mux: &mut Multiplexer, branch_name: &str, request_id: u64) {
        mux.pull_request_lookup.request_id = request_id;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some(branch(branch_name));
        mux.open_github_context_dialog(Instant::now());
    }

    #[test]
    fn outer_terminal_title_uses_workspace_and_pr_title() {
        let title = compose_outer_terminal_title(
            Path::new("/Users/operator/Projects/jackin"),
            Some("feat/capsule-pr-context-bar"),
            Some(&pull_request_fixture(436)),
        );

        assert_eq!(title, "jackin · PR #436 · Surface PR context in Capsule");
    }

    #[test]
    fn outer_terminal_title_falls_back_to_branch_without_pr() {
        let title = compose_outer_terminal_title(
            Path::new("/Users/operator/Projects/jackin"),
            Some("feat/capsule-pr-context-bar"),
            None,
        );

        assert_eq!(title, "jackin · feat/capsule-pr-context-bar");
    }

    #[test]
    fn outer_terminal_title_sanitizes_control_bytes() {
        let pull_request = PullRequestInfo {
            number: 436,
            title: "bad\x1b]2;owned\x07title".to_string(),
            url: "https://github.com/jackin-project/jackin/pull/436".to_string(),
            is_draft: false,
            checks: None,
        };
        let title =
            compose_outer_terminal_title(Path::new("/workspace/jackin"), None, Some(&pull_request));

        assert_eq!(title, "jackin · PR #436 · bad ]2;owned title");
    }

    #[test]
    fn display_title_falls_back_when_shell_sets_empty_title() {
        let (mut session, _rx) = test_shell_session(20, 80);
        session.feed_pty(b"\x1b]2;\x07");

        assert_eq!(display_title(&session), "Test");
    }

    #[test]
    fn display_title_uses_shell_title_without_repeating_shell_label() {
        let (mut session, _rx) = test_shell_session(20, 80);
        session.feed_pty(b"\x1b]2;prompt title\x07");

        assert_eq!(display_title(&session), "prompt title");
    }

    #[test]
    fn display_title_uses_shell_cwd_without_repeating_shell_label() {
        let (mut session, _rx) = test_shell_session(20, 80);
        session.feed_pty(b"\x1b]7;file:///workspace/project\x07");

        assert_eq!(display_title(&session), "/workspace/project");
    }

    #[test]
    fn full_frame_emits_outer_terminal_title_once_until_context_changes() {
        let mut mux = single_pane_tab_mux();
        mux.workdir = PathBuf::from("/workspace/jackin");
        mux.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));

        let first =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            first.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
            "first frame should set branch title: {first:?}"
        );

        let second =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            !second.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
            "unchanged full frame should not spam title: {second:?}"
        );

        mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
        let updated =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            updated.contains("\x1b]2;jackin · PR #436 · Surface PR context in Capsule\x1b\\"),
            "PR context change should refresh title: {updated:?}"
        );
    }

    #[test]
    fn full_frame_updates_outer_terminal_title_on_branch_switch() {
        let mut mux = single_pane_tab_mux();
        mux.workdir = PathBuf::from("/workspace/jackin");
        mux.workdir_context.default_branch = Some("main".to_string());
        mux.pull_request_context_branch = Some(branch("feat/a"));

        let first =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            first.contains("\x1b]2;jackin · feat/a\x1b\\"),
            "first non-default branch should set title: {first:?}"
        );

        mux.pull_request_context_branch = Some(branch("feat/b"));
        let switched =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            switched.contains("\x1b]2;jackin · feat/b\x1b\\"),
            "branch switch should refresh title: {switched:?}"
        );

        mux.pull_request_context_branch = Some(branch("main"));
        let default_branch =
            String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::ExplicitRedraw))
                .to_string();
        assert!(
            default_branch.contains("\x1b]2;jackin\x1b\\"),
            "default branch should fall back to workspace-only title: {default_branch:?}"
        );
        assert!(
            !default_branch.contains("jackin · main"),
            "default branch name should not be propagated into title: {default_branch:?}"
        );
    }

    fn test_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        test_session_with_agent(rows, cols, Some("codex".to_string()))
    }

    fn test_shell_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        test_session_with_agent(rows, cols, None)
    }

    fn pane_kind_cases() -> [(Option<&'static str>, &'static str); 2] {
        [(Some("codex"), "agent"), (None, "shell")]
    }

    fn test_pane_session(
        rows: u16,
        cols: u16,
        agent: Option<&str>,
    ) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        test_session_with_agent(rows, cols, agent.map(ToString::to_string))
    }

    fn assert_focused_scroll_chrome(frame: &[u8], context: &str) {
        let rendered = String::from_utf8_lossy(frame);
        let focused_scroll_fg = format!(
            "{}{}",
            jackin_tui::ansi::RESET,
            jackin_tui::ansi::rgb_fg(jackin_tui::PHOSPHOR_GREEN)
        );
        assert!(
            rendered.contains(&focused_scroll_fg),
            "focused {context} should use green chrome"
        );
        assert!(
            rendered.contains('█'),
            "focused {context} should draw a scrollbar thumb"
        );
    }

    fn assert_no_scroll_thumb(frame: &[u8], context: &str) {
        assert!(
            !String::from_utf8_lossy(frame).contains('█'),
            "{context} should not draw fake scrollback chrome"
        );
    }

    fn assert_wheel_cursor_fallback_sent(
        input_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
        expected_bytes: &[u8],
    ) {
        assert_eq!(
            input_rx
                .try_recv()
                .expect("wheel fallback should reach PTY"),
            expected_bytes,
        );
        assert!(
            input_rx.try_recv().is_err(),
            "wheel should not produce extra PTY input"
        );
    }

    fn feed_top_anchored_inline_history(session: &mut Session, region_bottom: u16, lines: usize) {
        session.feed_pty(format!("\x1b[1;{region_bottom}r\x1b[{region_bottom};1H").as_bytes());
        for i in 0..lines {
            session.feed_pty(format!("\r\n\x1b[2Khistory {i}").as_bytes());
        }
        session.feed_pty(b"\x1b[r");
    }

    fn test_session_with_agent(
        rows: u16,
        cols: u16,
        agent: Option<String>,
    ) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        (
            Session::new_for_test(
                "Test".to_string(),
                agent,
                None,
                (rows, cols),
                100,
                input_tx,
                Arc::new(Mutex::new(Box::new(NullMasterPty))),
                Arc::new(Mutex::new(Box::new(NullChildKiller))),
            ),
            input_rx,
        )
    }

    fn test_provider_session(
        provider: jackin_protocol::Provider,
    ) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (mut session, input_rx) = test_session_with_agent(24, 80, Some("claude".to_string()));
        session.provider = Some(crate::session::SessionProvider {
            label: provider.label().to_string(),
            env_overrides: provider.env_overrides(Some("zai-test-token")),
        });
        (session, input_rx)
    }

    #[test]
    fn refresh_tab_labels_preserves_provider_suffix() {
        let mut mux = test_mux(24, 80);
        let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
        mux.sessions.insert(1, session);
        mux.tabs.push(Tab::new_single("Claude", 1));

        mux.refresh_tab_labels();

        assert_eq!(mux.tabs[0].label(), "Claude (Z.AI)");
    }

    #[test]
    fn split_metadata_inherits_focused_provider() {
        let mut mux = test_mux(24, 80);
        let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
        let expected_env = session
            .provider
            .as_ref()
            .map(|p| p.env_overrides.clone())
            .unwrap_or_default();
        mux.sessions.insert(1, session);
        mux.tabs.push(Tab::new_single("Claude (Z.AI)", 1));

        let (agent, env, provider) = mux.focused_spawn_metadata();

        assert_eq!(agent.as_deref(), Some("claude"));
        assert_eq!(provider.as_deref(), Some("Z.AI"));
        assert_eq!(env, expected_env);
    }

    fn split_tab_mux() -> Multiplexer {
        let mut mux = test_mux(24, 80);
        let mut tab = Tab::new_single("Shell", 1);
        assert!(tab.tree.split_h(1, 2, SplitPosition::After));
        mux.tabs.push(tab);
        mux
    }

    #[test]
    fn resize_zero_zero_normalizes_to_default_dimensions() {
        // A client sending Resize { rows: 0, cols: 0 } is asking for
        // "use the defaults"; the daemon must floor through
        // `normalize_size` and never store 0 in `term_rows`/`term_cols`,
        // because zero-row PTYs collapse vt100 rendering.
        let mut mux = test_mux(48, 160);
        mux.resize(0, 0);
        assert_eq!(
            (mux.term_rows, mux.term_cols),
            (
                crate::terminal_geometry::DEFAULT_ROWS,
                crate::terminal_geometry::DEFAULT_COLS
            )
        );
    }

    #[test]
    fn initial_spawn_request_is_data_only_agent_or_shell() {
        assert_eq!(
            initial_spawn_request("codex", None),
            SpawnRequest::Agent("codex".to_string())
        );
        assert_eq!(initial_spawn_request("", None), SpawnRequest::Shell);
    }

    #[test]
    fn initial_spawn_request_carries_provider_when_selected() {
        let provider = jackin_protocol::InitialProvider {
            label: jackin_protocol::Provider::Zai.label().to_string(),
        };
        assert_eq!(
            initial_spawn_request("claude", Some(&provider)),
            SpawnRequest::AgentWithProvider {
                slug: "claude".to_string(),
                provider_label: "Z.AI".to_string(),
            }
        );
        // An empty agent still degrades to a shell even with a provider.
        assert_eq!(
            initial_spawn_request("", Some(&provider)),
            SpawnRequest::Shell
        );
    }

    #[test]
    fn spawn_request_rejects_agent_outside_allowlist_before_pty_spawn() {
        let mut mux = test_mux(24, 80);
        mux.available_agents = vec!["codex".to_string()];

        let err = mux
            .spawn_request(SpawnRequest::Agent("claude".to_string()), &[])
            .unwrap_err();

        assert!(err.to_string().contains("rejected agent \"claude\""));
        assert!(mux.sessions.is_empty());
    }

    #[test]
    fn command_palette_labels_single_pane_close_as_close_tab() {
        let mut mux = single_pane_tab_mux();
        mux.open_command_palette();

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::CommandPalette {
                close_label: PaletteCloseLabel::CloseTab,
                ..
            })
        ));
    }

    #[test]
    fn dialog_opaque_backdrop_hides_multiplexer_chrome() {
        fn mux_with_two_sessions() -> Multiplexer {
            let mut mux = split_tab_mux();
            let (session_one, _) = test_session(24, 80);
            let (session_two, _) = test_shell_session(24, 80);
            mux.sessions.insert(1, session_one);
            mux.sessions.insert(2, session_two);
            mux
        }

        fn assert_backdrop_opaque(mut mux: Multiplexer, context: &str) {
            let frame =
                String::from_utf8_lossy(&mux.compose_full_frame(FullRedrawReason::DialogChange))
                    .to_string();

            assert!(
                frame.contains(jackin_tui::ansi::reset_rgb_bg(jackin_tui::DIALOG_BACKDROP)),
                "{context} should paint an opaque black backdrop: {frame:?}"
            );
            assert!(
                !frame.contains("jackin'"),
                "{context} should hide the top status brand pill behind the dialog: {frame:?}"
            );
            assert!(
                !frame.contains(&format!(
                    "{}┌",
                    jackin_tui::ansi::rgb_fg(jackin_tui::BORDER_GRAY)
                )),
                "{context} should hide inactive pane borders behind the dialog: {frame:?}"
            );
            assert!(
                !frame.contains(&format!(
                    "{}┌",
                    jackin_tui::ansi::rgb_fg(jackin_tui::PHOSPHOR_GREEN)
                )),
                "{context} should hide the active pane border behind the dialog: {frame:?}"
            );
        }

        let mut menu_mux = mux_with_two_sessions();
        menu_mux.open_command_palette();
        assert_backdrop_opaque(menu_mux, "menu dialog");

        let mut container_mux = mux_with_two_sessions();
        container_mux.open_container_info_dialog();
        assert_backdrop_opaque(container_mux, "container info dialog");

        let mut github_mux = mux_with_two_sessions();
        github_mux.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));
        github_mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
        github_mux.workdir_context.gh_available = false;
        github_mux.open_github_context_dialog(Instant::now());
        assert_backdrop_opaque(github_mux, "GitHub context dialog");
    }

    #[test]
    fn palette_close_single_pane_opens_confirm_directly() {
        let mut mux = single_pane_tab_mux();
        mux.handle_palette_command(PaletteCommand::Close);

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ConfirmAction {
                kind: ConfirmKind::CloseTab,
                selected_yes: false
            })
        ));
    }

    #[test]
    fn palette_close_split_tab_opens_target_picker() {
        let mut mux = split_tab_mux();
        mux.handle_palette_command(PaletteCommand::Close);

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::CloseTargetPicker {
                selected: 0,
                filter
            }) if filter.is_empty()
        ));
    }

    #[test]
    fn branch_context_bar_renders_pr_id_title_and_container_without_url() {
        let pr = pull_request_fixture(434);
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            120,
            Some("asa/pr-context"),
            Some(&pr),
            false,
            "jk-test-container",
            None,
        );
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("\x1b[24;1H"));
        assert!(rendered.contains("\x1b[48;2;255;255;255m"));
        assert!(rendered.contains("PR #434"));
        assert!(!rendered.contains("asa/pr-context"));
        assert!(rendered.contains("Surface PR context in Capsule"));
        assert!(rendered.contains("jk-test-container"));
        assert!(!rendered.contains("https://github.com/jackin-project/jackin/pull/434"));
        assert!(!rendered.contains("\x1b]8;;"));
    }

    #[test]
    fn branch_context_bar_renders_non_default_branch_without_pr() {
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            80,
            Some("feature/no-pr"),
            None,
            false,
            "jk-test-container",
            None,
        );
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Branch · feature/no-pr"));
        assert!(rendered.contains("jk-test-container"));
        assert!(!rendered.contains("\x1b]8;;"));
    }

    #[test]
    fn branch_context_bar_shows_pr_lookup_in_progress() {
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            100,
            Some("feature/slow-gh"),
            None,
            true,
            "jk-test-container",
            None,
        );
        let rendered = String::from_utf8_lossy(&buf);

        assert!(rendered.contains("Resolving PR · feature/slow-gh"));
        assert!(!rendered.contains("Branch · feature/slow-gh"));
    }

    #[test]
    fn branch_context_bar_truncates_left_chunk_on_narrow_terminal() {
        // 20-column terminal with a long PR title: the left chunk
        // must truncate via `take_display_cols` and the container
        // region must drop entirely rather than overlap the left
        // chunk. No panic from `u16::try_from` overflow.
        let mut pr = pull_request_fixture(999);
        pr.title =
            "Implement enormous feature with very long title that exceeds the bar".to_string();
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            20,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test-container-with-extra-long-suffix",
            None,
        );
        let rendered = String::from_utf8_lossy(&buf);
        assert!(rendered.contains("PR #999"));
        // The container label should be omitted at this width — its
        // chunk would otherwise collide with the (already-truncated)
        // left chunk.
        assert!(
            !rendered.contains("jk-test-container-with-extra-long-suffix"),
            "narrow terminal must drop container chunk: {rendered:?}"
        );
    }

    #[test]
    fn branch_context_bar_layout_returns_none_for_zero_dimensions() {
        let pr = pull_request_fixture(1);
        assert!(
            branch_context_bar_layout(0, 80, Some("feature/x"), Some(&pr), false, "jk-test")
                .is_none()
        );
        assert!(
            branch_context_bar_layout(24, 0, Some("feature/x"), Some(&pr), false, "jk-test")
                .is_none()
        );
    }

    #[test]
    fn branch_context_bar_hit_rejects_columns_outside_region() {
        let pr = pull_request_fixture(7);
        let layout =
            branch_context_bar_layout(24, 120, Some("feature/x"), Some(&pr), false, "jk-test")
                .expect("layout fits");
        // left_region covers exactly its declared range.
        let region = layout.left_region.expect("left region present");
        let left_start = region.start;
        let left_end = region.end;
        assert_eq!(
            branch_context_bar_hit(
                24,
                left_start,
                24,
                120,
                Some("feature/x"),
                Some(&pr),
                false,
                "jk-test"
            ),
            Some(BranchContextBarHit::Context)
        );
        assert_eq!(
            branch_context_bar_hit(
                24,
                left_end - 1,
                24,
                120,
                Some("feature/x"),
                Some(&pr),
                false,
                "jk-test"
            ),
            Some(BranchContextBarHit::Context)
        );
        // `end` is exclusive — `col == end` is outside the region.
        let outside_left = branch_context_bar_hit(
            24,
            left_end,
            24,
            120,
            Some("feature/x"),
            Some(&pr),
            false,
            "jk-test",
        );
        // The column may belong to the container region if it abuts.
        assert!(matches!(
            outside_left,
            None | Some(BranchContextBarHit::Container)
        ));
        // Wrong row — never a hit.
        assert_eq!(
            branch_context_bar_hit(
                23,
                left_start,
                24,
                120,
                Some("feature/x"),
                Some(&pr),
                false,
                "jk-test"
            ),
            None
        );
    }

    #[test]
    fn branch_context_bar_hover_highlights_click_targets() {
        let pr = pull_request_fixture(434);
        let mut context_buf = Vec::new();
        render_branch_context_bar(
            &mut context_buf,
            24,
            120,
            Some("asa/pr-context"),
            Some(&pr),
            false,
            "jk-test-container",
            Some(HoverTarget::BranchContext),
        );
        let context_rendered = String::from_utf8_lossy(&context_buf);
        assert!(context_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_BG));
        assert!(context_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_FG));

        let mut container_buf = Vec::new();
        render_branch_context_bar(
            &mut container_buf,
            24,
            120,
            Some("asa/pr-context"),
            Some(&pr),
            false,
            "jk-test-container",
            Some(HoverTarget::Container),
        );
        let container_rendered = String::from_utf8_lossy(&container_buf);
        assert!(container_rendered.contains(BRANCH_CONTEXT_BAR_HOVER_BG));
        assert!(container_rendered.contains("jk-test-container"));
    }

    #[test]
    fn branch_context_bar_leaves_left_side_empty_when_branch_filtered_out() {
        // `Multiplexer::context_bar_branch` is the layer that drops
        // default-branch names before this function runs, so the
        // post-filter input here is `None` regardless of whether the
        // operator is on main, master, trunk, develop, or detached HEAD.
        let mut buf = Vec::new();
        render_branch_context_bar(
            &mut buf,
            24,
            80,
            None,
            None,
            false,
            "jk-test-container",
            None,
        );
        let rendered = String::from_utf8_lossy(&buf);
        assert!(rendered.contains("jk-test-container"));
        assert!(!rendered.contains("jackin"));
        assert!(!rendered.contains("Branch ·"));
        assert!(!rendered.contains("Resolving PR"));
        assert!(!rendered.contains("PR #"));
        assert_eq!(
            branch_context_bar_hit(24, 2, 24, 80, None, None, false, "jk-test-container"),
            None
        );
    }

    #[test]
    fn branch_context_visibility_keeps_content_area_reserved() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        assert_eq!(mux.content_rows, 21);

        mux.pull_request_context_cache.insert(
            branch("asa/pr-context"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: None,
                pull_request: Some(Arc::new(pull_request_fixture(434))),
            },
        );
        assert!(mux.apply_git_branch_context(Some("asa/pr-context"), now));
        assert_eq!(mux.content_rows, 21);
        assert_eq!(
            mux.pull_request_context.as_deref().map(|pr| pr.number),
            Some(434)
        );

        mux.pull_request_context_cache.insert(
            branch("feature/no-pr"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: None,
                pull_request: None,
            },
        );
        assert!(mux.apply_git_branch_context(Some("feature/no-pr"), now));
        assert_eq!(mux.content_rows, 21);
        assert!(mux.pull_request_context.is_none());

        assert!(mux.apply_git_branch_context(Some("main"), now));
        assert_eq!(mux.content_rows, 21);
        assert!(mux.pull_request_context.is_none());
    }

    #[test]
    fn git_branch_context_updates_status_before_github_lookup() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some(branch("old/pr"));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
        mux.reconcile_content_rows();
        assert_eq!(mux.content_rows, 21);

        mux.pull_request_context_cache.insert(
            branch("new/local-branch"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: None,
                pull_request: None,
            },
        );
        assert!(mux.apply_git_branch_context(Some("new/local-branch"), now));

        assert_eq!(
            mux.pull_request_context_branch.as_deref(),
            Some("new/local-branch")
        );
        assert!(mux.pull_request_context.is_none());
        assert_eq!(mux.content_rows, 21);
    }

    #[test]
    fn git_branch_context_recognizes_repo_after_startup() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.workdir_context.is_git_repo = false;
        mux.workdir_context.gh_available = false;

        assert!(mux.apply_git_branch_context(Some("feat/capsule-pr-context-bar"), now));

        assert!(mux.workdir_context.is_git_repo);
        assert_eq!(
            mux.context_bar_branch(),
            Some("feat/capsule-pr-context-bar")
        );
        assert!(mux.pull_request_context.is_none());
    }

    #[test]
    fn apply_pull_request_context_loaded_drops_stale_request() {
        let mut mux = test_mux(24, 100);
        mux.pull_request_lookup.request_id = 5;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some(branch("feat/x"));
        let pr = pull_request_fixture(99);
        let changed = mux.apply_pull_request_context_loaded(
            3,
            Some(branch("feat/x")),
            None,
            PullRequestLookupOutcome::Resolved(Some(Arc::new(pr))),
            Instant::now(),
        );
        assert!(!changed, "stale request must not mutate state");
        assert!(
            mux.pull_request_lookup.in_flight,
            "stale request must leave in_flight untouched"
        );
        assert!(
            mux.pull_request_context.is_none(),
            "stale request must not write PR"
        );
    }

    #[test]
    fn apply_pull_request_context_loaded_transient_failure_preserves_prior_cache() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_lookup.request_id = 7;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(123)));
        mux.pull_request_context_cache.insert(
            branch("feat/x"),
            PullRequestContextCacheEntry {
                checked_at: now - Duration::from_secs(5),
                head: None,
                pull_request: Some(Arc::new(pull_request_fixture(123))),
            },
        );
        let changed = mux.apply_pull_request_context_loaded(
            7,
            Some(branch("feat/x")),
            None,
            PullRequestLookupOutcome::TransientFailure,
            now,
        );
        assert!(!changed, "transient failure must not mutate visible state");
        assert!(
            !mux.pull_request_lookup.in_flight,
            "transient failure must clear in_flight so next tick retries"
        );
        assert_eq!(
            mux.pull_request_context_cache
                .get("feat/x")
                .and_then(|e| e.pull_request.as_ref().map(|p| p.number)),
            Some(123),
            "cache must be untouched by transient failure"
        );
    }

    #[test]
    fn apply_pull_request_context_loaded_refreshes_open_github_dialog() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        arm_pending_pr_lookup(&mut mux, "feat/x", 7);

        let changed = mux.apply_pull_request_context_loaded(
            7,
            Some(branch("feat/x")),
            None,
            PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
            now,
        );

        assert!(changed, "dialog refresh should request redraw");
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::GitHubContext { copied: false })
        ));
        assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/x"));
        assert_eq!(
            mux.pull_request_context.as_ref().map(|pr| pr.number),
            Some(436)
        );
        assert!(!mux.pull_request_context_loading());
    }

    #[test]
    fn transient_pull_request_failure_clears_open_dialog_loading_state() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        arm_pending_pr_lookup(&mut mux, "feat/x", 7);

        let changed = mux.apply_pull_request_context_loaded(
            7,
            Some(branch("feat/x")),
            None,
            PullRequestLookupOutcome::TransientFailure,
            now,
        );

        assert!(changed, "dialog loading state changed");
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::GitHubContext { copied: false })
        ));
        assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/x"));
        assert!(mux.pull_request_context.is_none());
        assert!(!mux.pull_request_context_loading());
        assert!(
            !mux.pull_request_context_cache.contains_key("feat/x"),
            "transient failure must not cache a no-PR result"
        );
    }

    #[test]
    fn apply_git_branch_context_loaded_drops_stale_request() {
        let mut mux = test_mux(24, 100);
        mux.git_branch_lookup.request_id = 4;
        mux.git_branch_lookup.in_flight = true;
        let changed = mux.apply_git_branch_context_loaded(
            2,
            GitContext::Branch {
                name: branch("feat/x"),
                head: None,
            },
            Instant::now(),
        );
        assert!(!changed);
        assert!(mux.git_branch_lookup.in_flight, "stale id leaves in_flight");
        assert!(mux.pull_request_context_branch.is_none());
    }

    #[test]
    fn apply_git_branch_context_bumps_pr_request_id_on_branch_change() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some(branch("feat/a"));
        mux.workdir_context.gh_available = false;
        let id_before = mux.pull_request_lookup.request_id;
        let _ = mux.apply_git_branch_context(Some("feat/b"), now);
        assert_eq!(
            mux.pull_request_lookup.request_id,
            id_before.wrapping_add(1),
            "branch change must bump request_id so stale gh worker responses are rejected"
        );
    }

    #[test]
    fn apply_git_context_bumps_pr_request_id_on_same_branch_head_change() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some(branch("feat/a"));
        mux.pull_request_context_head = Some(oid('1'));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
        mux.pull_request_context_cache.insert(
            branch("feat/a"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: Some(oid('1')),
                pull_request: Some(Arc::new(pull_request_fixture(455))),
            },
        );
        mux.workdir_context.gh_available = false;
        let id_before = mux.pull_request_lookup.request_id;

        let changed = mux.apply_git_context(
            GitContext::Branch {
                name: branch("feat/a"),
                head: Some(oid('2')),
            },
            now,
        );

        assert!(
            changed,
            "visible PR context must clear on same-branch HEAD change"
        );
        assert_eq!(
            mux.pull_request_lookup.request_id,
            id_before.wrapping_add(1),
            "HEAD change must bump request_id so stale gh worker responses are rejected"
        );
        assert!(
            mux.pull_request_context.is_none(),
            "old PR cache must not stay visible for the new HEAD"
        );
    }

    #[test]
    fn purge_expired_pull_request_cache_entries_drops_old_entries() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        let ttl = PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2;
        mux.pull_request_context_cache.insert(
            branch("feat/fresh"),
            PullRequestContextCacheEntry {
                checked_at: now - Duration::from_secs(10),
                head: None,
                pull_request: Some(Arc::new(pull_request_fixture(1))),
            },
        );
        mux.pull_request_context_cache.insert(
            branch("feat/old"),
            PullRequestContextCacheEntry {
                checked_at: now - ttl - Duration::from_secs(1),
                head: None,
                pull_request: Some(Arc::new(pull_request_fixture(2))),
            },
        );
        mux.purge_expired_pull_request_cache_entries(now);
        assert!(mux.pull_request_context_cache.contains_key("feat/fresh"));
        assert!(!mux.pull_request_context_cache.contains_key("feat/old"));
    }

    #[test]
    fn pull_request_cache_fresh_at_strict_boundary() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        // Just-fresh: at the boundary minus 1 ms.
        mux.pull_request_context_cache.insert(
            branch("branch-a"),
            PullRequestContextCacheEntry {
                checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL + Duration::from_millis(1),
                head: None,
                pull_request: None,
            },
        );
        // Just-stale: at the boundary plus 1 ms.
        mux.pull_request_context_cache.insert(
            branch("branch-b"),
            PullRequestContextCacheEntry {
                checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL - Duration::from_millis(1),
                head: None,
                pull_request: None,
            },
        );
        assert!(mux.pull_request_cache_is_fresh("branch-a", now));
        assert!(!mux.pull_request_cache_is_fresh("branch-b", now));
    }

    #[test]
    fn pull_request_cache_fresh_requires_matching_head() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_head = Some(oid('2'));
        mux.pull_request_context_cache.insert(
            branch("branch-a"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: Some(oid('1')),
                pull_request: None,
            },
        );

        assert!(
            !mux.pull_request_cache_is_fresh("branch-a", now),
            "a cached no-PR answer from an older HEAD must not suppress a fresh lookup"
        );
    }

    #[test]
    fn pull_request_force_refresh_bypasses_fresh_no_pr_cache() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_cache.insert(
            branch("branch-a"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: None,
                pull_request: None,
            },
        );

        assert!(mux.pull_request_cache_blocks_lookup(
            "branch-a",
            now,
            PullRequestLookupMode::RespectCache
        ));
        assert!(!mux.pull_request_cache_blocks_lookup(
            "branch-a",
            now,
            PullRequestLookupMode::ForceRefresh
        ));
    }

    #[test]
    fn git_branch_context_keeps_current_pr_while_refreshing_same_branch() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some(branch("feature/current"));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_cache.insert(
            branch("feature/current"),
            PullRequestContextCacheEntry {
                checked_at: now - PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL,
                head: None,
                pull_request: Some(Arc::new(pull_request_fixture(436))),
            },
        );

        assert!(!mux.apply_git_branch_context(Some("feature/current"), now));
        assert_eq!(
            mux.pull_request_context.as_deref().map(|pr| pr.number),
            Some(436)
        );
    }

    #[test]
    fn cached_pull_request_stays_visible_during_forced_dialog_refresh() {
        let mut mux = test_mux(24, 100);
        mux.pull_request_context_branch = Some(branch("feature/current"));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
        mux.pull_request_lookup.in_flight = true;
        // Exercise the real dialog-open path so a future refactor that
        // skips force_spawn (or routes through a different dispatcher)
        // is caught here instead of by silent UX regression.
        mux.workdir_context.gh_available = false;
        mux.open_github_context_dialog(Instant::now());

        let view = mux.github_context_view();

        assert!(matches!(
            view.status,
            PullRequestStatus::Loaded(pr) if pr.number == 436
        ));
        assert!(
            !mux.pull_request_context_loading(),
            "known PR details should remain visible while a forced refresh runs in the background"
        );
    }

    #[test]
    fn open_github_context_dialog_force_spawns_when_gh_available() {
        let mut mux = test_mux(24, 100);
        mux.workdir_context.gh_available = true;
        mux.workdir_context.is_git_repo = true;
        mux.workdir_context.default_branch = Some("main".to_string());
        mux.pull_request_context_branch = Some(branch("feat/x"));
        let id_before = mux.pull_request_lookup.request_id;

        mux.open_github_context_dialog(Instant::now());

        assert!(
            mux.pull_request_lookup.in_flight,
            "dialog-open must fire a real worker spawn when gh_available is true"
        );
        assert_eq!(
            mux.pull_request_lookup.request_id,
            id_before.wrapping_add(1),
            "force-spawn must bump request_id"
        );
    }

    #[test]
    fn open_github_context_dialog_force_spawns_when_startup_missed_gh() {
        let mut mux = test_mux(24, 100);
        mux.workdir_context.gh_available = false;
        mux.workdir_context.is_git_repo = true;
        mux.workdir_context.default_branch = Some("main".to_string());
        mux.pull_request_context_branch = Some(branch("feat/x"));
        let id_before = mux.pull_request_lookup.request_id;

        mux.open_github_context_dialog(Instant::now());

        assert!(
            mux.pull_request_lookup.in_flight,
            "manual refresh must schedule a background lookup even when startup marked gh unavailable"
        );
        assert_eq!(
            mux.pull_request_lookup.request_id,
            id_before.wrapping_add(1),
            "manual refresh should not need a synchronous gh availability probe"
        );
        assert!(
            !mux.workdir_context.gh_available,
            "gh availability flips only after the background lookup succeeds"
        );
    }

    #[test]
    fn background_pull_request_success_marks_gh_available_after_startup_miss() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.workdir_context.gh_available = false;
        mux.workdir_context.is_git_repo = true;
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_lookup.request_id = 7;
        mux.pull_request_lookup.in_flight = true;

        let changed = mux.apply_pull_request_context_loaded(
            7,
            Some(branch("feat/x")),
            None,
            PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(436)))),
            now,
        );

        assert!(changed);
        assert!(
            mux.workdir_context.gh_available,
            "successful background gh lookup should unblock later conservative refreshes"
        );
    }

    #[test]
    fn open_github_context_dialog_bypasses_fresh_no_pr_cache() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.workdir_context.gh_available = true;
        mux.workdir_context.is_git_repo = true;
        mux.workdir_context.default_branch = Some("main".to_string());
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_context_cache.insert(
            branch("feat/x"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: None,
                pull_request: None,
            },
        );

        mux.open_github_context_dialog(now);

        assert!(
            mux.pull_request_lookup.in_flight,
            "manual dialog open must refresh even when a recent background lookup saw no PR"
        );
        assert!(
            mux.pull_request_context_loading(),
            "dialog should show resolving while the forced refresh is in flight"
        );
    }

    #[test]
    fn apply_git_context_head_change_schedules_fresh_pr_lookup() {
        // gh_available=true so the spawn path runs end-to-end; we assert
        // in_flight=true after the head flip to prove the maybe_spawn at
        // the tail of `apply_git_context` fires (not just request_id bump).
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.workdir_context.gh_available = true;
        mux.workdir_context.is_git_repo = true;
        mux.workdir_context.default_branch = Some("main".to_string());
        mux.pull_request_context_branch = Some(branch("feat/a"));
        mux.pull_request_context_head = Some(oid('1'));

        mux.apply_git_context(
            GitContext::Branch {
                name: branch("feat/a"),
                head: Some(oid('2')),
            },
            now,
        );

        assert!(
            mux.pull_request_lookup.in_flight,
            "head flip must schedule a fresh gh worker via maybe_spawn"
        );
    }

    #[test]
    fn apply_pull_request_context_loaded_refuses_head_mismatch() {
        // Defense-in-depth: request_id matched but mux.head drifted
        // between spawn and apply. The result MUST NOT overwrite
        // pull_request_context or land in the cache against the new head.
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_lookup.request_id = 9;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_context_head = Some(oid('a'));

        let changed = mux.apply_pull_request_context_loaded(
            9,
            Some(branch("feat/x")),
            Some(oid('b')),
            PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(777)))),
            now,
        );

        assert!(
            mux.pull_request_context.is_none(),
            "head-drift result must not be assigned to visible context"
        );
        assert!(
            !mux.pull_request_context_cache.contains_key("feat/x"),
            "head-drift result must not poison the cache"
        );
        assert!(
            !changed || mux.dialog_top().is_none(),
            "head-drift apply only flips loading state; no PR data assigned"
        );
    }

    #[test]
    fn apply_pull_request_context_loaded_refuses_head_drift_none_to_some() {
        // Spawn-time head was None (e.g. mid-write HEAD), apply-time
        // mux.head resolved to Some. Drift guard must refuse the spawn
        // payload — its data is keyed against the absent-head state.
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_lookup.request_id = 11;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_context_head = Some(oid('c'));

        let _ = mux.apply_pull_request_context_loaded(
            11,
            Some(branch("feat/x")),
            None,
            PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(778)))),
            now,
        );

        assert!(
            mux.pull_request_context.is_none(),
            "None→Some head drift refused"
        );
        assert!(!mux.pull_request_context_cache.contains_key("feat/x"));
    }

    #[test]
    fn apply_pull_request_context_loaded_refuses_head_drift_some_to_none() {
        // Inverse: spawn captured a head, apply-time mux.head was
        // cleared (e.g. HEAD became unreadable between spawn and apply).
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_lookup.request_id = 13;
        mux.pull_request_lookup.in_flight = true;
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_context_head = None;

        let _ = mux.apply_pull_request_context_loaded(
            13,
            Some(branch("feat/x")),
            Some(oid('d')),
            PullRequestLookupOutcome::Resolved(Some(Arc::new(pull_request_fixture(779)))),
            now,
        );

        assert!(
            mux.pull_request_context.is_none(),
            "Some→None head drift refused"
        );
        assert!(!mux.pull_request_context_cache.contains_key("feat/x"));
    }

    #[test]
    fn apply_git_context_simultaneous_branch_and_head_change_invalidates_cache() {
        let mut mux = test_mux(24, 100);
        let now = Instant::now();
        mux.pull_request_context_branch = Some(branch("feat/a"));
        mux.pull_request_context_head = Some(oid('1'));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(455)));
        mux.pull_request_context_cache.insert(
            branch("feat/a"),
            PullRequestContextCacheEntry {
                checked_at: now,
                head: Some(oid('1')),
                pull_request: Some(Arc::new(pull_request_fixture(455))),
            },
        );
        mux.workdir_context.gh_available = false;
        let id_before = mux.pull_request_lookup.request_id;

        let changed = mux.apply_git_context(
            GitContext::Branch {
                name: branch("feat/b"),
                head: Some(oid('2')),
            },
            now,
        );

        assert!(changed, "branch+head flip must dirty the visible context");
        assert_eq!(
            mux.pull_request_lookup.request_id,
            id_before.wrapping_add(1),
            "simultaneous branch+head flip must bump request_id once"
        );
        assert_eq!(mux.pull_request_context_branch.as_deref(), Some("feat/b"));
        assert_eq!(
            mux.pull_request_context_head.as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
        assert!(
            mux.pull_request_context.is_none(),
            "old PR cache entry under feat/a must not survive the branch flip"
        );
    }

    #[test]
    fn read_branch_from_git_head_reads_normal_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

        assert_eq!(
            read_branch_from_git_head(temp.path()).as_deref(),
            Some("feat/context")
        );
    }

    #[test]
    fn read_context_from_git_metadata_reads_loose_head_oid() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(git_dir.join("refs/heads/feat")).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();
        std::fs::write(
            git_dir.join("refs/heads/feat/context"),
            "1111111111111111111111111111111111111111\n",
        )
        .unwrap();

        let context = read_context_from_git_metadata(temp.path()).unwrap();

        assert_eq!(
            context.branch_name().map(BranchName::as_str),
            Some("feat/context")
        );
        assert_eq!(
            context.head().map(Oid::as_str),
            Some("1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn read_context_from_git_metadata_reads_packed_head_oid() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();
        std::fs::write(
            git_dir.join("packed-refs"),
            "\
# pack-refs with: peeled fully-peeled sorted
2222222222222222222222222222222222222222 refs/tags/v0.1.0
1111111111111111111111111111111111111111 refs/heads/feat/context
^3333333333333333333333333333333333333333
",
        )
        .unwrap();

        let context = read_context_from_git_metadata(temp.path()).unwrap();

        assert_eq!(
            context.branch_name().map(BranchName::as_str),
            Some("feat/context")
        );
        assert_eq!(
            context.head().map(Oid::as_str),
            Some("1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn read_packed_git_ref_oid_refreshes_after_metadata_change() {
        let temp = tempfile::tempdir().unwrap();
        let packed_refs = temp.path().join("packed-refs");
        std::fs::write(
            &packed_refs,
            "1111111111111111111111111111111111111111 refs/heads/feat/context\n",
        )
        .unwrap();

        assert_eq!(
            read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/context").as_deref(),
            Some("1111111111111111111111111111111111111111")
        );

        std::fs::write(
            &packed_refs,
            "\
# changed
2222222222222222222222222222222222222222 refs/heads/feat/context
",
        )
        .unwrap();

        assert_eq!(
            read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/context").as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
    }

    #[test]
    fn workdir_context_recognizes_direct_git_metadata_without_default_branch() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat/context\n").unwrap();

        let context = WorkdirContext::resolve(temp.path());

        assert!(context.is_git_repo);
    }

    #[test]
    fn read_branch_from_git_head_reads_worktree_gitdir_file() {
        let temp = tempfile::tempdir().unwrap();
        let (workdir, common_git) = make_worktree_layout(temp.path(), "workdir");
        let wt_git = common_git.join("worktrees/workdir");
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/worktree\n").unwrap();

        assert_eq!(
            read_branch_from_git_head(&workdir).as_deref(),
            Some("feat/worktree")
        );
    }

    #[test]
    fn oid_parse_accepts_sha1_and_sha256_lengths_only() {
        assert!(Oid::parse(&"a".repeat(40)).is_some());
        assert!(Oid::parse(&"F".repeat(40)).is_some());
        assert!(Oid::parse(&"0".repeat(64)).is_some());
        assert!(Oid::parse(&"f".repeat(64)).is_some());
        assert!(Oid::parse(&"a".repeat(39)).is_none());
        assert!(Oid::parse(&"a".repeat(41)).is_none());
        assert!(Oid::parse(&"a".repeat(63)).is_none());
        assert!(Oid::parse(&"a".repeat(65)).is_none());
        // Non-hex character at SHA-1 length.
        let mut s = "a".repeat(39);
        s.push('g');
        assert!(Oid::parse(&s).is_none());
    }

    #[test]
    fn read_context_from_git_metadata_reads_detached_head_oid() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            git_dir.join("HEAD"),
            "1111111111111111111111111111111111111111\n",
        )
        .unwrap();

        let context = read_context_from_git_metadata(temp.path()).unwrap();

        assert_eq!(context.branch_name(), None);
        assert_eq!(
            context.head().map(Oid::as_str),
            Some("1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn read_context_from_git_metadata_handles_malformed_head_content() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        // Neither `ref: ` prefix nor full hex OID — corruption / mid-write.
        std::fs::write(git_dir.join("HEAD"), "abc123\n").unwrap();

        let context = read_context_from_git_metadata(temp.path()).unwrap();

        assert_eq!(context.branch_name(), None);
        assert_eq!(
            context.head(),
            None,
            "malformed HEAD content must not be treated as an OID"
        );
    }

    #[test]
    fn read_context_from_git_metadata_handles_malformed_gitfile_content() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path();
        // `.git` is a file but does not start with `gitdir:` — corruption.
        std::fs::write(workdir.join(".git"), "not a gitdir pointer\n").unwrap();

        assert!(read_context_from_git_metadata(workdir).is_none());
    }

    #[test]
    fn apply_git_context_flips_is_git_repo_on_detached_head() {
        let mut mux = test_mux(24, 100);
        mux.workdir_context.is_git_repo = false;
        let now = Instant::now();

        mux.apply_git_context(GitContext::Detached { head: oid('1') }, now);

        assert!(
            mux.workdir_context.is_git_repo,
            "detached HEAD must promote is_git_repo (branch is None but head is Some)"
        );
    }

    #[test]
    fn read_context_from_git_metadata_resolves_worktree_head_via_commondir() {
        let temp = tempfile::tempdir().unwrap();
        let (workdir, common_git) = make_worktree_layout(temp.path(), "wt");
        let wt_git = common_git.join("worktrees/wt");
        std::fs::create_dir_all(common_git.join("refs/heads/feat")).unwrap();
        // Loose ref lives in the COMMON dir, not the per-worktree gitdir.
        std::fs::write(
            common_git.join("refs/heads/feat/wt"),
            "1111111111111111111111111111111111111111\n",
        )
        .unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/wt\n").unwrap();
        std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();

        let context = read_context_from_git_metadata(&workdir).unwrap();

        assert_eq!(context.branch_name(), Some(&branch("feat/wt")));
        assert_eq!(context.head(), Some(&oid('1')));
    }

    #[test]
    fn read_packed_git_ref_oid_does_not_cache_truncated_read() {
        // packed-refs cap forces a synthetic-truncation scenario: write
        // exactly PACKED_REFS_MAX_BYTES of content so read_text_bounded's
        // length equals the cap, then mutate underlying bytes and confirm
        // the second read sees the new value (would not, if the truncated
        // first read had cached).
        let temp = tempfile::tempdir().unwrap();
        let packed_refs = temp.path().join("packed-refs-truncated");
        // Pad with comment lines + a real ref entry until total length
        // matches the cap exactly.
        let real_line = "1111111111111111111111111111111111111111 refs/heads/feat/x\n";
        let padding_per_line = "# padding to fill packed-refs to the cap byte limit aaaaaaaaaa\n";
        // Target one byte OVER the cap so metadata.len() > cap triggers
        // the real truncation path (not the exactly-cap edge case).
        let target_size = PACKED_REFS_MAX_BYTES as usize + 1;
        let mut buf = String::with_capacity(target_size);
        while buf.len() + real_line.len() + padding_per_line.len() <= target_size {
            buf.push_str(padding_per_line);
        }
        buf.push_str(real_line);
        let remaining = target_size.saturating_sub(buf.len());
        buf.extend(std::iter::repeat_n('#', remaining));
        buf.truncate(target_size);
        std::fs::write(&packed_refs, &buf).unwrap();

        let _ = read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x");

        // Mutate same-length bytes (overwrite oid in place); mtime advances.
        let buf2 = buf.replacen(
            "1111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222",
            1,
        );
        std::fs::write(&packed_refs, &buf2).unwrap();

        assert_eq!(
            read_packed_git_ref_oid(&packed_refs, "refs/heads/feat/x").as_deref(),
            Some("2222222222222222222222222222222222222222"),
            "truncated first read must not have cached; second read sees fresh content"
        );
    }

    #[test]
    fn packed_refs_cache_eviction_bounds_entries_at_cap() {
        // Create CAP+1 distinct packed-refs paths and read each once.
        // After the (CAP+1)th insert, exactly CAP of the inserted
        // paths must remain — proves both the upper bound AND that
        // eviction removed only one entry (catches over-evict bugs
        // where the cache would degrade to a single entry).
        let temp = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for i in 0..=PACKED_REFS_CACHE_MAX_ENTRIES {
            let path = temp.path().join(format!("packed-refs-evict-{i}"));
            std::fs::write(
                &path,
                format!("1111111111111111111111111111111111111111 refs/heads/branch-{i}\n"),
            )
            .unwrap();
            let _ = read_packed_git_ref_oid(&path, &format!("refs/heads/branch-{i}"));
            paths.push(path);
        }

        let count = with_packed_refs_cache(|cache| {
            paths
                .iter()
                .filter(|p| cache.contains_key(p.as_path()))
                .count()
        });
        // The just-inserted (CAP+1)th entry MUST be present; eviction
        // targets pre-existing entries, never the new insert.
        assert!(
            with_packed_refs_cache(|cache| cache.contains_key(paths.last().unwrap().as_path())),
            "newly-inserted entry must survive eviction"
        );
        // Exactly one of the previously-inserted CAP entries must have
        // been evicted: count of our tracked paths in the cache should
        // equal CAP, not less (over-evict) or more (no-op evict).
        assert_eq!(
            count, PACKED_REFS_CACHE_MAX_ENTRIES,
            "eviction must drop exactly one entry; saw {count} surviving of CAP={}",
            PACKED_REFS_CACHE_MAX_ENTRIES
        );
    }

    #[test]
    fn read_git_ref_oid_loose_wins_over_packed() {
        let temp = tempfile::tempdir().unwrap();
        let git_dir = temp.path().to_path_buf();
        std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
        std::fs::write(
            git_dir.join("refs/heads/feat-x"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        )
        .unwrap();
        std::fs::write(
            git_dir.join("packed-refs"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feat-x\n",
        )
        .unwrap();

        assert_eq!(
            read_git_ref_oid(&git_dir, None, "refs/heads/feat-x").as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "loose ref must win over packed-refs entry"
        );
    }

    #[test]
    fn force_spawn_pull_request_context_lookup_skipped_when_in_flight() {
        let mut mux = test_mux(24, 100);
        mux.workdir_context.gh_available = true;
        mux.workdir_context.is_git_repo = true;
        mux.workdir_context.default_branch = Some("main".to_string());
        mux.pull_request_context_branch = Some(branch("feat/x"));
        mux.pull_request_lookup.in_flight = true;
        let id_before = mux.pull_request_lookup.request_id;

        let spawned = mux.force_spawn_pull_request_context_lookup(Instant::now());

        assert!(
            !spawned,
            "force-spawn must no-op when a worker is in flight"
        );
        assert_eq!(
            mux.pull_request_lookup.request_id, id_before,
            "force-spawn skip must not bump request_id"
        );
    }

    #[test]
    fn palette_exit_opens_exit_confirm() {
        let mut mux = single_pane_tab_mux();
        mux.handle_palette_command(PaletteCommand::Exit);

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ConfirmAction {
                kind: ConfirmKind::Exit,
                selected_yes: false
            })
        ));
    }

    #[test]
    fn kitty_escape_in_agent_picker_returns_to_menu() {
        let mut mux = single_pane_tab_mux();
        mux.open_command_palette();
        let frame = mux
            .handle_input(InputEvent::Data(b"\r".to_vec()))
            .expect("New tab command should redraw");
        assert!(String::from_utf8_lossy(&frame).contains("New tab"));
        assert!(matches!(mux.dialog_top(), Some(Dialog::AgentPicker { .. })));

        let events = mux.input_parser.parse(b"\x1b[27;1u");
        assert_eq!(events, vec![InputEvent::Data(b"\x1b".to_vec())]);
        for event in events {
            mux.handle_input(event);
        }

        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::CommandPalette { .. })
        ));
    }

    #[test]
    fn mouse_sgr_encoding_preserves_press_and_release() {
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, true, vt100::MouseProtocolEncoding::Sgr).unwrap(),
            b"\x1b[<0;12;3M"
        );
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, false, vt100::MouseProtocolEncoding::Sgr).unwrap(),
            b"\x1b[<0;12;3m"
        );
    }

    #[test]
    fn mouse_default_encoding_uses_xterm_fields() {
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, true, vt100::MouseProtocolEncoding::Default)
                .unwrap(),
            b"\x1b[M ,#"
        );
        assert_eq!(
            encode_mouse_for_protocol(0, 12, 3, false, vt100::MouseProtocolEncoding::Default)
                .unwrap(),
            b"\x1b[M#,#"
        );
    }

    #[test]
    fn mouse_mode_filter_respects_tracking_granularity() {
        use vt100::MouseProtocolMode;

        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::None,
            0,
            true
        ));
        assert!(mouse_event_allowed_for_mode(
            MouseProtocolMode::Press,
            0,
            true
        ));
        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::Press,
            0,
            false
        ));
        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::PressRelease,
            32,
            true
        ));
        assert!(mouse_event_allowed_for_mode(
            MouseProtocolMode::ButtonMotion,
            32,
            true
        ));
        assert!(!mouse_event_allowed_for_mode(
            MouseProtocolMode::ButtonMotion,
            SGR_NO_BUTTON_MOTION,
            true
        ));
        assert!(mouse_event_allowed_for_mode(
            MouseProtocolMode::AnyMotion,
            SGR_NO_BUTTON_MOTION,
            true
        ));
    }

    #[test]
    fn wheel_forwards_to_mouse_enabled_tui() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_session(20, 78);
        session.feed_pty(b"\x1b[?1049h\x1b[?1003h\x1b[?1006h");
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "pane-owned wheel should not redraw jackin'"
        );
        assert_eq!(
            input_rx.try_recv().expect("wheel should reach PTY"),
            b"\x1b[<64;1;1M"
        );
        assert!(
            input_rx.try_recv().is_err(),
            "wheel should not produce extra PTY input"
        );
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }

    #[test]
    fn wheel_scrolls_jackin_scrollback_when_mouse_is_disabled() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux();
            let (mut session, mut input_rx) = test_pane_session(20, 78, agent);
            for i in 0..40 {
                session.feed_pty(format!("line {i}\r\n").as_bytes());
            }
            assert_eq!(session.scrollback_offset, 0);
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            assert!(
                redraw.is_some(),
                "{pane_kind} pane scrollback should redraw jackin'"
            );
            assert!(
                input_rx.try_recv().is_err(),
                "mouse-disabled {pane_kind} panes must not receive raw wheel bytes"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
        }
    }

    #[test]
    fn wheel_noops_for_focused_normal_screen_pane_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(55, 200);
            let (mut session, mut input_rx) = test_pane_session(51, 198, agent);
            session.feed_pty(b"\x1b[49;3Hcodex prompt");
            assert_eq!(session.scrollback_filled(), 0);
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 10,
                col: 10,
                button: 64,
            });

            assert!(
                redraw.is_none(),
                "{pane_kind} normal-screen pane without scrollback should not redraw jackin'"
            );
            assert!(
                input_rx.try_recv().is_err(),
                "normal-screen {pane_kind} pane without scrollback must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
        }
    }

    #[test]
    fn wheel_scrolls_top_anchored_inline_history_for_all_panes() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(12, 40);
            let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
            feed_top_anchored_inline_history(&mut session, 5, 12);
            session.feed_pty(b"\x1b[8;1Hlive prompt");
            assert!(
                session.scrollback_filled() >= 3,
                "{pane_kind} pane should retain top-anchored inline history"
            );
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            let frame = redraw.expect("inline history wheel should redraw");
            assert!(
                input_rx.try_recv().is_err(),
                "{pane_kind} pane must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
            assert_focused_scroll_chrome(
                &frame,
                &format!("normal-screen {pane_kind} pane with inline history"),
            );
            assert!(
                String::from_utf8_lossy(&frame).contains("history 4"),
                "normal-screen {pane_kind} wheel should render retained inline history"
            );
        }
    }

    #[test]
    fn wheel_scrolls_normal_screen_history_preserved_before_clear_for_all_panes() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(12, 40);
            let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
            for i in 0..5 {
                session.feed_pty(format!("release note {i}\r\n").as_bytes());
            }
            assert_eq!(
                session.scrollback_filled(),
                0,
                "{pane_kind} setup output fits without native scrollback before clear"
            );

            session.feed_pty(b"\x1b[1;1H\x1b[Jlive prompt");
            assert!(
                session.scrollback_filled() >= 5,
                "{pane_kind} pane should preserve normal-screen rows erased by clear/redraw"
            );
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            let frame = redraw.expect("clear-preserved history wheel should redraw");
            assert!(
                input_rx.try_recv().is_err(),
                "{pane_kind} pane must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 3);
            assert_focused_scroll_chrome(
                &frame,
                &format!("normal-screen {pane_kind} pane with clear-preserved history"),
            );
            assert!(
                String::from_utf8_lossy(&frame).contains("release note"),
                "normal-screen {pane_kind} wheel should render rows preserved before clear"
            );
        }
    }

    #[test]
    fn wheel_scrolls_csi_scroll_up_inline_history_for_all_panes() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(12, 40);
            let (mut session, mut input_rx) = test_pane_session(8, 38, agent);
            session.feed_pty(b"\x1b[1;5r\x1b[1;1Htop row\x1b[2;1Hsecond row\x1b[3;1Hthird row");
            session.feed_pty(b"\x1b[2S\x1b[r\x1b[8;1Hlive prompt");
            assert!(
                session.scrollback_filled() >= 2,
                "{pane_kind} pane should retain rows removed by top-anchored CSI S"
            );
            mux.sessions.insert(1, session);

            let redraw = mux.handle_input(InputEvent::MousePress {
                row: STATUS_BAR_ROWS + 1,
                col: 1,
                button: 64,
            });

            let frame = redraw.expect("CSI S inline history wheel should redraw");
            assert!(
                input_rx.try_recv().is_err(),
                "{pane_kind} pane must not receive cursor-key wheel fallback"
            );
            assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 2);
            assert_focused_scroll_chrome(
                &frame,
                &format!("normal-screen {pane_kind} pane with CSI S inline history"),
            );
            assert!(
                String::from_utf8_lossy(&frame).contains("top row"),
                "normal-screen {pane_kind} wheel should render CSI S retained history"
            );
        }
    }

    #[test]
    fn wheel_sends_cursor_fallback_to_mouse_disabled_alt_screen_tui() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_shell_session(20, 78);
        session.feed_pty(b"\x1b[?1049h");
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "pane-owned fallback should not redraw jackin'"
        );
        assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }

    #[test]
    fn wheel_sends_cursor_fallback_to_alt_screen_tui_with_retained_primary_scrollback() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_shell_session(20, 78);
        for i in 0..40 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert!(
            session.scrollback_filled() > 0,
            "setup should leave retained primary-screen scrollback"
        );
        session.feed_pty(b"\x1b[?1049h");
        assert!(
            session.screen().alternate_screen(),
            "setup should leave pane in the alternate screen"
        );
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 64,
        });

        assert!(
            redraw.is_none(),
            "alternate-screen fallback should not redraw jackin'"
        );
        assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(mux.sessions.get(&1).unwrap().scrollback_offset, 0);
    }

    #[test]
    fn wheel_cursor_fallback_respects_application_cursor_mode() {
        let mut mux = single_pane_tab_mux();
        let (mut session, mut input_rx) = test_session(20, 78);
        session.feed_pty(b"\x1b[?1049h\x1b[?1h");
        mux.sessions.insert(1, session);

        let redraw = mux.handle_input(InputEvent::MousePress {
            row: STATUS_BAR_ROWS + 1,
            col: 1,
            button: 65,
        });

        assert!(
            redraw.is_none(),
            "pane-owned fallback should not redraw jackin'"
        );
        assert_wheel_cursor_fallback_sent(&mut input_rx, b"\x1bOB\x1bOB\x1bOB");
    }

    #[test]
    fn alt_screen_overflow_does_not_draw_scrollbar_without_retained_scrollback() {
        let mut mux = single_pane_tab_mux();
        let (mut session, _input_rx) = test_session(8, 20);
        session.feed_pty(b"\x1b[?1049h");
        for i in 0..20 {
            session.feed_pty(format!("line {i}\r\n").as_bytes());
        }
        assert_eq!(session.scrollback_filled(), 0);
        mux.sessions.insert(1, session);

        let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
        assert_no_scroll_thumb(&frame, "alt-screen pane without retained scrollback");
    }

    #[test]
    fn normal_screen_panes_do_not_draw_scrollbar_when_grid_is_full_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux();
            let (mut session, _input_rx) = test_pane_session(8, 20, agent);
            for row in 0..8 {
                session.feed_pty(format!("\x1b[{};1Hrow {row}", row + 1).as_bytes());
            }
            mux.sessions.insert(1, session);

            let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
            assert_no_scroll_thumb(
                &frame,
                &format!("normal-screen {pane_kind} pane with full grid but no scrollback"),
            );
        }
    }

    #[test]
    fn normal_screen_panes_do_not_draw_scrollbar_when_content_spans_viewport_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux();
            let (mut session, _input_rx) = test_pane_session(8, 20, agent);
            session.feed_pty(b"\x1b[1;1Htop transcript\x1b[8;1Hbottom status");
            assert_eq!(session.scrollback_filled(), 0);
            mux.sessions.insert(1, session);

            let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
            assert_no_scroll_thumb(
                &frame,
                &format!(
                    "normal-screen {pane_kind} pane with viewport-spanning content but no scrollback"
                ),
            );
        }
    }

    #[test]
    fn normal_screen_panes_do_not_keep_scrollbar_when_cursor_moves_without_scrollback() {
        for (agent, pane_kind) in pane_kind_cases() {
            let mut mux = single_pane_tab_mux_with_size(55, 200);
            let (mut session, _input_rx) = test_pane_session(51, 198, agent);
            session.feed_pty(b"\x1b[1;1Hrelease notes\x1b[51;1Hstatus line\x1b[48;3Hx");
            assert_eq!(session.scrollback_filled(), 0);
            mux.sessions.insert(1, session);

            let frame = mux.compose_full_frame(FullRedrawReason::FirstAttach);
            assert_no_scroll_thumb(
                &frame,
                &format!("normal-screen {pane_kind} transcript pane after cursor moved up"),
            );
        }
    }

    #[test]
    fn alt_screen_exit_resets_keyboard_modes_for_shell_prompt() {
        let (mut session, _input_rx) = test_session(8, 20);
        session.feed_pty(b"\x1b[?1049h\x1b[>1u\x1b[>4;2m");
        let _ = session.drain_passthrough();

        session.feed_pty(b"\x1b[?1049l");
        let drained = session.drain_passthrough();

        assert!(
            drained.iter().any(|bytes| bytes == b"\x1b[<u"),
            "kitty keyboard reset missing from {drained:?}"
        );
        assert!(
            drained.iter().any(|bytes| bytes == b"\x1b[>4;0m"),
            "modifyOtherKeys reset missing from {drained:?}"
        );
    }

    #[test]
    fn osc22_pointer_shape_uses_css_names() {
        assert_eq!(
            osc22_pointer_shape(PointerShape::Pointer),
            b"\x1b]22;pointer\x1b\\"
        );
        assert_eq!(
            osc22_pointer_shape(PointerShape::EwResize),
            b"\x1b]22;ew-resize\x1b\\"
        );
    }

    #[test]
    fn pointer_shape_updates_only_when_shape_changes() {
        let mut mux = test_mux(24, 80);
        mux.pointer_shapes_supported = true;
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.instance_id_label = "test".to_string();
        mux.pull_request_context_branch = Some(branch("feature/context"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let hit = branch_context_bar_layout(
            mux.term_rows,
            mux.term_cols,
            mux.pull_request_context_branch.as_deref(),
            mux.pull_request_context.as_deref(),
            mux.pull_request_context_loading(),
            mux.status_bar.instance_id_label(),
        )
        .and_then(|layout| layout.left_region)
        .expect("branch context should fit");

        mux.update_pointer_shape_for_mouse(23, hit.start - 1, SGR_NO_BUTTON_MOTION);
        let first = rx.try_recv().expect("first pointer-shape update");
        assert!(first.ends_with(b"\x1b]22;pointer\x1b\\"));

        mux.update_pointer_shape_for_mouse(23, hit.start, SGR_NO_BUTTON_MOTION);
        assert!(rx.try_recv().is_err(), "unchanged shape should not re-emit");
    }

    #[test]
    fn pointer_shape_updates_for_clickable_top_chrome() {
        let mut mux = single_pane_tab_mux();
        mux.pointer_shapes_supported = true;
        let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let tab_col = mux
            .status_bar
            .tab_regions
            .first()
            .map(|(start, _)| start.saturating_sub(1))
            .expect("tab region should render");

        mux.update_pointer_shape_for_mouse(0, tab_col, SGR_NO_BUTTON_MOTION);
        let tab_shape = rx.try_recv().expect("tab pointer-shape update");
        assert!(tab_shape.ends_with(b"\x1b]22;pointer\x1b\\"));

        let mut mux = single_pane_tab_mux();
        mux.pointer_shapes_supported = true;
        let _ = mux.compose_full_frame(FullRedrawReason::ExplicitRedraw);
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let menu_col = mux
            .status_bar
            .hint_region
            .map(|(start, _)| start.saturating_sub(1))
            .expect("menu region should render");

        mux.update_pointer_shape_for_mouse(0, menu_col, SGR_NO_BUTTON_MOTION);
        let menu_shape = rx.try_recv().expect("menu pointer-shape update");
        assert!(menu_shape.ends_with(b"\x1b]22;pointer\x1b\\"));
    }

    #[test]
    fn pointer_shape_updates_for_clickable_dialog_copy_target() {
        let mut mux = single_pane_tab_mux();
        mux.pointer_shapes_supported = true;
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.open_container_info_dialog();
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let dialog = mux.dialog_top().expect("container info dialog should open");
        let (row, col, _, _) = dialog.box_rect(mux.term_rows, mux.term_cols);

        mux.update_pointer_shape_for_mouse(
            row.saturating_add(1),
            col.saturating_add(1),
            SGR_NO_BUTTON_MOTION,
        );
        let shape = rx.try_recv().expect("dialog pointer-shape update");
        assert!(shape.ends_with(b"\x1b]22;pointer\x1b\\"));
    }

    #[test]
    fn bottom_container_click_opens_container_info_without_copying() {
        let mut mux = test_mux(24, 80);
        mux.pointer_shapes_supported = false;
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.instance_id_label = "test".to_string();
        mux.status_bar.role = "the-architect".to_string();
        mux.pull_request_context_branch = Some(branch("feature/context"));
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let hit = branch_context_bar_layout(
            mux.term_rows,
            mux.term_cols,
            mux.pull_request_context_branch.as_deref(),
            mux.pull_request_context.as_deref(),
            mux.pull_request_context_loading(),
            mux.status_bar.instance_id_label(),
        )
        .and_then(|layout| layout.container_region)
        .expect("container should fit");

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: mux.term_rows - 1,
                col: hit.start - 1,
                button: 0,
            })
            .expect("container click should redraw");

        while let Ok(output) = rx.try_recv() {
            assert!(
                !output
                    .windows(b"\x1b]52;c;".len())
                    .any(|w| w == b"\x1b]52;c;"),
                "opening container info must not send OSC 52"
            );
        }
        assert!(!String::from_utf8_lossy(&frame).contains("Copied!"));
        let Some(Dialog::ContainerInfo {
            copied: false,
            workdir,
            ..
        }) = mux.dialog_top()
        else {
            panic!("identity click should open container info")
        };
        assert_eq!(workdir, "/workspace");
    }

    #[test]
    fn bottom_context_click_opens_github_context_dialog() {
        let mut mux = test_mux(24, 100);
        mux.status_bar.identity_label = "jk-test-container".to_string();
        mux.status_bar.instance_id_label = "test".to_string();
        mux.pull_request_context_branch = Some(branch("feature/context"));
        mux.pull_request_context = Some(Arc::new(pull_request_fixture(434)));
        mux.workdir_context.gh_available = false;
        let hit = branch_context_bar_layout(
            mux.term_rows,
            mux.term_cols,
            mux.pull_request_context_branch.as_deref(),
            mux.pull_request_context.as_deref(),
            mux.pull_request_context_loading(),
            mux.status_bar.instance_id_label(),
        )
        .and_then(|layout| layout.left_region)
        .expect("GitHub context should fit");

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: mux.term_rows - 1,
                col: hit.start - 1,
                button: 0,
            })
            .expect("context click should redraw");

        let rendered = String::from_utf8_lossy(&frame);
        assert!(rendered.contains("GitHub context"));
        assert!(
            rendered.contains("copy GitHub URL"),
            "dialog hint must render above the bottom branch/context bar: {rendered:?}"
        );
        assert!(
            rendered.rfind("copy GitHub URL") > rendered.rfind("test"),
            "dialog footer should be painted after the bottom branch/context bar so it clears its own rows: {rendered:?}"
        );
        let hint_row = mux.term_rows - 2;
        let bottom_row = mux.term_rows;
        assert!(
            rendered.contains(&format!("\x1b[{hint_row};")),
            "dialog hint should render one row above the spacer: {rendered:?}"
        );
        assert!(
            rendered.contains(&format!("\x1b[{bottom_row};")),
            "bottom branch/context bar should stay on the final row: {rendered:?}"
        );
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::GitHubContext { copied: false })
        ));
        assert_eq!(
            mux.pull_request_context_branch.as_deref(),
            Some("feature/context")
        );
        assert_eq!(
            mux.pull_request_context.as_ref().map(|pr| pr.number),
            Some(434)
        );
    }

    #[test]
    fn container_info_copy_feedback_expires() {
        let mut mux = test_mux(24, 80);
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            copied: true,
        });
        let now = Instant::now();
        mux.dialog_copy_feedback_deadline = Some(now);

        assert!(mux.expire_dialog_copy_feedback(now));
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ContainerInfo { copied: false, .. })
        ));
    }

    #[test]
    fn container_info_id_click_copies_and_renders_feedback() {
        let mut mux = test_mux(40, 120);
        mux.pointer_shapes_supported = false;
        mux.dialog_push(Dialog::ContainerInfo {
            container_name: "jk-test-container".to_string(),
            role: "the-architect".to_string(),
            focused_agent: Some("claude".to_string()),
            workdir: "/workspace".to_string(),
            copied: false,
        });
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.attached_out = Some(tx);
        let (box_row, box_col, _, _) = mux
            .dialog_top()
            .expect("container info dialog should be open")
            .box_rect(mux.term_rows, mux.term_cols);

        let frame = mux
            .handle_input(InputEvent::MousePress {
                row: box_row + 1,
                col: box_col + 1,
                button: 0,
            })
            .expect("container id click should redraw copy feedback");

        let mut saw_osc52 = false;
        while let Ok(output) = rx.try_recv() {
            saw_osc52 |= output
                .windows(b"\x1b]52;c;".len())
                .any(|w| w == b"\x1b]52;c;");
        }
        assert!(saw_osc52, "copy should emit OSC 52");
        assert!(String::from_utf8_lossy(&frame).contains("Copied!"));
        assert!(matches!(
            mux.dialog_top(),
            Some(Dialog::ContainerInfo { copied: true, .. })
        ));
    }

    #[test]
    fn prefix_ctrl_l_has_named_pane_clear_reason() {
        assert_eq!(
            prefix_full_redraw_reason(&PrefixCommand::ClearPane),
            FullRedrawReason::PaneClear
        );
    }

    #[test]
    fn command_stdout_trimmed_returns_trimmed_stdout() {
        let mut command = Command::new("printf");
        command.arg("  branch-name\n");

        assert_eq!(
            command_stdout_trimmed(&mut command),
            Some("branch-name".to_string())
        );
    }

    #[test]
    fn command_stdout_trimmed_rejects_known_failure_status() {
        // `sleep 0.05` keeps the child alive long enough for the
        // try_wait poll loop to observe `Ok(None)` first and then the
        // failing `Ok(Some(1))` exit on the next tick. Without the
        // sleep the child can vanish between spawn and the first
        // try_wait, which collapses the Err(ECHILD) "status lost"
        // arm and the Ok(Some(false)) "failed" arm into one path.
        let mut command = Command::new("sh");
        command.args(["-c", "printf branch-name; sleep 0.05; exit 1"]);

        assert_eq!(command_stdout_trimmed(&mut command), None);
    }

    #[test]
    fn gh_lookup_output_rejects_statusless_stderr_only_failure() {
        let err = command_output_or_lookup_error("gh", None, b"", b"HTTP 401: Bad credentials\n")
            .expect_err("stderr-only statusless gh output is a transient failure");

        assert!(
            err.to_string().contains("HTTP 401"),
            "stderr detail should survive for logs: {err}"
        );
    }

    // Action-boundary dispatch tests: drive apply_action directly without
    // going through handle_input so the dispatch layer is testable without
    // a live PTY or input parser in the loop.

    #[test]
    fn apply_action_dismiss_closes_top_dialog() {
        let mut mux = single_pane_tab_mux();
        mux.open_command_palette();
        assert!(mux.dialog_open(), "palette should be open");

        mux.apply_action(Action::Dialog(DialogAction::Dismiss));

        assert!(!mux.dialog_open(), "dismiss should close the dialog");
        assert_eq!(mux.mux_mode(), MuxMode::Normal);
    }

    #[test]
    fn apply_action_open_palette_pushes_palette_dialog() {
        let mut mux = single_pane_tab_mux();
        assert!(!mux.dialog_open());

        mux.apply_action(Action::OpenPalette);

        assert!(
            matches!(mux.dialog_top(), Some(Dialog::CommandPalette { .. })),
            "OpenPalette should push CommandPalette dialog"
        );
        assert_eq!(mux.mux_mode(), MuxMode::Dialog);
    }

    #[test]
    fn apply_action_dialog_consume_keeps_dialog_open() {
        let mut mux = single_pane_tab_mux();
        mux.open_command_palette();
        assert!(mux.dialog_open());

        // Consume should leave the dialog open (key was absorbed, no state change).
        mux.apply_action(Action::Dialog(DialogAction::Consume));

        assert!(mux.dialog_open(), "Consume must not close the dialog");
    }

    #[test]
    fn apply_action_focus_report_does_not_open_dialog() {
        let mut mux = single_pane_tab_mux();
        assert!(!mux.dialog_open());

        mux.apply_action(Action::FocusReport(true));
        assert!(!mux.dialog_open());

        mux.apply_action(Action::FocusReport(false));
        assert!(!mux.dialog_open());
    }
}
