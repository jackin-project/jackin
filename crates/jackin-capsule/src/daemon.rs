//! In-container multiplexer daemon: accepts attach connections, manages PTY
//! sessions, dispatches input, and renders the status bar.
//!
//! Not responsible for: PTY I/O (see `session`), socket binding (see
//! `socket`), or terminal rendering (see `tui`).
//!
//! Key invariant: at most one attach client is active at a time; a new
//! `Hello` frame displaces the previous client.

use chrono::{DateTime, Utc};
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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use portable_pty::CommandBuilder;

use crate::agent_status::rules::RulePackRegistry;
use crate::attach_protocol::{
    AttachHandshake, ControlRequest, detach_attached_task, detach_client, drain_and_exit,
    drain_and_exit_with_reason, handle_attach_client, initial_spawn_request, perform_handshake,
    spawn_request_label,
};
use crate::clipboard::{
    CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT, ClipboardImageTransfers, cleanup_clipboard_run_dir,
    stage_clipboard_image,
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
use crate::pr_context::gh_pull_request_info;
use crate::protocol::attach::{
    AttachCapabilities, ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_server,
};
use crate::protocol::control::SessionInfo;
use crate::pull_request::PullRequestInfo;
use crate::session::{
    BranchName, GitContext, Oid, PullRequestLookupOutcome, SESSION_ENV_PASSTHROUGH, Session,
    SessionEvent, build_agent_command, build_shell_command,
};
use crate::socket;
use crate::token_monitor::{TokenMonitor, TokenTotals};
#[cfg(test)]
use crate::tui::components::branch_context_bar::branch_context_bar_layout;
#[cfg(test)]
use crate::tui::components::dialog::ConfirmKind;
use crate::tui::components::dialog::{
    Dialog, DialogAction, GithubContextView, InspectRow, PaletteCloseLabel, PaletteCommand,
    PickerIntent, SplitDirection, github_context_view_from_state,
};
use crate::tui::components::status_bar::prefix_mode_for_mux_mode;
use crate::tui::components::status_bar::{STATUS_BAR_ROWS, StatusBar};
use crate::tui::effect::InitialFrameKind;
#[cfg(test)]
use crate::tui::input::mouse_event_allowed_for_mode;
use crate::tui::input::{
    ArrowDir, DEFAULT_ESCAPE_TIME, ENV_ESCAPE_TIME, InputEvent, InputParser, PrefixCommand,
    SGR_NO_BUTTON_MOTION, encode_mouse_for_protocol, encode_wheel_cursor_fallback,
    mouse_event_encoding_for_mode, pane_wheel_cursor_fallback_reason,
};
#[cfg(test)]
use crate::tui::layout::SplitOrient;
use crate::tui::layout::{
    Direction, Rect, SplitDirectionGeometry, SplitPosition, Tab, available_content_rows,
    content_rect, local_mouse_position, split_spawn_inner_size,
};
use crate::tui::message::{
    Action, ConfirmedActionRoute, InputDispatchContext, PaletteCommandRoute, PaletteToggleRoute,
    StatusBarClickState, branch_context_bar_click_action, confirmed_action_route,
    input_event_action, mouse_chrome_update_action, mouse_release_action, palette_command_route,
    palette_toggle_route, pane_button_motion_action, prefix_command_action,
    status_bar_click_action,
};
use crate::tui::model::{
    ChromeHitState, CursorVisibilityState, DragState, HoverState, HoverTarget, MuxMode,
    MuxModeState, PointerShape, PointerShapeState, VisiblePane, chrome_hover_target_for_state,
    cursor_visible_for_state, hover_target_for_state, mux_mode_for_state, pointer_shape_for_state,
    visible_panes_for_layout,
};
use crate::tui::selection::{
    SelectionState, move_selection_end, selection_start_for_inner_rect, selection_text,
    selection_was_dragged,
};
use crate::tui::subscriptions::{
    GIT_BRANCH_CONTEXT_POLL_INTERVAL, PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL, RENDER_TICK_INTERVAL,
    STATE_TICK_INTERVAL, USAGE_ACCOUNT_REFRESH_POLL_INTERVAL,
};
use crate::tui::terminal::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};
use crate::tui::title::{
    append_osc_window_title, compose_outer_terminal_title, pane_display_title,
};
#[cfg(test)]
use crate::tui::update::prefix_full_redraw_reason;
use crate::tui::update::{
    FullRedrawReason, HoverFramePlan, dialog_action_frame_plan, dialog_change_redraw_reason,
    drag_resize_ratio, drag_resize_redraw_reason, explicit_redraw_reason,
    first_attach_redraw_reason, focus_change_redraw_reason, hover_frame_plan,
    palette_route_frame_plan, pane_data_redraw_reason, selection_change_redraw_reason,
    selection_start_redraw_reason, session_exit_redraw_reason, status_change_redraw_reason,
    wheel_scrollback_redraw_reason,
};
use crate::tui::view::spawn_request_failure_message;
use crate::usage::UsageCache;
use jackin_core::agent::Agent;
use jackin_protocol::control::{ClientMsg, ServerMsg};

mod compositor;
mod context_mgmt;
mod dialog_mgmt;
mod file_export;
mod input_dispatch;
mod mouse_input;
mod multiplexer_utils;
mod pane_layout;
mod resource_metrics;
mod session_lifecycle;

fn session_display_title(session: &Session) -> String {
    pane_display_title(session.title(), session.cwd(), &session.label)
}

struct SessionLaunch {
    label: String,
    cmd: CommandBuilder,
}

#[expect(
    missing_debug_implementations,
    reason = "Multiplexer owns PTY sessions and render/input state; targeted debug logs expose the useful fields."
)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal multiplexer state flags (detach_requested, \
              selection_copied, pointer_shapes_supported, tab_bar_focused) \
              — each tracks an independent runtime state consumed individually \
              by the event loop + compositor branches. Named-field reads match \
              the direct mutation idiom the impl blocks use."
)]
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
    /// them again. Sub-dialogs (Menu → New tab → `AgentPicker`,
    /// Menu → Split pane → `SplitDirectionPicker` → `AgentPicker`,
    /// Menu → Close → `CloseTargetPicker` / `ConfirmClose`, …) push onto
    /// this stack so Esc walks the operator back one step at a time
    /// instead of nuking the whole flow. The empty stack means "no
    /// dialog open" — every consumer treats `dialog_top()` as the
    /// canonical "is a dialog visible" check.
    dialog_stack: Vec<Dialog>,
    content_rows: u16,
    available_agents: Vec<String>,
    launch_config: CapsuleConfig,
    /// Control-channel reply slot for an in-flight `jackin-exec` credential
    /// picker. Set when an `ExecCommand` opens the `Dialog::ExecPicker`; the
    /// confirm/cancel handlers take it to send `ExecResult` / `ExecDenied`. A
    /// new `ExecCommand` arriving while one is pending denies the prior reply
    /// with `ExecDenied { reason: "superseded …" }` (in `begin_exec_picker`) so
    /// that client gets a structured answer rather than a dropped connection.
    pending_exec_reply: Option<tokio::sync::oneshot::Sender<ServerMsg>>,
    /// Set by the dirty-exit modal's keep/discard rows; the event loop writes
    /// the host exit-action file and drains on the next iteration.
    exit_request: Option<jackin_protocol::ExitAction>,
    env_passthrough: Vec<(String, String)>,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    zoomed: Option<u64>,
    input_parser: InputParser,
    detach_requested: bool,
    /// The only writer to the attach socket: composed frames are
    /// `?2026`-bracketed, out-of-band bytes flush at frame boundaries.
    pub(crate) client: crate::client_writer::ClientWriter,
    /// `JoinHandle` of the spawned `handle_attach_client` task for the
    /// currently-attached client. Tracked so a takeover (second `Hello`)
    /// can abort the old task's reader loop — without the abort, the
    /// old client's stale Input / Resize / Detach frames keep flowing
    /// into the shared `cmd_tx` until its socket finally closes.
    pub(crate) attached_task: Option<tokio::task::JoinHandle<()>>,
    /// Records the previous tab-cell click so a second click on the
    /// same tab within `TAB_DOUBLE_CLICK_WINDOW` is treated as a
    /// double-click (open the rename modal).
    last_tab_click: Option<(usize, Instant)>,
    /// Active mouse-drag resize, if any. Populated when the operator
    /// presses the left button on a shared pane border; updated on
    /// every motion event; cleared on release.
    drag: Option<DragState>,
    /// Active mouse text selection on a pane whose program ignored
    /// the mouse. Updated on every motion event; copied to the
    /// outer clipboard via OSC 52 on release.
    selection: Option<SelectionState>,
    /// Candidate text selection captured on primary press. Promoted to
    /// `selection` only after real drag motion leaves the anchor cell.
    pending_selection: Option<SelectionState>,
    /// Previous primary press on a pane cell, kept one click long so the
    /// next press can be classified as a double-click (word select).
    last_pane_press: Option<mouse_input::PanePress>,
    /// True after a dragged selection was copied and its highlight remains
    /// visible. Cleared by the next click or typed input.
    selection_copied: bool,
    selection_copy_feedback_deadline: Option<Instant>,
    /// Transient operator-facing result of a host clipboard image paste:
    /// staged path, dialog-owned-input warning, or rejected payload reason.
    clipboard_image_notice: Option<String>,
    clipboard_image_notice_deadline: Option<Instant>,
    clipboard_image_transfers: ClipboardImageTransfers,
    clipboard_image_insert_mode: ClipboardImageInsertMode,
    /// Monotonic state-change counter: every mutation that can affect the
    /// visible frame bumps it via `invalidate`. The render loop composes
    /// when it moved past `rendered_generation` — there are no repaint
    /// tiers and no per-cause request flags (derived rendering, §3.2 of
    /// the capsule rendering plan).
    frame_generation: u64,
    /// Generation the last composed frame reflected.
    rendered_generation: u64,
    /// Wipe policy: a real `\x1b[2J` precedes the next frame only for
    /// `FirstAttach` and `Resize` — the geometry events whose previous
    /// layout must not survive. Every other invalidation repaints in place.
    wipe_pending: Option<FullRedrawReason>,
    /// Telemetry: the most recent invalidation reason, labelled on the next
    /// composed frame's debug trace.
    last_invalidate_reason: Option<FullRedrawReason>,
    /// Cursor + mode state the encoder asserted with the last frame; the
    /// per-frame reconciliation emits only transitions against this. `None`
    /// (fresh attach) asserts everything explicitly.
    last_asserted_client_state: Option<compositor::AssertedClientState>,
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
    /// Host-adaptive terminal capabilities derived from the active attach
    /// client. This backend-side record may change on reattach and must not
    /// alter agent-visible terminal model semantics.
    attached_capabilities: AttachCapabilities,
    /// Hash of the last multiplexer-owned OSC 2 title sent to the
    /// outer terminal. Gates re-emission on inequality: without the
    /// diff, every full frame would reassert the workspace/PR title
    /// and override per-pane agent-set titles in the outer terminal's
    /// tab list on every redraw. Reset to `None` when a child pane
    /// updates its own title so the next full frame re-asserts.
    last_outer_terminal_title: Option<String>,
    hover_target: Option<HoverTarget>,
    /// Link target under an Alt/Ctrl hover in a mouse-disabled pane. Rendered
    /// as a compositor-owned notice so no hover bytes are written into the PTY.
    link_hover_url: Option<String>,
    /// P5: focus is on the agent-tab bar (green underline + Left/Right switch
    /// tabs; Down/Esc/click returns focus to the agent content). `false` means
    /// the agent terminal holds focus, the default.
    tab_bar_focused: bool,
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
    /// instead of `$HOME` (`portable_pty`'s `CommandBuilder` default).
    workdir: PathBuf,
    /// API keys captured from the operator env at construction, keyed by the
    /// provider that consumes them. A provider is present only when its
    /// [`key_env_var`](jackin_protocol::Provider::key_env_var) was set and
    /// non-empty. Populated once over [`jackin_protocol::Provider::ALL`], so a
    /// new provider needs no new field, env read, or match arm here.
    provider_keys: BTreeMap<jackin_protocol::Provider, String>,
    /// Cached at construction for the hot polling path. The only
    /// mutation after that is `gh_available` flipping false → true when
    /// a background PR lookup succeeds, so a startup PATH /
    /// tool-availability race does not freeze PR discovery for the
    /// daemon lifetime.
    workdir_context: WorkdirContext,
    /// Ratatui terminal backed by [`SocketBackend`].
    ///
    /// Chrome widgets (status bar, pane boxes, dialogs) render through this
    /// terminal for full-frame draws so they can use shared `jackin-tui`
    /// components. The raw ANSI compositor remains as the fallback and partial
    /// update path while the remaining render migration proceeds.
    ratatui_terminal: ratatui::Terminal<crate::tui::socket_backend::SocketBackend>,
    /// Shared terminal row arena for every pane in this daemon. All
    /// `DamageGrid`s draw primary, alternate, and scrollback rows from this
    /// store so closing a session returns row buffers for later panes.
    terminal_row_arena: jackin_term::RowArena,
    /// Codenames currently assigned to open tabs.
    /// A codename in `codename_live` is NOT in `codename_retired`.
    codename_live: HashSet<String>,
    /// All codenames ever assigned in this container lifetime. Never shrinks.
    /// A codename that moves from `live` to here on tab close is never
    /// reassigned — prevents agents from confusing a new tab for a closed one.
    codename_retired: HashSet<String>,
    /// Append-only history of every tab ever opened. Never pruned.
    agent_history: Vec<AgentRecord>,
    /// Debug-only process RSS/CPU sampler, emitted on the state ticker so live
    /// multi-pane smokes can attach resource data to the run id.
    resource_metrics: resource_metrics::ResourceMetricsSampler,
    /// Daemon-owned focused usage/quota cache. Capsule UI renders this cache;
    /// it does not poll providers from render code.
    usage_cache: UsageCache,
    /// Daemon-owned per-session token-spend monitor. Reconciled against the
    /// live agent sessions and polled on the state tick; provider reads are
    /// owned here, never in render or client code.
    token_monitor: TokenMonitor,
    /// Provider tab requested by the usage overlay. The normal usage refresh
    /// ticker consumes this as a focused-first target so opening/switching tabs
    /// stays non-blocking but the selected provider refreshes next.
    pending_usage_refresh: Option<crate::usage::UsageRefreshTarget>,
    /// Background account refresh worker. Provider probes can run HTTP
    /// requests and CLI subprocesses, so the daemon select loop only starts
    /// and joins this task; it never performs the probe work inline.
    usage_refresh_task: Option<tokio::task::JoinHandle<UsageCache>>,
    /// Offset into the wordlist for the next codename pick, seeded once at
    /// daemon construction from the current time subsecond nanos.
    wordlist_offset: usize,
}

/// In-memory record of one tab ever opened in this container lifetime.
/// The history is append-only and never pruned; it is the authoritative
/// data source for `jackin-capsule agents` and the tab hover tooltip.
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub session_id: u64,
    pub codename: String,
    /// Agent slug (`"claude"`, `"codex"`, …), or `None` for shell sessions.
    pub agent: Option<String>,
    /// Provider label (e.g. `"Z.AI"`), or `None` when no provider selected.
    pub provider: Option<String>,
    pub started_at: DateTime<Utc>,
    pub exited_at: Option<DateTime<Utc>>,
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

/// Hard cap on simultaneous tabs. 32 is well past any operator
/// workflow but small enough that an accidental loop of new-tab
/// requests cannot drive the container OOM.
const MAX_TABS: usize = 32;

/// Hard cap on simultaneous sessions (panes). Splits within tabs
/// can grow the session count past the tab count; cap separately
/// for the same memory-bounding reason.
const MAX_SESSIONS: usize = 64;

impl Multiplexer {
    pub fn new(rows: u16, cols: u16, launch_config: CapsuleConfig) -> io::Result<Self> {
        let (rows, cols) = normalize_size(rows, cols);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = available_content_rows(rows);
        let agents = launch_config.supported_agents();
        let provider_keys: BTreeMap<jackin_protocol::Provider, String> =
            jackin_protocol::Provider::ALL
                .into_iter()
                .filter_map(|provider| {
                    let var = provider.key_env_var()?;
                    let value = std::env::var(var).ok().filter(|v| !v.is_empty())?;
                    Some((provider, value))
                })
                .collect();

        let env_passthrough: Vec<(String, String)> = SESSION_ENV_PASSTHROUGH
            .iter()
            .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_owned(), v)))
            .collect();

        let input_bindings = crate::services::input_bindings::resolve_input_bindings();
        let input_parser = InputParser::new(input_bindings.prefix, input_bindings.palette_key);
        let workdir = PathBuf::from(&launch_config.workdir);
        let workdir_context = WorkdirContext::resolve(&workdir);
        crate::clog!(
            "workdir-context: git_available={} gh_available={} is_git_repo={} default_branch={:?}",
            workdir_context.git_available,
            workdir_context.gh_available,
            workdir_context.is_git_repo,
            workdir_context.default_branch
        );
        let status_identity = crate::container_context::resolve_status_identity();
        let mut status_bar = StatusBar::new_with_role_labels(
            launch_config.role.clone(),
            status_identity.container_name,
            status_identity.instance_id,
        );
        status_bar.set_prefix_enabled(input_parser.prefix_enabled());

        let ratatui_terminal =
            ratatui::Terminal::new(crate::tui::socket_backend::SocketBackend::new(cols, rows))?;

        Ok(Self {
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
            pending_exec_reply: None,
            exit_request: None,
            env_passthrough,
            event_tx,
            event_rx,
            zoomed: None,
            input_parser,
            detach_requested: false,
            client: crate::client_writer::ClientWriter::default(),
            attached_task: None,
            last_tab_click: None,
            drag: None,
            selection: None,
            pending_selection: None,
            last_pane_press: None,
            selection_copied: false,
            selection_copy_feedback_deadline: None,
            clipboard_image_notice: None,
            clipboard_image_notice_deadline: None,
            clipboard_image_transfers: ClipboardImageTransfers::default(),
            clipboard_image_insert_mode: ClipboardImageInsertMode::PastePath,
            frame_generation: 0,
            rendered_generation: 0,
            wipe_pending: None,
            last_invalidate_reason: None,
            last_asserted_client_state: None,
            pointer_shape: PointerShape::Default,
            pointer_shapes_supported: false,
            attached_terminal: ClientTerminal::default(),
            attached_capabilities: AttachCapabilities::default(),
            last_outer_terminal_title: None,
            hover_target: None,
            link_hover_url: None,
            tab_bar_focused: false,
            dialog_copy_feedback_deadline: None,
            pull_request_context_branch: None,
            pull_request_context_head: None,
            pull_request_context: None,
            git_branch_lookup: LookupState::default(),
            pull_request_lookup: LookupState::default(),
            pull_request_context_cache: HashMap::new(),
            workdir,
            workdir_context,
            provider_keys,
            ratatui_terminal,
            terminal_row_arena: jackin_term::RowArena::default(),
            codename_live: HashSet::new(),
            codename_retired: HashSet::new(),
            agent_history: Vec::new(),
            resource_metrics: resource_metrics::ResourceMetricsSampler::default(),
            usage_cache: UsageCache::default(),
            token_monitor: TokenMonitor::new(),
            pending_usage_refresh: None,
            usage_refresh_task: None,
            wordlist_offset: {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(42, |d| d.subsec_nanos() as usize)
            },
        })
    }

    /// Send a composed frame to the attached client through the single
    /// writer. Queued out-of-band bytes flush ahead of the bracketed frame.
    fn send_frame(&mut self, bytes: Vec<u8>) {
        self.client.write_frame(bytes);
    }

    /// Queue bytes that are not cell content (OSC passthrough, clipboard,
    /// pointer shapes, mode prefaces); they flush at the next frame boundary.
    pub(crate) fn send_out_of_band(&mut self, bytes: Vec<u8>) {
        self.client.enqueue_out_of_band(bytes);
    }

    /// Send a typed attach protocol frame that is not terminal output.
    fn send_protocol_frame(&mut self, frame: ServerFrame) {
        self.client.send_protocol_frame(frame);
    }

    fn request_clipboard_image_from_text_path(&mut self) {
        self.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        self.send_protocol_frame(ServerFrame::HostStageImageFromClipboardPath);
    }

    fn request_clipboard_image_paste(&mut self) {
        self.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        self.send_protocol_frame(ServerFrame::HostPasteImageFromClipboard);
    }

    fn request_clipboard_image_stage_only(&mut self) {
        self.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;
        self.send_protocol_frame(ServerFrame::HostStageImageFromClipboard);
    }

    fn stage_clipboard_image_response(&mut self, image: jackin_protocol::attach::ClipboardImage) {
        self.stage_clipboard_image_response_with(image, stage_clipboard_image);
    }

    fn stage_clipboard_image_response_with<F>(
        &mut self,
        image: jackin_protocol::attach::ClipboardImage,
        stage: F,
    ) where
        F: FnOnce(&jackin_protocol::attach::ClipboardImage) -> Result<PathBuf>,
    {
        let insert_mode = std::mem::take(&mut self.clipboard_image_insert_mode);
        match stage(&image) {
            Ok(path) => {
                let path = path.to_string_lossy();
                let bytes = image.bytes.len();
                crate::clog!(
                    "clipboard-image: staged extension={} bytes={} path={path}",
                    image.format.extension(),
                    bytes
                );
                if insert_mode == ClipboardImageInsertMode::StageOnly {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes)"
                    ));
                } else if self.dialog_captures_input() {
                    crate::clog!(
                        "clipboard-image: ignored staged path because a dialog owns input"
                    );
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes; dialog focused; not pasted)"
                    ));
                } else if self.paste_text_to_focused_pane(path.as_bytes()) {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes)"
                    ));
                } else {
                    crate::clog!(
                        "clipboard-image: staged path not pasted because no writable focused pane was available"
                    );
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes; no writable focused pane; not pasted)"
                    ));
                }
            }
            Err(err) => {
                log_clipboard_image_rejection("payload", &err);
                self.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ClipboardImageInsertMode {
    #[default]
    PastePath,
    StageOnly,
}

fn log_clipboard_image_rejection(stage: &str, err: &anyhow::Error) {
    let reason = clipboard_image_error_reason(err);
    crate::clog!("clipboard-image: rejected reason={reason} stage={stage}");
    crate::cdebug!("clipboard-image: rejected stage={stage} detail={err:#}");
}

fn clipboard_image_error_reason(err: &anyhow::Error) -> &'static str {
    classify_clipboard_image_error(&format!("{err:#}"))
}

fn clipboard_image_host_error_reason(message: &str) -> &'static str {
    classify_clipboard_image_error(message)
}

fn classify_clipboard_image_error(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("empty") {
        "empty"
    } else if lower.contains("exceeds cap")
        || lower.contains("too large")
        || lower.contains("over cap")
    {
        "oversize"
    } else if lower.contains("magic")
        || lower.contains("signature")
        || lower.contains("not an image")
        || lower.contains("unsupported image")
    {
        "signature-mismatch"
    } else if lower.contains("sha-256") || lower.contains("digest") {
        "digest-mismatch"
    } else if lower.contains("offset") || lower.contains("did not match expected") {
        "offset-mismatch"
    } else if lower.contains("no active start") {
        "missing-transfer"
    } else if lower.contains("already active") {
        "duplicate-transfer"
    } else if lower.contains("display")
        || lower.contains("wayland")
        || lower.contains("xclip")
        || lower.contains("wl-paste")
        || lower.contains("wl-copy")
    {
        "backend-unavailable"
    } else if lower.contains("create")
        || lower.contains("creating")
        || lower.contains("open")
        || lower.contains("opening")
        || lower.contains("write")
        || lower.contains("writing")
        || lower.contains("flush")
        || lower.contains("flushing")
        || lower.contains("permission")
        || lower.contains("metadata")
        || lower.contains("read")
        || lower.contains("reading")
    {
        "staging-io"
    } else {
        "invalid-payload"
    }
}

#[cfg(test)]
use crate::client_writer::scan_emitted_frame;

/// Build the read-only Inspect rows for the dirty-exit modal: a section header
/// per dirty repo followed by its `<status> <path>` change rows.
fn build_exit_inspect_rows(repos: &[crate::exit_assess::DirtyRepo]) -> Arc<[InspectRow]> {
    use crate::tui::components::dialog::InspectRow;
    let mut rows = Vec::new();
    for repo in repos {
        rows.push(InspectRow::Repo(repo.label().to_owned()));
        for f in &repo.changed {
            rows.push(InspectRow::File(format!("{} {}", f.status, f.path)));
        }
    }
    rows.into()
}

/// Handle the last live session exiting. Returns `true` when the daemon should
/// exit and `false` to keep the event loop running — either because a dirty-exit
/// modal was just opened, or because the modal flow is already in progress
/// (re-entry guard). With policy `ask` and dirty isolated work the modal is
/// shown (no teardown); otherwise the container drains and exits, preserving the
/// original non-clean-exit reason.
async fn handle_last_session_exit(mux: &mut Multiplexer, reason: Option<String>) -> bool {
    // Called from two sites: the session-exit event handler (once, on last-session
    // exit) and the client-frame handler (on every frame while no sessions remain).
    // The guard below handles the client-frame re-entry case: if a dialog is already
    // open (modal, Inspect view, or New-tab picker launched from "Start a new agent")
    // the dirty-exit flow is already active. Re-entering would push a fresh modal
    // and re-run the git assessment on every keypress, resetting selection to 0 —
    // so the operator could never move past the first row. Defer until resolved.
    if mux.dialog_open() {
        return false;
    }
    match crate::exit_assess::decide_exit(&mux.launch_config).await {
        crate::exit_assess::ExitDecision::Drain => {
            if let Some(ref r) = reason {
                crate::clog!("session: final session exited: {r}");
            }
            drain_and_exit_with_reason(mux, reason).await;
            true
        }
        crate::exit_assess::ExitDecision::DrainWithAction(action) => {
            // Policy keep/discard: record the action for the host, no prompt.
            // Write failure is logged but does not block exit — a configured
            // policy path cannot stall indefinitely waiting for a broken fs.
            if let Err(error) = crate::exit_assess::write_exit_action(action) {
                crate::output::stderr_line(format_args!(
                    "[daemon] exit: failed to write exit-action file, policy will not be applied: {error}"
                ));
            }
            drain_and_exit_with_reason(mux, reason).await;
            true
        }
        crate::exit_assess::ExitDecision::ShowModal(repos) => {
            crate::clog!(
                "exit: {} dirty repo(s) with policy ask — showing in-capsule dirty-exit modal",
                repos.len()
            );
            let summary = repos
                .iter()
                .map(crate::exit_assess::DirtyRepo::summary_line)
                .collect();
            let inspect_rows = build_exit_inspect_rows(&repos);
            mux.dialog_push(Dialog::new_exit_dirty(summary, inspect_rows));
            mux.invalidate(FullRedrawReason::DialogChange);
            false
        }
    }
}

async fn handle_state_tick(mux: &mut Multiplexer, rule_registry: Option<&RulePackRegistry>) {
    mux.log_resource_metrics().await;
    mux.maybe_spawn_pull_request_context_lookup(Instant::now());
    // Reap idle clipboard-image transfers and surface a notice. Must NOT
    // short-circuit the tick: agent-state advancement below is the 1 Hz floor —
    // every session re-evaluates each tick — and a clipboard reap is an
    // orthogonal concern that must not freeze it. The `invalidate` guarantees
    // the notice repaints even if no agent state changed this tick (otherwise
    // the no-change return below would leave the frame clean and the notice
    // never painted).
    let stale_image_transfers = mux
        .clipboard_image_transfers
        .abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    if stale_image_transfers > 0 {
        crate::clog!(
            "clipboard-image: cleaned up {stale_image_transfers} idle transfer{}",
            if stale_image_transfers == 1 { "" } else { "s" }
        );
        mux.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        mux.set_clipboard_image_notice(format!(
            "Image paste interrupted: cleaned up {stale_image_transfers} idle transfer{}",
            if stale_image_transfers == 1 { "" } else { "s" }
        ));
        mux.invalidate(status_change_redraw_reason());
    }
    // Evidence arbitration is the ONLY path that authors agent state. Each
    // session assembles an EvidenceSnapshot (authority, process, OSC, screen)
    // in `advance_status`, arbitrates to a raw state + confidence, and
    // publishes through SessionStatus (which derives the public `effective`
    // state, incl. done-from-seen).
    let now = Instant::now();
    // Token-spend monitor: keep it synced to the live agent sessions and poll
    // any due providers. `poll_due_sessions` self-throttles to the 30s/60s
    // cadence, so calling it each state tick is cheap.
    let token_sessions: Vec<(u64, Agent)> = mux
        .sessions
        .iter()
        .filter_map(|(id, s)| Some((*id, Agent::from_slug(s.agent.as_deref()?)?)))
        .collect();
    mux.token_monitor.reconcile_sessions(&token_sessions);
    // Returned changed-id list is unused for now (no live event stream yet);
    // the poll updates the cached per-session totals that
    // `ClientMsg::TokenUsage` reads.
    drop(mux.token_monitor.poll_due_sessions().await);
    // Snapshot visible agent state, refresh, snapshot again. The ticker's only
    // time-based effect is Working→Idle transitions; tab labels derive from
    // state and the status bar has no per-second counter, so when state is
    // unchanged the chrome is identical. A full redraw (clear + repaint) every
    // tick reads as a constant flicker, so skip it unless state actually
    // changed.
    let states_before: Vec<_> = mux.sessions.iter().map(|(id, s)| (*id, s.state)).collect();
    for (&session_id, session) in &mut mux.sessions {
        // Session::advance_status is the sole state-authoring path; the daemon
        // only reacts to the resulting transition.
        let tick = session.advance_status(rule_registry, now);
        if let Some(transition) = tick.transition {
            // Flap-rate telemetry: every public transition is logged with the
            // deciding evidence so a regression (an agent update breaking a
            // pack) shows up as a burst.
            crate::clog!(
                "agent-status: session {session_id} {} -> {} (winner={:?})",
                transition.previous.label(),
                transition.effective.label(),
                transition.winner
            );
        }
        if tick.stuck {
            crate::clog!(
                "status.stuck: session {session_id} demoted to unknown — \
                 working claimed with no output/CPU/children past the watchdog window"
            );
        }
    }
    // Seen/ack: the focused pane is being reviewed, so it must never linger on
    // `done`. Acknowledge it each tick (idempotent — only done→idle changes
    // anything), which records the seen revision.
    if let Some(focused) = mux.active_focused_id()
        && let Some(session) = mux.sessions.get_mut(&focused)
        && let Some(effective) = session.status.acknowledge()
    {
        session.state = effective;
    }
    let states_after: Vec<_> = mux.sessions.iter().map(|(id, s)| (*id, s.state)).collect();
    if mux.expire_dialog_copy_feedback(Instant::now()) {
        mux.invalidate(dialog_change_redraw_reason());
        return;
    }
    if mux.expire_selection_copy_feedback(Instant::now()) {
        mux.invalidate(selection_change_redraw_reason());
        return;
    }
    if mux.expire_clipboard_image_notice(Instant::now()) {
        mux.invalidate(status_change_redraw_reason());
        return;
    }
    if mux.refresh_open_usage_dialog_from_cache() {
        mux.invalidate(dialog_change_redraw_reason());
        return;
    }
    // A modal owns the whole screen behind an opaque backdrop; repainting the
    // status/branch chrome here would draw it back over the fill, so skip the
    // chrome frame while a dialog is open.
    if mux.dialog_open() {
        return;
    }
    if states_before == states_after {
        return;
    }
    mux.refresh_tab_labels();
    mux.invalidate(status_change_redraw_reason());
}

fn screen_detection_disabled_message(error: &anyhow::Error) -> String {
    format!("Agent status screen detection is off: {error:#}")
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
#[allow(
    clippy::too_many_lines,
    reason = "Top-level daemon entry point: spawns the event loop, the attach \
              socket acceptor, and the input parser in sequence. Each stage has \
              its own focused init + handoff. Body extraction follows the same \
              deferred-parallel-pass plan as the launch fns — the inline shape \
              preserves captured-runtime state across stages."
)]
#[allow(
    clippy::cognitive_complexity,
    reason = "Same justification as the too_many_lines allow: daemon entry point \
              branching tracks the spawn → accept → input-parser init sequence, \
              not algorithmic complexity."
)]
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

    // OTLP export for this session — no-op unless the host injected an
    // endpoint. Installs the tracing subscriber the clog!/cdebug! bridge and
    // the session-anchor span feed into; the guard flushes on daemon exit.
    let _otlp_flush = crate::telemetry::init();
    // Initialise the capsule log after OTLP so the logger can use a single
    // durable sink: OTLP when active, `multiplexer.log` otherwise.
    crate::logging::init();
    let _live_dhat_profiler = crate::alloc_telemetry::init_from_env();
    crate::debug_panic::panic_if_requested_from_env();
    crate::clog!(
        "daemon start: rows={rows} cols={cols} initial_agent={initial_agent:?} workdir={}",
        launch_config.workdir.as_str()
    );

    let initial_spawn =
        initial_spawn_request(&initial_agent, launch_config.initial_provider.as_ref());
    let mut mux = Multiplexer::new(rows, cols, launch_config)?;
    start_git_context_watcher(mux.workdir.clone(), mux.event_tx.clone());
    // Defer the first pane until the first attach Hello has supplied
    // real outer-terminal dimensions. Later panes already spawn after
    // attach-time resize; routing the first pane through the same
    // path removes first-tab-only scrollback/chrome differences.
    let mut pending_initial_spawn = Some(initial_spawn);

    let mut new_clients = socket::start_listener()?;
    // Screen rule packs: the universal detector. Loaded once; the embedded
    // packs are validated, so a load failure means a broken build — log and
    // run without screen evidence rather than killing the daemon.
    let rule_registry = match RulePackRegistry::bundled() {
        Ok(registry) => Some(registry),
        Err(e) => {
            crate::clog!("agent-status: rule packs failed to load, screen detection off: {e:#}");
            mux.open_spawn_failure_dialog(screen_detection_disabled_message(&e));
            None
        }
    };
    let mut branch_context_ticker = interval(GIT_BRANCH_CONTEXT_POLL_INTERVAL);
    let mut state_ticker = interval(STATE_TICK_INTERVAL);
    let mut usage_account_ticker = interval(USAGE_ACCOUNT_REFRESH_POLL_INTERVAL);
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
    let (control_tx, mut control_rx) = mpsc::unbounded_channel::<ControlRequest>();

    // Resolve the operator's escape-time once at startup; the value
    // cannot change after daemon launch, so per-iteration env reads
    // would be wasted syscalls. A present-but-unparseable env var
    // emits a debug line so the operator sees their config rejected
    // rather than silently falling back to the default.
    let escape_time = match std::env::var(ENV_ESCAPE_TIME) {
        Ok(raw) => {
            if let Ok(ms) = raw.parse::<u64>() {
                Duration::from_millis(ms)
            } else {
                crate::clog!(
                    "{ENV_ESCAPE_TIME}={raw:?} ignored (not a positive integer); using default {} ms",
                    DEFAULT_ESCAPE_TIME.as_millis()
                );
                DEFAULT_ESCAPE_TIME
            }
        }
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
    // Event-driven composition with a cadence cap (§3.10): compose
    // immediately when the last frame is older than the cap, otherwise
    // schedule at the cap. Latency is no longer floored at a fixed tick —
    // the first event after an idle gap paints at once, and bursts coalesce
    // to one frame per cap interval. Atomicity comes from the writer's
    // `?2026` brackets, not from pacing.
    let mut last_frame_at: Option<tokio::time::Instant> = None;
    loop {
        // The dirty-exit modal's keep/discard rows set `exit_request`; record
        // the operator's choice for the host, then drain and exit.
        if let Some(action) = mux.exit_request.take() {
            if let Err(error) = crate::exit_assess::write_exit_action(action) {
                // The operator explicitly chose keep/discard. Draining without
                // writing the file would lose their choice and silently apply
                // the wrong host cleanup. Log to stderr (operator-visible) and
                // retry next loop iteration instead of draining.
                crate::output::stderr_line(format_args!(
                    "[daemon] exit: failed to write exit-action file, retrying: {error}"
                ));
                mux.exit_request = Some(action);
            } else {
                drain_and_exit(&mut mux).await;
                return Ok(());
            }
        }
        if mux.input_parser.esc_pending() {
            if esc_deadline.is_none() {
                esc_deadline = Some(tokio::time::Instant::now() + escape_time);
            }
        } else {
            esc_deadline = None;
        }
        let render_deadline: Option<tokio::time::Instant> =
            if mux.has_pending_render() || mux.client.has_out_of_band() {
                Some(
                    last_frame_at.map_or_else(tokio::time::Instant::now, |last| {
                        (last + RENDER_TICK_INTERVAL).max(tokio::time::Instant::now())
                    }),
                )
            } else {
                None
            };
        tokio::select! {
            biased;

            _ = sigterm.recv() => {
                detach_client(&mut mux).await;
                cleanup_clipboard_run_dir();
                return Ok(());
            }
            _ = sigint.recv() => {
                detach_client(&mut mux).await;
                cleanup_clipboard_run_dir();
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
                let control_tx = control_tx.clone();
                tokio::spawn(perform_handshake(
                    stream,
                    client_permit,
                    handshake_tx,
                    control_tx,
                ));
            }

            Some(request) = control_rx.recv() => {
                // `jackin-exec` is the one control message with a deferred reply:
                // it opens the operator credential picker and answers only after
                // confirm/cancel resolves. Every other message replies inline.
                if let ClientMsg::ExecCommand { command, args } = request.msg {
                    mux.begin_exec_picker(command, args, request.reply_tx);
                } else {
                    let reply = control_reply_for_request(&mut mux, request.msg);
                    drop(request.reply_tx.send(reply));
                }
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
                crate::cdebug!("resize-event: source=attach rows={rows} cols={cols}");
                mux.resize(rows, cols);
                let capabilities = terminal.attach_capabilities();
                mux.pointer_shapes_supported = capabilities.pointer_shapes;
                // Attach-handshake outcome (clog tier): the triage line for
                // "agent themed wrong" reports — None means the client could
                // not read its terminal's palette and grids keep what they
                // had. `caps` (with its `sources` provenance) is logged so a
                // wrong-capability report can be traced to whichever input
                // (handshake identity, terminfo, color probe, override,
                // denylist) decided it.
                crate::clog!(
                    "attach: client terminal term={:?} colors fg={:?} bg={:?} caps={:?}",
                    terminal.term,
                    terminal.default_fg,
                    terminal.default_bg,
                    capabilities,
                );
                mux.attached_terminal = terminal;
                mux.attached_capabilities = capabilities;
                mux.apply_client_colors_to_sessions();
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
                let mut pending_spawn_failure = None;
                if let Some(request) = spawn {
                    let label = spawn_request_label(&request);
                    if let Err(err) = mux.spawn_request(request, &env) {
                        crate::clog!("attach: spawn {label} failed: {err:#}");
                        pending_spawn_failure = Some(spawn_request_failure_message(&label, &err));
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
                mux.client.attach(new_out_tx.clone());
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
                        crate::tui::terminal::client_owned_mode_state().to_vec(),
                    )),
                ));
                // A fresh client has no asserted cursor/mode state; the
                // first frame's reconciliation asserts everything explicitly.
                mux.last_asserted_client_state = None;
                if let Some(message) = pending_spawn_failure {
                    mux.open_spawn_failure_dialog(message);
                }
                mux.invalidate(first_attach_redraw_reason());
                let mut initial = crate::tui::terminal::RESET_CLEAR_HOME.to_vec();
                initial.extend(mux.compose_pending_frame());
                initial_frames.push((
                    InitialFrameKind::FirstAttach,
                    encode_server(ServerFrame::Output(initial)),
                ));
                let first_failure = initial_frames
                    .into_iter()
                    .find_map(|(kind, bytes)| new_out_tx.send(bytes).err().map(|_| kind));
                if let Some(kind) = first_failure {
                    crate::clog!(
                        "attach: receiver closed before initial frame ({}); operator's terminal will not paint",
                        kind.label()
                    );
                    mux.client.mark_dead_logged();
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
                // Coalesce consecutive Resize frames: process only the latest size
                // so a SIGWINCH storm produces one reflow instead of N full repaints.
                let frame = if let ClientFrame::Resize { .. } = &frame {
                    let mut latest = frame;
                    let mut coalesced: u32 = 0;
                    while let Ok(ClientFrame::Resize { rows, cols }) = cmd_rx.try_recv() {
                        latest = ClientFrame::Resize { rows, cols };
                        coalesced = coalesced.saturating_add(1);
                    }
                    if coalesced > 0 {
                        crate::cdebug!("resize: coalesced {coalesced} pending resize(s), using latest");
                    }
                    latest
                } else {
                    frame
                };
                handle_client_frame(&mut mux, frame).await;
                if mux.detach_requested {
                    mux.detach_requested = false;
                    detach_client(&mut mux).await;
                }
                if mux.no_live_sessions()
                    && handle_last_session_exit(&mut mux, None).await
                {
                    cleanup_clipboard_run_dir();
                    return Ok(());
                }
            }

            // Periodic state refresh: this arm intentionally sits above PTY
            // output in the biased select. A busy agent can keep event_rx
            // continuously ready; polling the ticker first preserves the 1 Hz
            // status floor while the output arm remains one-event-per-pass
            // bounded.
            _ = state_ticker.tick() => {
                handle_state_tick(&mut mux, rule_registry.as_ref()).await;
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
                            if is_focused {
                                reassert_outer_terminal_title = !drained.is_empty();
                                to_emit.extend(drained);
                            }
                        }
                        for bytes in to_emit {
                            mux.send_out_of_band(bytes);
                        }
                        if reassert_outer_terminal_title {
                            mux.last_outer_terminal_title = None;
                        }
                        // Bump the generation; the render loop coalesces
                        // bursts of PTY output into one frame per pass.
                        // Dialog-open still invalidates — the next frame
                        // paints the dialog overlay against the latest pane
                        // state, so dismiss doesn't jump.
                        mux.invalidate(FullRedrawReason::PtyOutput);
                    }
                    SessionEvent::Exited {
                        session_id,
                        mut reason,
                    } => {
                        // Only a non-clean exit carries a `reason`; skip the
                        // pane snapshot entirely on clean teardown so the grid
                        // render never runs on the common exit path. When the
                        // pane has no tail to attach (PTY never rendered, or the
                        // session was already removed), keep the base reason —
                        // dropping it would misroute a real failure into the
                        // clean-shutdown branch and swallow it.
                        if let Some(base) = reason.take() {
                            let tail = mux
                                .sessions
                                .get(&session_id)
                                .and_then(|session| session.diagnostic_tail(12));
                            reason = Some(match tail {
                                Some(tail) => {
                                    crate::clog!(
                                        "session {session_id}: final output tail:\n{tail}"
                                    );
                                    format!("{base}\nlast pane output:\n{tail}")
                                }
                                None => base,
                            });
                        }
                        // Remove the pane / tab immediately rather than
                        // leaving a stale `○ Done` placeholder behind.
                        // Matches the operator's mental model: "agent
                        // exited → its tab is gone."
                        mux.remove_exited_session(session_id);
                        mux.invalidate(session_exit_redraw_reason());
                        // When the last live session exits — whether
                        // the operator typed `/exit` in the agent or
                        // the agent crashed — there is nothing left to
                        // attach to. Tear down the container so the
                        // host cleanup path fires.
                        if mux.no_live_sessions()
                            && handle_last_session_exit(&mut mux, reason).await
                        {
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
                            mux.invalidate(status_change_redraw_reason());
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
                            mux.invalidate(status_change_redraw_reason());
                        }
                    }
                }
            }

            // Escape-time fired: the operator's `\x1b` did not get a
            // follow-up byte in time, so emit it as a bare Data event.
            // Dialogs treat it as dismiss; agents see the lone Esc.
            () = async {
                match esc_deadline {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending().await,
                }
            }, if esc_deadline.is_some() => {
                esc_deadline = None;
                let events = mux.input_parser.flush_pending_esc();
                for event in events {
                    mux.handle_input(event);
                }
            }

            // Render pass: fires the moment the deadline lapses — immediately
            // after an idle gap, or one cadence-cap after the previous frame
            // during a burst. An empty frame degenerates to an out-of-band
            // flush inside the writer, so queued OSC bytes never sit past a
            // pass.
            () = async {
                match render_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            }, if render_deadline.is_some() => {
                let frame_data = mux.compose_pending_frame();
                mux.send_frame(frame_data);
                last_frame_at = Some(tokio::time::Instant::now());
            }

            // Branch changes are directly operator-triggered (`git checkout`)
            // and should surface in chrome immediately. Keep this separate
            // from the heavier 1s state ticker so session state refreshes and
            // GitHub lookups do not need the same fast cadence.
            _ = branch_context_ticker.tick() => {
                mux.maybe_spawn_git_branch_context_lookup(Instant::now());
            }

            // Account refresh scheduler. This remains the provider-calling
            // path; Capsule renderers read the last Turso snapshot.
            _ = usage_account_ticker.tick() => {
                let now = Instant::now();
                let refreshed = mux.finish_usage_account_refresh_if_ready(now).await;
                mux.spawn_active_usage_account_refresh(now);
                if refreshed && mux.refresh_open_usage_dialog_from_cache() {
                    mux.invalidate(dialog_change_redraw_reason());
                }
            }

        }
    }
}

mod control;
pub use control::*;

#[cfg(test)]
mod tests;
