// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, Interval, interval};

use crate::agent_status::rules::RulePackRegistry;
use crate::attach_protocol::{AttachHandshake, ControlRequest, initial_spawn_requests};
use crate::git_context::start_git_context_watcher;
use crate::protocol::attach::{ClientFrame, SpawnRequest};
use crate::socket;
use crate::tui::subscriptions::{
    GIT_BRANCH_CONTEXT_POLL_INTERVAL, STATE_TICK_INTERVAL, USAGE_ACCOUNT_REFRESH_POLL_INTERVAL,
};
use crate::tui::terminal::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};

use super::{Multiplexer, configured_escape_time, screen_detection_disabled_message};

mod events;

pub(crate) async fn run_daemon_loop(
    initial_agent: String,
    launch_config: CapsuleConfig,
    telemetry: &mut crate::telemetry::FlushGuard,
    socket_path: &Path,
) -> Result<()> {
    let rows = std::env::var("JACKIN_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ROWS);
    let cols = std::env::var("JACKIN_COLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_COLS);
    let (rows, cols) = normalize_size(rows, cols);

    crate::logging::init();
    let _live_dhat_profiler = crate::alloc_telemetry::init_from_env();
    crate::debug_panic::panic_if_requested_from_env();

    DaemonLoop::initialize(
        &initial_agent,
        launch_config,
        telemetry,
        socket_path,
        rows,
        cols,
    )?
    .run()
    .await
}

// Fields follow the reverse order of the original local bindings: Rust drops
// struct fields in declaration order, retaining the loop resources' teardown order.
struct DaemonLoop {
    last_frame_at: Option<tokio::time::Instant>,
    esc_deadline: Option<tokio::time::Instant>,
    escape_time: Duration,
    control_rx: mpsc::UnboundedReceiver<ControlRequest>,
    control_tx: mpsc::UnboundedSender<ControlRequest>,
    handshake_rx: mpsc::UnboundedReceiver<AttachHandshake>,
    handshake_tx: mpsc::UnboundedSender<AttachHandshake>,
    cmd_rx: mpsc::UnboundedReceiver<ClientFrame>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
    sigint: Signal,
    sigterm: Signal,
    usage_account_ticker: Interval,
    state_ticker: Interval,
    branch_context_ticker: Interval,
    rule_registry: Option<RulePackRegistry>,
    new_clients:
        mpsc::UnboundedReceiver<(tokio::net::UnixStream, tokio::sync::OwnedSemaphorePermit)>,
    pending_initial_spawns: Vec<SpawnRequest>,
    mux: Multiplexer,
}

impl DaemonLoop {
    fn initialize(
        initial_agent: &str,
        launch_config: CapsuleConfig,
        telemetry: &mut crate::telemetry::FlushGuard,
        socket_path: &Path,
        rows: u16,
        cols: u16,
    ) -> Result<Self> {
        let initial_spawns = initial_spawn_requests(initial_agent, &launch_config);
        let mut mux = Multiplexer::new(rows, cols, launch_config)?;
        start_git_context_watcher(mux.launch_env.workdir.clone(), mux.control.event_tx.clone());
        let pending_initial_spawns = initial_spawns;

        let new_clients = socket::start_listener_at(socket_path)?;
        telemetry.listener_ready();
        let rule_registry = match RulePackRegistry::bundled() {
            Ok(registry) => Some(registry),
            Err(e) => {
                let _warning = jackin_telemetry::record_recovered_degradation();
                mux.open_spawn_failure_dialog(screen_detection_disabled_message(&e));
                None
            }
        };
        let branch_context_ticker = interval(GIT_BRANCH_CONTEXT_POLL_INTERVAL);
        let state_ticker = interval(STATE_TICK_INTERVAL);
        let usage_account_ticker = interval(USAGE_ACCOUNT_REFRESH_POLL_INTERVAL);
        let sigterm = signal(SignalKind::terminate())?;
        let sigint = signal(SignalKind::interrupt())?;

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ClientFrame>();
        let (handshake_tx, handshake_rx) = mpsc::unbounded_channel::<AttachHandshake>();
        let (control_tx, control_rx) = mpsc::unbounded_channel::<ControlRequest>();

        let escape_time = configured_escape_time();

        let esc_deadline: Option<tokio::time::Instant> = None;
        let last_frame_at: Option<tokio::time::Instant> = None;
        Ok(Self {
            last_frame_at,
            esc_deadline,
            escape_time,
            control_rx,
            control_tx,
            handshake_rx,
            handshake_tx,
            cmd_rx,
            cmd_tx,
            sigint,
            sigterm,
            usage_account_ticker,
            state_ticker,
            branch_context_ticker,
            rule_registry,
            new_clients,
            pending_initial_spawns,
            mux,
        })
    }
}
