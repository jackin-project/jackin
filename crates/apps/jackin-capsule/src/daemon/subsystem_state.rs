// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Owned multiplexer subsystem state: clients, clipboard, watches, render.

use std::collections::HashMap;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::clipboard::ClipboardImageTransfers;
use crate::git_context::WorkdirContext;

use crate::protocol::attach::{AttachCapabilities, ClientTerminal};

use crate::pull_request::PullRequestInfo;
use crate::session::{BranchName, Oid, SessionEvent};

use crate::token_monitor::TokenMonitor;
use crate::tui::components::dialog::Dialog;

use crate::tui::components::status_bar::StatusBar;
use crate::tui::input::InputParser;

use crate::tui::model::{DragState, HoverTarget, PointerShape};
use crate::tui::selection::SelectionState;
use crate::tui::subscriptions::PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL;

use crate::tui::update::FullRedrawReason;

use crate::usage::UsageCache;

use super::{ClipboardImageInsertMode, PendingExecReply};

/// Single active attach client + terminal identity.
pub(crate) struct ClientRegistry {
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
pub(crate) struct StatusState {
    pub(crate) status_bar: StatusBar,
}

/// Text selection + clipboard image paste state.
pub(crate) struct ClipboardState {
    pub(crate) selection: Option<SelectionState>,
    pub(crate) pending_selection: Option<SelectionState>,
    pub(crate) last_pane_press: Option<super::mouse_input::PanePress>,
    pub(crate) selection_copied: bool,
    pub(crate) selection_copy_feedback_deadline: Option<Instant>,
    pub(crate) clipboard_image_notice: Option<String>,
    pub(crate) clipboard_image_notice_deadline: Option<Instant>,
    pub(crate) clipboard_image_transfers: ClipboardImageTransfers,
    pub(crate) clipboard_image_insert_mode: ClipboardImageInsertMode,
    pub(crate) attach_control_operations: HashMap<u64, PendingAttachControl>,
    pub(crate) dialog_copy_feedback_deadline: Option<Instant>,
}

pub(crate) struct PendingAttachControl {
    pub(crate) request_id: u64,
    pub(crate) context: jackin_protocol::TelemetryContext,
    pub(crate) operation: Option<jackin_telemetry::operation::OperationGuard>,
}

/// Git branch + PR watch cache.
pub(crate) struct PrWatch {
    pub(crate) pull_request_context_branch: Option<BranchName>,
    pub(crate) pull_request_context_head: Option<Oid>,
    pub(crate) pull_request_context: Option<Arc<PullRequestInfo>>,
    pub(crate) git_branch_lookup: LookupState,
    pub(crate) pull_request_lookup: LookupState,
    pub(crate) pull_request_context_cache: HashMap<BranchName, PullRequestContextCacheEntry>,
}

/// Usage/quota cache and token monitor.
pub(crate) struct UsageState {
    pub(crate) usage_cache: UsageCache,
    pub(crate) token_monitor: TokenMonitor,
    pub(crate) pending_usage_refresh: Option<crate::usage::UsageRefreshTarget>,
    pub(crate) usage_refresh_task: Option<tokio::task::JoinHandle<Vec<BrokerUsageRefresh>>>,
}

pub(crate) struct BrokerUsageRefresh {
    pub(crate) target: crate::usage::UsageRefreshTarget,
    pub(crate) result: Result<
        jackin_protocol::usage_broker::UsageGenerationView,
        jackin_protocol::usage_broker::UsageCoordinationError,
    >,
}

/// Dialog stack, control replies, session event channel.
pub(crate) struct ControlRouting {
    pub(crate) dialog_stack: Vec<Dialog>,
    pub(crate) pending_exec_reply: Option<PendingExecReply>,
    pub(crate) exit_request: Option<jackin_protocol::ExitAction>,
    pub(crate) input_parser: InputParser,
    pub(crate) event_tx: mpsc::UnboundedSender<SessionEvent>,
    pub(crate) event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    /// Open `events` subscriptions. Empty in the common case (no host client
    /// listening), which is what keeps the emit sites free.
    pub(crate) event_subscribers: super::events::EventSubscribers,
}

impl ControlRouting {
    pub(crate) fn dialog_open(&self) -> bool {
        !self.dialog_stack.is_empty()
    }
}

/// Terminal geometry, frame generation, compositor caches.
pub(crate) struct RenderState {
    pub(crate) term_rows: u16,
    pub(crate) term_cols: u16,
    pub(crate) content_rows: u16,
    pub(crate) frame_generation: u64,
    pub(crate) rendered_generation: u64,
    pub(crate) wipe_pending: Option<FullRedrawReason>,
    pub(crate) last_invalidate_reason: Option<FullRedrawReason>,
    pub(crate) last_asserted_client_state: Option<super::compositor::AssertedClientState>,
    pub(crate) pane_region_cache: HashMap<u64, super::compositor::PaneRegionCache>,
    pub(crate) hover_target: Option<HoverTarget>,
    pub(crate) link_hover_url: Option<String>,
    pub(crate) tab_bar_focused: bool,
    pub(crate) drag: Option<DragState>,
    pub(crate) last_tab_click: Option<(usize, Instant)>,
    pub(crate) ratatui_terminal: ratatui::Terminal<crate::tui::socket_backend::SocketBackend>,
    pub(crate) terminal_row_arena: termpane::RowArena,
}

/// Static launch configuration at daemon construction.
pub(crate) struct LaunchEnv {
    pub(crate) available_instances: Vec<String>,
    pub(crate) launch_config: CapsuleConfig,
    pub(crate) agent_credentials: jackin_protocol::AgentCredentialEnv,
    pub(crate) env_passthrough: Vec<(String, String)>,
    pub(crate) workdir: PathBuf,
    pub(crate) workdir_context: WorkdirContext,
}

/// Three book-keeping fields for a background context lookup. They
/// MUST move together: `begin_spawn` bumps `request_id`, stamps
/// `last_run`, and flips `in_flight`; `invalidate_in_flight` bumps
/// `request_id` and clears `in_flight`. Open-coding any subset
/// re-opens the race where a stale response carrying an old
/// `request_id` overwrites a fresh branch's cache slot.
#[derive(Default)]
pub(crate) struct LookupState {
    pub(crate) request_id: u64,
    pub(crate) in_flight: bool,
    pub(crate) last_run: Option<Instant>,
}

impl LookupState {
    /// Atomic spawn-state transition: bump `request_id`, stamp
    /// `last_run`, set `in_flight=true`. The three fields move together
    /// or not at all; open-coding any subset is the symmetric-variant
    /// drift this struct exists to prevent.
    pub(crate) fn begin_spawn(&mut self, now: Instant) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.last_run = Some(now);
        self.in_flight = true;
        self.request_id
    }

    /// Invalidate any in-flight worker without consuming the spawn slot.
    /// Used on branch flips so a stale response carrying the old
    /// `request_id` fails the equality guard in the apply path.
    pub(crate) fn invalidate_in_flight(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        self.in_flight = false;
    }

    pub(crate) fn cooldown_active(&self, now: Instant, interval: Duration) -> bool {
        self.last_run
            .is_some_and(|last| now.duration_since(last) < interval)
    }
}

#[derive(Clone)]
pub(crate) struct PullRequestContextCacheEntry {
    pub(crate) checked_at: Instant,
    pub(crate) head: Option<Oid>,
    pub(crate) pull_request: Option<Arc<PullRequestInfo>>,
}

impl PullRequestContextCacheEntry {
    pub(crate) fn is_fresh(&self, head: Option<&Oid>, now: Instant) -> bool {
        self.head.as_ref() == head
            && now.duration_since(self.checked_at) < PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL
    }

    pub(crate) fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.checked_at) >= PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL * 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PullRequestLookupMode {
    RespectCache,
    ForceRefresh,
}
