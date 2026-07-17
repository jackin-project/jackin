// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
use jackin_telemetry::ResultTelemetryExt as _;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use portable_pty::CommandBuilder;

use crate::agent_status::rules::RulePackRegistry;
use crate::attach_protocol::{
    AttachHandshake, ControlRequest, ControlResponse, detach_attached_task, detach_client,
    drain_and_exit, drain_and_exit_with_reason, handle_attach_client_with_handshake,
    initial_spawn_request, perform_handshake, spawn_request_label,
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

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;
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
use jackin_core::Agent;
use jackin_core::{Clock, SessionId, SystemClock};
use jackin_protocol::control::{ClientMsg, ServerMsg};

// Presentation Multiplexer impls live under `src/tui/daemon/` (TUI source
// location rule) but remain daemon submodules so `impl Multiplexer` and
// `pub(super)` stay valid. `#[path]` is intentional — not a second module tree.
#[path = "tui/daemon/compositor.rs"]
mod compositor;
mod context_mgmt;
mod control_reply;
#[path = "tui/daemon/dialog_mgmt.rs"]
mod dialog_mgmt;
mod file_export;
#[path = "tui/daemon/input_dispatch.rs"]
mod input_dispatch;
#[path = "tui/daemon/mouse_input.rs"]
mod mouse_input;
mod multiplexer_utils;
#[path = "tui/daemon/pane_layout.rs"]
mod pane_layout;
mod ports;
mod resource_metrics;
mod session_lifecycle;
mod subsystems;

use control_reply::PendingExecReply;

fn session_display_title(session: &Session) -> String {
    pane_display_title(session.title(), session.cwd(), &session.label)
}

struct SessionLaunch {
    label: String,
    cmd: CommandBuilder,
}

// ── Owned subsystems (plan 017) ────────────────────────────────────────────

/// Session map, tabs, and codename assignment.
pub(super) struct SessionSupervisor {
    pub(crate) sessions: SessionRegistry,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active_tab: usize,
    pub(crate) codename_live: HashSet<String>,
    pub(crate) codename_retired: HashSet<String>,
    pub(crate) agent_history: Vec<AgentRecord>,
    pub(crate) wordlist_offset: usize,
}

impl SessionSupervisor {
    pub(crate) fn retire_codename(&mut self, codename: &str, now: DateTime<Utc>) {
        self.codename_live.remove(codename);
        self.codename_retired.insert(codename.to_owned());
        if let Some(record) = self
            .agent_history
            .iter_mut()
            .rev()
            .find(|record| record.codename == codename)
        {
            record.exited_at = Some(now);
        }
    }
}

#[derive(Default)]
pub(crate) struct SessionRegistry(HashMap<SessionId, Session>);

impl SessionRegistry {
    pub(crate) fn get(&self, id: u64) -> Option<&Session> {
        SessionId::new(id).ok().and_then(|id| self.0.get(&id))
    }

    pub(crate) fn get_mut(&mut self, id: u64) -> Option<&mut Session> {
        SessionId::new(id).ok().and_then(|id| self.0.get_mut(&id))
    }

    pub(crate) fn insert(&mut self, id: u64, session: Session) -> Option<Session> {
        let id = SessionId::new(id).ok()?;
        self.0.insert(id, session)
    }

    pub(crate) fn remove(&mut self, id: u64) -> Option<Session> {
        SessionId::new(id).ok().and_then(|id| self.0.remove(&id))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (u64, &Session)> {
        self.0.iter().map(|(id, session)| (id.get(), session))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (u64, &mut Session)> {
        self.0.iter_mut().map(|(id, session)| (id.get(), session))
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Session> {
        self.0.values()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut Session> {
        self.0.values_mut()
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = (u64, Session)> + '_ {
        self.0.drain().map(|(id, session)| (id.get(), session))
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Single active attach client + terminal identity.
pub(super) struct ClientRegistry {
    pub(crate) client: crate::client_writer::ClientWriter,
    pub(crate) attached_task: Option<tokio::task::JoinHandle<()>>,
    pub(crate) detach_requested: bool,
    pub(crate) attached_terminal: ClientTerminal,
    pub(crate) attached_capabilities: AttachCapabilities,
    pub(crate) pointer_shape: PointerShape,
    pub(crate) pointer_shapes_supported: bool,
    pub(crate) last_outer_terminal_title: Option<String>,
}

impl ClientRegistry {
    pub(crate) fn has_attached_client(&self) -> bool {
        self.attached_task.is_some()
    }
}

/// Status bar chrome.
pub(super) struct StatusState {
    pub(crate) status_bar: StatusBar,
}

/// Text selection + clipboard image paste state.
pub(super) struct ClipboardState {
    pub(crate) selection: Option<SelectionState>,
    pub(crate) pending_selection: Option<SelectionState>,
    pub(crate) last_pane_press: Option<mouse_input::PanePress>,
    pub(crate) selection_copied: bool,
    pub(crate) selection_copy_feedback_deadline: Option<Instant>,
    pub(crate) clipboard_image_notice: Option<String>,
    pub(crate) clipboard_image_notice_deadline: Option<Instant>,
    pub(crate) clipboard_image_transfers: ClipboardImageTransfers,
    pub(crate) clipboard_image_insert_mode: ClipboardImageInsertMode,
    pub(crate) attach_control_operations: HashMap<u64, PendingAttachControl>,
    pub(crate) dialog_copy_feedback_deadline: Option<Instant>,
}

pub(super) struct PendingAttachControl {
    pub(crate) request_id: u64,
    pub(crate) context: jackin_protocol::TelemetryContext,
    pub(crate) operation: Option<jackin_telemetry::operation::OperationGuard>,
}

/// Git branch + PR watch cache.
pub(super) struct PrWatch {
    pub(crate) pull_request_context_branch: Option<BranchName>,
    pub(crate) pull_request_context_head: Option<Oid>,
    pub(crate) pull_request_context: Option<Arc<PullRequestInfo>>,
    pub(crate) git_branch_lookup: LookupState,
    pub(crate) pull_request_lookup: LookupState,
    pub(crate) pull_request_context_cache: HashMap<BranchName, PullRequestContextCacheEntry>,
}

/// Usage/quota cache and token monitor.
pub(super) struct UsageState {
    pub(crate) usage_cache: UsageCache,
    pub(crate) token_monitor: TokenMonitor,
    pub(crate) pending_usage_refresh: Option<crate::usage::UsageRefreshTarget>,
    pub(crate) usage_refresh_task: Option<tokio::task::JoinHandle<UsageCache>>,
}

/// Dialog stack, control replies, session event channel.
pub(super) struct ControlRouting {
    pub(crate) dialog_stack: Vec<Dialog>,
    pub(crate) pending_exec_reply: Option<PendingExecReply>,
    pub(crate) exit_request: Option<jackin_protocol::ExitAction>,
    pub(crate) input_parser: InputParser,
    pub(crate) event_tx: mpsc::UnboundedSender<SessionEvent>,
    pub(crate) event_rx: mpsc::UnboundedReceiver<SessionEvent>,
}

fn handle_control_request(mux: &mut Multiplexer, request: ControlRequest) {
    let Some(operation) = control_server_operation(&request.ctx, &request.msg) else {
        drop(request.reply_tx.send(ControlResponse {
            msg: ServerMsg::Unknown,
            operation: None,
            outcome: jackin_telemetry::schema::enums::OutcomeValue::Failure,
            error_type: Some(RPC_ERROR),
        }));
        return;
    };
    if let ClientMsg::ExecCommand { command, args } = request.msg {
        mux.begin_exec_picker(command, args, request.reply_tx, operation);
        return;
    }
    let reply = if let Some(guard) = operation.as_ref() {
        guard
            .span()
            .in_scope(|| control_reply_for_request(mux, request.msg.clone()))
    } else {
        control_reply_for_request(mux, request.msg.clone())
    };
    let unknown = matches!(reply, ServerMsg::Unknown);
    let response = ControlResponse {
        msg: reply,
        operation,
        outcome: if unknown {
            jackin_telemetry::schema::enums::OutcomeValue::Failure
        } else {
            jackin_telemetry::schema::enums::OutcomeValue::Success
        },
        error_type: unknown.then_some(RPC_ERROR),
    };
    if let Err(response) = request.reply_tx.send(response) {
        response.complete_delivery_failure();
    }
}

impl ControlRouting {
    pub(crate) fn dialog_open(&self) -> bool {
        !self.dialog_stack.is_empty()
    }
}

/// Terminal geometry, frame generation, compositor caches.
pub(super) struct RenderState {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) content_rows: u16,
    pub(crate) frame_generation: u64,
    pub(crate) rendered_generation: u64,
    pub(crate) wipe_pending: Option<FullRedrawReason>,
    pub(crate) last_invalidate_reason: Option<FullRedrawReason>,
    pub(crate) last_asserted_client_state: Option<compositor::AssertedClientState>,
    pub(crate) pane_region_cache: HashMap<u64, compositor::PaneRegionCache>,
    pub(crate) hover_target: Option<HoverTarget>,
    pub(crate) link_hover_url: Option<String>,
    pub(crate) tab_bar_focused: bool,
    pub(crate) drag: Option<DragState>,
    pub(crate) last_tab_click: Option<(usize, Instant)>,
    pub(crate) ratatui_terminal: ratatui::Terminal<crate::tui::socket_backend::SocketBackend>,
    pub(crate) terminal_row_arena: jackin_term::RowArena,
}

/// Static launch configuration at daemon construction.
pub(super) struct LaunchEnv {
    pub(crate) available_agents: Vec<String>,
    pub(crate) launch_config: CapsuleConfig,
    pub(crate) env_passthrough: Vec<(String, String)>,
    pub(crate) workdir: PathBuf,
    pub(crate) workdir_context: WorkdirContext,
    pub(crate) provider_keys: BTreeMap<jackin_protocol::Provider, String>,
}

#[expect(
    missing_debug_implementations,
    reason = "Multiplexer owns PTY sessions and render/input state; targeted debug logs expose the useful fields."
)]
pub struct Multiplexer {
    pub(crate) session_supervisor: SessionSupervisor,
    pub(crate) client_registry: ClientRegistry,
    pub(crate) status: StatusState,
    pub(crate) clipboard: ClipboardState,
    pub(crate) pr_watch: PrWatch,
    pub(crate) usage: UsageState,
    pub(crate) control: ControlRouting,
    pub(crate) render: RenderState,
    pub(crate) launch_env: LaunchEnv,
    pub(crate) resource_metrics: resource_metrics::ResourceMetricsSampler,
    pub(crate) widget_focus: jackin_telemetry::ui::WidgetFocusTracker,
    /// Wall/monotonic clock for lifecycle timestamps (plan 025). Tests inject
    /// [`jackin_core::ManualClock`] via [`Multiplexer::with_clock`].
    pub(crate) clock: Arc<dyn Clock>,
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
pub(crate) struct LookupState {
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
pub(crate) struct PullRequestContextCacheEntry {
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
        Self::with_clock(rows, cols, launch_config, Arc::new(SystemClock))
    }

    /// Construct a multiplexer with an injected clock (tests / deterministic
    /// lifecycle timestamps).
    pub fn with_clock(
        rows: u16,
        cols: u16,
        launch_config: CapsuleConfig,
        clock: Arc<dyn Clock>,
    ) -> io::Result<Self> {
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
        let status_identity = crate::container_context::resolve_status_identity();
        let mut status_bar = StatusBar::new_with_role_labels(
            launch_config.role.clone(),
            status_identity.container_name,
            status_identity.instance_id,
        );
        status_bar.set_prefix_enabled(input_parser.prefix_enabled());

        let ratatui_terminal =
            ratatui::Terminal::new(crate::tui::socket_backend::SocketBackend::new(cols, rows))?;

        let mut mux = Self {
            session_supervisor: SessionSupervisor {
                sessions: SessionRegistry::default(),
                tabs: Vec::new(),
                active_tab: 0,
                codename_live: HashSet::new(),
                codename_retired: HashSet::new(),
                agent_history: Vec::new(),
                wordlist_offset: {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(42, |d| d.subsec_nanos() as usize)
                },
            },
            client_registry: ClientRegistry {
                client: crate::client_writer::ClientWriter::default(),
                attached_task: None,
                detach_requested: false,
                attached_terminal: ClientTerminal::default(),
                attached_capabilities: AttachCapabilities::default(),
                pointer_shape: PointerShape::Default,
                pointer_shapes_supported: false,
                last_outer_terminal_title: None,
            },
            status: StatusState { status_bar },
            clipboard: ClipboardState {
                selection: None,
                pending_selection: None,
                last_pane_press: None,
                selection_copied: false,
                selection_copy_feedback_deadline: None,
                clipboard_image_notice: None,
                clipboard_image_notice_deadline: None,
                clipboard_image_transfers: ClipboardImageTransfers::default(),
                clipboard_image_insert_mode: ClipboardImageInsertMode::PastePath,
                attach_control_operations: HashMap::new(),
                dialog_copy_feedback_deadline: None,
            },
            pr_watch: PrWatch {
                pull_request_context_branch: None,
                pull_request_context_head: None,
                pull_request_context: None,
                git_branch_lookup: LookupState::default(),
                pull_request_lookup: LookupState::default(),
                pull_request_context_cache: HashMap::new(),
            },
            usage: UsageState {
                usage_cache: UsageCache::default(),
                token_monitor: TokenMonitor::new(),
                pending_usage_refresh: None,
                usage_refresh_task: None,
            },
            control: ControlRouting {
                dialog_stack: Vec::new(),
                pending_exec_reply: None,
                exit_request: None,
                input_parser,
                event_tx,
                event_rx,
            },
            render: RenderState {
                term_rows: rows,
                term_cols: cols,
                content_rows,
                frame_generation: 0,
                rendered_generation: 0,
                wipe_pending: None,
                last_invalidate_reason: None,
                last_asserted_client_state: None,
                pane_region_cache: HashMap::new(),
                hover_target: None,
                link_hover_url: None,
                tab_bar_focused: false,
                drag: None,
                last_tab_click: None,
                ratatui_terminal,
                terminal_row_arena: jackin_term::RowArena::default(),
            },
            launch_env: LaunchEnv {
                available_agents: agents,
                launch_config,
                env_passthrough,
                workdir,
                workdir_context,
                provider_keys,
            },
            resource_metrics: resource_metrics::ResourceMetricsSampler::default(),
            widget_focus: jackin_telemetry::ui::WidgetFocusTracker::default(),
            clock,
        };
        mux.sync_widget_focus();
        Ok(mux)
    }

    /// Wall-clock `DateTime<Utc>` from the injected clock.
    pub(crate) fn wall_now_utc(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from(self.clock.now_system())
    }

    /// Send a composed frame to the attached client through the single
    /// writer. Queued out-of-band bytes flush ahead of the bracketed frame.
    fn send_frame(&mut self, bytes: Vec<u8>) {
        self.client_registry.client.write_frame(bytes);
    }

    /// Queue bytes that are not cell content (OSC passthrough, clipboard,
    /// pointer shapes, mode prefaces); they flush at the next frame boundary.
    pub(crate) fn send_out_of_band(&mut self, bytes: Vec<u8>) {
        self.client_registry.client.enqueue_out_of_band(bytes);
    }

    /// Send a typed attach protocol frame that is not terminal output.
    fn send_protocol_frame(&mut self, frame: ServerFrame) {
        self.client_registry.client.send_protocol_frame(frame);
    }

    fn request_clipboard_image_from_text_path(&mut self) {
        self.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        self.send_protocol_frame(ServerFrame::HostStageImageFromClipboardPath);
    }

    fn request_clipboard_image_paste(&mut self) {
        self.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        self.send_protocol_frame(ServerFrame::HostPasteImageFromClipboard);
    }

    fn request_clipboard_image_stage_only(&mut self) {
        self.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;
        self.send_protocol_frame(ServerFrame::HostStageImageFromClipboard);
    }

    fn stage_clipboard_image_response(
        &mut self,
        image: jackin_protocol::attach::ClipboardImage,
    ) -> bool {
        self.stage_clipboard_image_response_with(image, stage_clipboard_image)
    }

    fn stage_clipboard_image_response_with<F>(
        &mut self,
        image: jackin_protocol::attach::ClipboardImage,
        stage: F,
    ) -> bool
    where
        F: FnOnce(&jackin_protocol::attach::ClipboardImage) -> Result<PathBuf>,
    {
        let insert_mode = std::mem::take(&mut self.clipboard.clipboard_image_insert_mode);
        match stage(&image) {
            Ok(path) => {
                let path = path.to_string_lossy();
                let bytes = image.bytes.len();
                if insert_mode == ClipboardImageInsertMode::StageOnly {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes)"
                    ));
                } else if self.dialog_captures_input() {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes; dialog focused; not pasted)"
                    ));
                } else if self.paste_text_to_focused_pane(path.as_bytes()) {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes)"
                    ));
                } else {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes; no writable focused pane; not pasted)"
                    ));
                }
                true
            }
            Err(err) => {
                let _error = jackin_telemetry::record_error(RPC_ERROR);
                self.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
                false
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardImageInsertMode {
    #[default]
    PastePath,
    StageOnly,
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
    use ports::{ExitDisposition, PORTS, PersistencePort};
    if PORTS.last_session_exit(&mux.control) == ExitDisposition::Defer {
        return false;
    }
    match crate::exit_assess::decide_exit(mux.launch_env.config()).await {
        crate::exit_assess::ExitDecision::Drain => {
            drain_and_exit_with_reason(mux, reason).await;
            true
        }
        crate::exit_assess::ExitDecision::DrainWithAction(action) => {
            // Policy keep/discard: record the action for the host, no prompt.
            // Write failure is logged but does not block exit — a configured
            // policy path cannot stall indefinitely waiting for a broken fs.
            if let Err(error) = crate::exit_assess::write_exit_action(action) {
                let _warning = jackin_telemetry::record_recovered_degradation();
                crate::output::stderr_line(format_args!(
                    "[daemon] exit: failed to write exit-action file, policy will not be applied: {error}"
                ));
            }
            drain_and_exit_with_reason(mux, reason).await;
            true
        }
        crate::exit_assess::ExitDecision::ShowModal(repos) => {
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

fn emit_agent_state_change(
    session: &Session,
    transition: &crate::session::StatusTransition,
    stuck: bool,
) {
    use jackin_telemetry::{Attr, FieldSet, Value};

    let Some(metric_attrs) = agent_status_metric_attrs(session, transition.effective) else {
        return;
    };
    let event_attrs = [
        metric_attrs[0],
        metric_attrs[1],
        metric_attrs[2],
        metric_attrs[3],
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATUS_STUCK,
            value: Value::Bool(stuck),
        },
    ];
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::AGENT_STATE_CHANGED,
        FieldSet::new(&event_attrs, None),
    );
    let _transition_result =
        jackin_telemetry::counter(&jackin_telemetry::metric::AGENT_STATE_TRANSITIONS)
            .add(1, &metric_attrs);
    if stuck {
        record_agent_stuck(&metric_attrs);
    }
}

fn agent_status_metric_attrs(
    session: &Session,
    state: crate::protocol::AgentState,
) -> Option<[jackin_telemetry::Attr<'_>; 4]> {
    use jackin_telemetry::{Attr, Value};

    let agent = session.agent.as_deref()?;
    let source = match session.status.report(None).source {
        jackin_protocol::agent_status::AgentStatusSource::None => "none",
        jackin_protocol::agent_status::AgentStatusSource::VisibleScreen => "visible_screen",
        jackin_protocol::agent_status::AgentStatusSource::ShellIntegration => "shell_integration",
        jackin_protocol::agent_status::AgentStatusSource::ForegroundProcess => "foreground_process",
        jackin_protocol::agent_status::AgentStatusSource::Reported { .. } => "reported",
    };
    let confidence = match session.status.confidence {
        jackin_protocol::agent_status::AgentStatusConfidence::Unknown => "unknown",
        jackin_protocol::agent_status::AgentStatusConfidence::Weak => "weak",
        jackin_protocol::agent_status::AgentStatusConfidence::Strong => "strong",
        jackin_protocol::agent_status::AgentStatusConfidence::Authoritative => "authoritative",
    };
    Some([
        Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        },
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATE,
            value: Value::Str(state.label()),
        },
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATUS_SOURCE,
            value: Value::Str(source),
        },
        Attr {
            key: jackin_telemetry::schema::attrs::AGENT_STATUS_CONFIDENCE,
            value: Value::Str(confidence),
        },
    ])
}

fn record_agent_stuck(attrs: &[jackin_telemetry::Attr<'_>]) {
    let _stuck_result =
        jackin_telemetry::counter(&jackin_telemetry::metric::AGENT_STATE_STUCK).add(1, attrs);
}

fn agent_status_cycle_attrs() -> [jackin_telemetry::Attr<'static>; 1] {
    [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::BACKGROUND_CYCLE_NAME,
        value: jackin_telemetry::Value::Str(
            jackin_telemetry::schema::enums::BackgroundCycleName::AgentStatus.as_str(),
        ),
    }]
}

fn record_skipped_agent_status() {
    let attrs = [
        agent_status_cycle_attrs()[0],
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::OutcomeValue::Skip.as_str(),
            ),
        },
    ];
    let _metric =
        jackin_telemetry::counter(&jackin_telemetry::metric::BACKGROUND_CYCLES).add(1, &attrs);
}

fn record_agent_status_tick(session: &Session, tick: crate::session::StatusTick) {
    if tick.transition.is_none() && !tick.stuck {
        record_skipped_agent_status();
        return;
    }
    let cycle = jackin_telemetry::autonomous_root_operation(
        &jackin_telemetry::operation::BACKGROUND_CYCLE,
        &agent_status_cycle_attrs(),
    )
    .ok();
    let record_result = || {
        if let Some(transition) = tick.transition {
            emit_agent_state_change(session, &transition, tick.stuck);
        } else if let Some(attrs) = agent_status_metric_attrs(session, session.state) {
            record_agent_stuck(&attrs);
        }
    };
    if let Some(cycle) = cycle {
        cycle.span().in_scope(record_result);
        cycle.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    } else {
        record_result();
    }
}

fn provider_probe_attrs() -> [jackin_telemetry::Attr<'static>; 1] {
    [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::BACKGROUND_CYCLE_NAME,
        value: jackin_telemetry::Value::Str(
            jackin_telemetry::schema::enums::BackgroundCycleName::ProviderProbe.as_str(),
        ),
    }]
}

fn record_skipped_provider_probe() {
    let attrs = [
        provider_probe_attrs()[0],
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::OutcomeValue::Skip.as_str(),
            ),
        },
    ];
    let _metric =
        jackin_telemetry::counter(&jackin_telemetry::metric::BACKGROUND_CYCLES).add(1, &attrs);
}

async fn handle_state_tick(mux: &mut Multiplexer, rule_registry: Option<&RulePackRegistry>) {
    mux.record_resource_metrics().await;
    mux.maybe_spawn_pull_request_context_lookup(Instant::now());
    // Reap idle clipboard-image transfers and surface a notice. Must NOT
    // short-circuit the tick: agent-state advancement below is the 1 Hz floor —
    // every session re-evaluates each tick — and a clipboard reap is an
    // orthogonal concern that must not freeze it. The `invalidate` guarantees
    // the notice repaints even if no agent state changed this tick (otherwise
    // the no-change return below would leave the frame clean and the notice
    // never painted).
    let stale_image_transfer_ids = mux
        .clipboard
        .clipboard_image_transfers
        .abort_idle_ids_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    let stale_image_transfers = stale_image_transfer_ids.len();
    for transfer_id in stale_image_transfer_ids {
        if let Some(pending) = mux.clipboard.attach_control_operations.remove(&transfer_id) {
            send_attach_control_response(
                mux,
                pending.request_id,
                jackin_protocol::attach::AttachControlResult::Rejected,
                pending.operation,
            );
        }
    }
    if stale_image_transfers > 0 {
        mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
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
        .session_supervisor
        .sessions
        .iter()
        .filter_map(|(id, s)| Some((id, Agent::from_slug(s.agent.as_deref()?)?)))
        .collect();
    mux.usage.token_monitor.reconcile_sessions(&token_sessions);
    // Returned changed-id list is unused for now (no live event stream yet);
    // the poll updates the cached per-session totals that
    // `ClientMsg::TokenUsage` reads.
    if mux.usage.token_monitor.due_session_count() == 0 {
        record_skipped_provider_probe();
    } else {
        let cycle = jackin_telemetry::autonomous_root_operation(
            &jackin_telemetry::operation::BACKGROUND_CYCLE,
            &provider_probe_attrs(),
        )
        .ok();
        let report = mux.usage.token_monitor.poll_due_sessions().await;
        if let Some(cycle) = cycle {
            if report.degraded == 0 {
                cycle.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
            } else {
                cycle.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Success,
                    Some(jackin_telemetry::schema::enums::ErrorType::RecoveredDegradation),
                );
            }
        }
    }
    // Snapshot visible agent state, refresh, snapshot again. The ticker's only
    // time-based effect is Working→Idle transitions; tab labels derive from
    // state and the status bar has no per-second counter, so when state is
    // unchanged the chrome is identical. A full redraw (clear + repaint) every
    // tick reads as a constant flicker, so skip it unless state actually
    // changed.
    let states_before: Vec<_> = mux
        .session_supervisor
        .sessions
        .iter()
        .map(|(id, s)| (id, s.state))
        .collect();
    for (_, session) in mux.session_supervisor.sessions.iter_mut() {
        // Session::advance_status is the sole state-authoring path; the daemon
        // only reacts to the resulting transition.
        let tick = session.advance_status(rule_registry, now);
        record_agent_status_tick(session, tick);
    }
    // Seen/ack: the focused pane is being reviewed, so it must never linger on
    // `done`. Acknowledge it each tick (idempotent — only done→idle changes
    // anything), which records the seen revision.
    if let Some(focused) = mux.active_focused_id()
        && let Some(session) = mux.session_supervisor.sessions.get_mut(focused)
        && let Some(effective) = session.status.acknowledge()
    {
        session.state = effective;
    }
    let states_after: Vec<_> = mux
        .session_supervisor
        .sessions
        .iter()
        .map(|(id, s)| (id, s.state))
        .collect();
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

fn configured_escape_time() -> Duration {
    let Ok(raw) = std::env::var(ENV_ESCAPE_TIME) else {
        return DEFAULT_ESCAPE_TIME;
    };
    let Ok(ms) = raw.parse::<u64>() else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        return DEFAULT_ESCAPE_TIME;
    };
    Duration::from_millis(ms)
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
#[expect(
    clippy::too_many_lines,
    reason = "Top-level daemon entry point: spawns the event loop, the attach \
              socket acceptor, and the input parser in sequence. Each stage has \
              its own focused init + handoff. Body extraction follows the same \
              deferred-parallel-pass plan as the launch fns — the inline shape \
              preserves captured-runtime state across stages."
)]
pub async fn run_daemon(
    initial_agent: String,
    launch_config: CapsuleConfig,
    telemetry: &mut crate::telemetry::FlushGuard,
) -> Result<()> {
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

    // Resolve Capsule telemetry detail and install panic handling after OTLP so
    // crash events use the active governed exporter.
    crate::logging::init();
    let _live_dhat_profiler = crate::alloc_telemetry::init_from_env();
    crate::debug_panic::panic_if_requested_from_env();

    let initial_spawn =
        initial_spawn_request(&initial_agent, launch_config.initial_provider.as_ref());
    let mut mux = Multiplexer::new(rows, cols, launch_config)?;
    start_git_context_watcher(mux.launch_env.workdir.clone(), mux.control.event_tx.clone());
    // Defer the first pane until the first attach Hello has supplied
    // real outer-terminal dimensions. Later panes already spawn after
    // attach-time resize; routing the first pane through the same
    // path removes first-tab-only scrollback/chrome differences.
    let mut pending_initial_spawn = Some(initial_spawn);

    let mut new_clients = socket::start_listener()?;
    telemetry.listener_ready();
    // Screen rule packs: the universal detector. Loaded once; the embedded
    // packs are validated, so a load failure means a broken build — log and
    // run without screen evidence rather than killing the daemon.
    let rule_registry = match RulePackRegistry::bundled() {
        Ok(registry) => Some(registry),
        Err(e) => {
            let _warning = jackin_telemetry::record_recovered_degradation();
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
    let escape_time = configured_escape_time();

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
        if let Some(action) = mux.control.exit_request.take() {
            if let Err(error) = crate::exit_assess::write_exit_action(action) {
                let _warning = jackin_telemetry::record_recovered_degradation();
                // The operator explicitly chose keep/discard. Draining without
                // writing the file would lose their choice and silently apply
                // the wrong host cleanup. Log to stderr (operator-visible) and
                // retry next loop iteration instead of draining.
                crate::output::stderr_line(format_args!(
                    "[daemon] exit: failed to write exit-action file, retrying: {error}"
                ));
                mux.control.exit_request = Some(action);
            } else {
                drain_and_exit(&mut mux).await;
                return Ok(());
            }
        }
        if mux.control.input_parser.esc_pending() {
            if esc_deadline.is_none() {
                esc_deadline = Some(tokio::time::Instant::now() + escape_time);
            }
        } else {
            esc_deadline = None;
        }
        let render_deadline: Option<tokio::time::Instant> =
            if mux.has_pending_render() || mux.client_registry.client.has_out_of_band() {
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
                jackin_telemetry::spawn::spawn_detached_with_completion(
                    &jackin_telemetry::operation::CONNECTION_ATTEMPT,
                    perform_handshake(stream, client_permit, handshake_tx, control_tx),
                );
            }

            Some(request) = control_rx.recv() => handle_control_request(&mut mux, request),

            // Validated attach handshake from the spawned handshake task.
            Some(ready) = handshake_rx.recv() => {
                let AttachHandshake {
                    stream,
                    rows,
                    cols,
                    spawn,
                    env,
                    terminal,
                    context,
                    focus_session,
                    client_permit,
                } = ready;
                let extracted = context
                    .as_ref()
                    .map_or(jackin_telemetry::propagation::ExtractOutcome::LocalRoot, |ctx| {
                        jackin_telemetry::propagation::extract(ctx.as_ref())
                    });
                if matches!(
                    extracted,
                    jackin_telemetry::propagation::ExtractOutcome::RejectRequest
                ) {
                    let mut stream = stream;
                    let response = encode_server(ServerFrame::Shutdown {
                        reason: Some("invalid correlation".to_owned()),
                    });
                    drop(
                        tokio::io::AsyncWriteExt::write_all(&mut stream, &response)
                            .await
                            .record_telemetry_error(RPC_ERROR),
                    );
                    drop(client_permit);
                    continue;
                }
                let attrs = [
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
                        value: jackin_telemetry::Value::Str("jackin"),
                    },
                    jackin_telemetry::Attr {
                        key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
                        value: jackin_telemetry::Value::Str("jackin.capsule.Attach/Handshake"),
                    },
                ];
                let attach_operation = match &extracted {
                    jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
                        jackin_telemetry::operation_with_remote_parent(
                            &jackin_telemetry::operation::RPC_SERVER,
                            &attrs,
                            parent,
                        )
                    }
                    _ => jackin_telemetry::operation(
                        &jackin_telemetry::operation::RPC_SERVER,
                        &attrs,
                    ),
                }
                .ok();
                mux.resize(rows, cols);
                let capabilities = terminal.attach_capabilities();
                mux.client_registry.pointer_shapes_supported = capabilities.pointer_shapes;
                mux.client_registry.attached_terminal = terminal;
                mux.client_registry.attached_capabilities = capabilities;
                mux.apply_client_colors_to_sessions();
                mux.client_registry.pointer_shape = PointerShape::Default;
                if mux.session_supervisor.sessions.is_empty()
                    && let Some(request) = pending_initial_spawn.take()
                    && let Err(err) = mux.spawn_request(request, &[])
                {
                    if let Some(operation) = attach_operation {
                        operation.complete(
                            jackin_telemetry::schema::enums::OutcomeValue::Failure,
                            Some(RPC_ERROR),
                        );
                    }
                    return Err(err);
                }
                if let Some(target) = focus_session {
                    let _focused = mux.focus_session_globally(target);
                }
                // Honor a spawn intent from `jackin-capsule new
                // <agent>` / `jackin-capsule new` (shell). Spawn
                // failures are surfaced to the new client as an Output frame
                // after Welcome so the operator
                // sees the reason in their terminal — silently
                // landing on an empty multiplexer would otherwise be
                // indistinguishable from "no spawn requested".
                let mut pending_spawn_failure = None;
                if let Some(request) = spawn {
                    let label = spawn_request_label(&request);
                    use ports::{AttachPort, PORTS};
                    let spawn_result = PORTS
                        .prepare_session_spawn(&mux.session_supervisor)
                        .and_then(|()| mux.spawn_request(request, &env).map(|_| ()));
                    if let Err(err) = spawn_result {
                        let _warning = jackin_telemetry::record_recovered_degradation();
                        pending_spawn_failure = Some(spawn_request_failure_message(&label, &err));
                    }
                }
                // Take over from any existing attach client (INV-D1). The
                // port decides displace; the helper sends Shutdown, drains
                // briefly, then aborts the old reader task.
                use ports::{AttachPort, AttachTransition, PORTS};
                if PORTS.begin_attach(&mux.client_registry) == AttachTransition::Displace {
                    detach_attached_task(&mut mux, "takeover").await;
                    PORTS.record_detached();
                }
                PORTS.record_attached();
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
                while cmd_rx.try_recv().is_ok() {
                }
                let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                let (completion_tx, completion_rx) = mpsc::unbounded_channel();
                mux.client_registry
                    .client
                    .attach_with_completions(new_out_tx.clone(), completion_tx);
                // A send failure here means the receiver closed in a takeover
                // race during this tick; the attach boundary owns one error.
                let mut initial_frames = Vec::with_capacity(5);
                initial_frames.push(encode_server(ServerFrame::Welcome {
                    session_count: mux.session_supervisor.sessions.len() as u32,
                }));
                // Re-assert the attach-client-owned mouse/focus modes,
                // then restore the focused session's modes (bracketed
                // paste, etc.). Without this, a re-attach loses
                // bracketed-paste and the operator's clipboard arrives
                // unwrapped.
                initial_frames.push(encode_server(ServerFrame::Output(
                    crate::tui::terminal::client_owned_mode_state().to_vec(),
                )));
                // A fresh client has no asserted cursor/mode state; the
                // first frame's reconciliation asserts everything explicitly.
                mux.render.last_asserted_client_state = None;
                if let Some(message) = pending_spawn_failure {
                    mux.open_spawn_failure_dialog(message);
                }
                mux.invalidate(first_attach_redraw_reason());
                let mut initial = crate::tui::terminal::RESET_CLEAR_HOME.to_vec();
                initial.extend(mux.compose_pending_frame());
                initial_frames.push(encode_server(ServerFrame::Output(initial)));
                if initial_frames
                    .into_iter()
                    .any(|bytes| new_out_tx.send(bytes).is_err())
                {
                    let _error = jackin_telemetry::record_error(
                        jackin_telemetry::schema::enums::ErrorType::RpcError,
                    );
                }
                let cmd_tx_for_task = cmd_tx.clone();
                mux.client_registry.attached_task = Some(jackin_telemetry::spawn::spawn_stream("capsule.attach", async move {
                    handle_attach_client_with_handshake(
                        stream,
                        new_out_rx,
                        completion_rx,
                        cmd_tx_for_task,
                        attach_operation,
                    )
                    .await;
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
                let (frames, _coalesced) =
                    coalesce_client_frames(frame, || cmd_rx.try_recv().ok());
                for frame in frames {
                    handle_client_frame(&mut mux, frame);
                    if mux.client_registry.detach_requested {
                        break;
                    }
                }
                if mux.client_registry.detach_requested {
                    mux.client_registry.detach_requested = false;
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
            Some(event) = mux.control.event_rx.recv() => {
                match event {
                    SessionEvent::Output { session_id, data } => {
                        let focused_id = mux.active_focused_id();
                        let is_focused = Some(session_id) == focused_id;
                        // Collect any focused-pane output into local
                        // vecs so the `&mut Session` borrow ends before
                        // `mux.send_output` (which takes `&mut Multiplexer`).
                        let mut to_emit: Vec<Vec<u8>> = Vec::new();
                        let mut reassert_outer_terminal_title = false;
                        if let Some(session) = mux.session_supervisor.sessions.get_mut(session_id) {
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
                            mux.client_registry.last_outer_terminal_title = None;
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
                                .session_supervisor.sessions
                                .get(session_id)
                                .and_then(|session| session.diagnostic_tail(12));
                            reason = Some(match tail {
                                Some(tail) => format!("{base}\nlast pane output:\n{tail}"),
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
                let events = mux.control.input_parser.flush_pending_esc();
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

pub(crate) fn control_server_operation(
    context: &jackin_protocol::TelemetryContext,
    message: &ClientMsg,
) -> Option<Option<jackin_telemetry::operation::OperationGuard>> {
    let extracted = jackin_telemetry::propagation::extract(context);
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        return None;
    }
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(message.rpc_method()),
        },
    ];
    let operation = match &extracted {
        jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
            jackin_telemetry::operation_with_remote_parent(
                &jackin_telemetry::operation::RPC_SERVER,
                &attrs,
                parent,
            )
        }
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
    }
    .ok();
    Some(operation)
}

mod control;
pub use control::*;

#[cfg(test)]
mod tests;
