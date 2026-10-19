// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};

use std::path::PathBuf;
#[cfg(test)]
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

#[cfg(test)]
use crate::git_context::{
    PACKED_REFS_CACHE_MAX_ENTRIES, PACKED_REFS_MAX_BYTES, read_branch_from_git_head,
    read_context_from_git_metadata, read_git_ref_oid, read_packed_git_ref_oid,
    with_packed_refs_cache,
};
use crate::git_context::{git_current_context, resolve_default_branch};
use crate::pr_context::gh_pull_request_info;
use crate::protocol::attach::{ClientFrame, SpawnRequest};
use crate::protocol::control::SessionInfo;
use crate::pull_request::PullRequestInfo;
use crate::session::{
    BranchName, GitContext, Oid, PullRequestLookupOutcome, SESSION_ENV_PASSTHROUGH, Session,
    SessionEvent, build_agent_command, build_shell_command,
};

use crate::token_monitor::TokenTotals;

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;
#[cfg(test)]
use crate::tui::components::branch_context_bar::branch_context_bar_layout;
#[cfg(test)]
use crate::tui::components::dialog::ConfirmKind;
use crate::tui::components::dialog::{
    Dialog, DialogAction, GithubContextView, PaletteCloseLabel, PaletteCommand, PickerIntent,
    SplitDirection, github_context_view_from_state,
};
use crate::tui::components::status_bar::STATUS_BAR_ROWS;
use crate::tui::components::status_bar::prefix_mode_for_mux_mode;
#[cfg(test)]
use crate::tui::input::mouse_event_allowed_for_mode;
use crate::tui::input::{
    ArrowDir, InputEvent, PrefixCommand, SGR_NO_BUTTON_MOTION, encode_mouse_for_protocol,
    encode_wheel_cursor_fallback, mouse_event_encoding_for_mode, pane_wheel_cursor_fallback_reason,
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
use crate::tui::subscriptions::GIT_BRANCH_CONTEXT_POLL_INTERVAL;
use crate::tui::terminal::normalize_size;
use crate::tui::title::{append_osc_window_title, compose_outer_terminal_title};
#[cfg(test)]
use crate::tui::update::prefix_full_redraw_reason;
use crate::tui::update::{
    FullRedrawReason, HoverFramePlan, dialog_action_frame_plan, drag_resize_ratio,
    drag_resize_redraw_reason, explicit_redraw_reason, focus_change_redraw_reason,
    hover_frame_plan, palette_route_frame_plan, pane_data_redraw_reason,
    selection_change_redraw_reason, selection_start_redraw_reason, status_change_redraw_reason,
    wheel_scrollback_redraw_reason,
};

use jackin_core::Clock;
use jackin_protocol::control::{ClientMsg, ServerMsg};

#[cfg(test)]
use crate::attach_protocol::ControlRequest;
#[cfg(test)]
use crate::attach_protocol::ControlResponse;
#[cfg(test)]
use crate::attach_protocol::drain_and_exit;
#[cfg(test)]
use crate::attach_protocol::handle_attach_client_with_handshake;
#[cfg(test)]
use crate::git_context::WorkdirContext;
#[cfg(test)]
use crate::protocol::attach::ClientTerminal;
#[cfg(test)]
use crate::protocol::attach::ServerFrame;
#[cfg(test)]
use crate::tui::subscriptions::PULL_REQUEST_CONTEXT_LOOKUP_INTERVAL;
#[cfg(test)]
use crate::tui::subscriptions::STATE_TICK_INTERVAL;
#[cfg(test)]
use crate::tui::terminal::DEFAULT_COLS;
#[cfg(test)]
use crate::tui::terminal::DEFAULT_ROWS;
#[cfg(test)]
use jackin_protocol::CapsuleConfig;
#[cfg(test)]
use jackin_protocol::control::SessionEventKind;
#[cfg(test)]
use portable_pty::CommandBuilder;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use tokio::net::UnixStream;
#[cfg(test)]
use tokio::sync::mpsc;
#[cfg(test)]
use tokio::time::Duration;
#[cfg(test)]
use tokio::time::interval;

mod attach_accept;
mod boot;
mod compositor;
mod construct;
mod context_mgmt;
mod control_dispatch;
mod control_reply;
mod dialog_mgmt;
mod events;
use events::{handle_control_subscription, publish_status_events, session_observation};
mod exit_flow;
mod file_export;
mod input_dispatch;
mod mouse_input;
mod multiplexer_utils;
mod pane_layout;
mod ports;
mod resource_metrics;
mod run_loop;
mod session_events;
mod session_lifecycle;
mod state_tick;
mod subsystem_state;
mod subsystems;
mod supervisor;
mod telemetry;

use control_reply::PendingExecReply;

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
    pub(crate) file_exports: file_export::FileExportState,
    pub(crate) pane_home_seq: u64,
    pub(crate) widget_focus: jackin_telemetry::ui::WidgetFocusTracker,
    pub(crate) clock: Arc<dyn Clock>,
}

#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub session_id: u64,
    pub codename: String,
    pub agent: Option<String>,
    pub account_id: Option<String>,
    pub provider: Option<String>,
    pub started_at: DateTime<Utc>,
    pub exited_at: Option<DateTime<Utc>>,
}

const MAX_TABS: usize = 32;

const MAX_SESSIONS: usize = 64;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardImageInsertMode {
    #[default]
    PastePath,
    StageOnly,
}

#[cfg(test)]
use crate::client_writer::scan_emitted_frame;

pub(crate) use attach_accept::accept_attach_handshake;
pub use boot::run_daemon;
#[cfg(test)]
pub(crate) use boot::run_daemon_for_test;
pub(crate) use boot::{
    configured_escape_time, reject_invalid_attach_handshake, screen_detection_disabled_message,
    spawn_boot_tabs,
};
pub(crate) use control_dispatch::{control_server_operation, handle_control_request};
#[cfg(test)]
pub(crate) use exit_flow::build_exit_inspect_rows;
pub(crate) use exit_flow::handle_last_session_exit;
pub(crate) use run_loop::run_daemon_loop;
pub(crate) use session_events::handle_session_event;
pub(crate) use state_tick::handle_state_tick;
pub(crate) use subsystem_state::{
    BrokerUsageRefresh, ClientRegistry, ClipboardState, ControlRouting, LaunchEnv, LookupState,
    PendingAttachControl, PrWatch, PullRequestContextCacheEntry, PullRequestLookupMode,
    RenderState, StatusState, UsageState,
};
pub(crate) use supervisor::{
    SessionLaunch, SessionRegistry, SessionSupervisor, session_display_title,
};
pub(crate) use telemetry::{record_agent_status_tick, record_skipped_provider_probe};

mod control;
pub use control::*;

#[cfg(test)]
mod tests;
