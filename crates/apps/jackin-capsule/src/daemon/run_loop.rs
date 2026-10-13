// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Daemon event loop: socket acceptor, attach flow, ticks, and rendering.

use std::path::Path;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use std::time::Instant;

use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::agent_status::rules::RulePackRegistry;
use crate::attach_protocol::{
    AttachHandshake, ControlRequest, detach_client, drain_and_exit, initial_spawn_requests,
    perform_handshake,
};
use crate::clipboard::cleanup_clipboard_run_dir;
use crate::git_context::start_git_context_watcher;

use crate::protocol::attach::ClientFrame;

use crate::socket;

use crate::tui::subscriptions::{
    GIT_BRANCH_CONTEXT_POLL_INTERVAL, RENDER_TICK_INTERVAL, STATE_TICK_INTERVAL,
    USAGE_ACCOUNT_REFRESH_POLL_INTERVAL,
};
use crate::tui::terminal::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};

use crate::tui::update::dialog_change_redraw_reason;

use super::{
    Multiplexer, accept_attach_handshake, coalesce_client_frames, configured_escape_time,
    handle_client_frame, handle_control_request, handle_last_session_exit, handle_session_event,
    handle_state_tick, screen_detection_disabled_message,
};

#[expect(
    clippy::too_many_lines,
    reason = "Top-level daemon entry point: spawns the event loop, the attach \
              socket acceptor, and the input parser in sequence. Each stage has \
              its own focused init + handoff. Body extraction follows the same \
              deferred-parallel-pass plan as the launch fns — the inline shape \
              preserves captured-runtime state across stages."
)]
/// # Errors
///
/// Returns an error when daemon initialization, socket setup, session
/// management, or the event loop fails.
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

    // Resolve Capsule telemetry detail and install panic handling after OTLP so
    // crash events use the active governed exporter.
    crate::logging::init();
    let _live_dhat_profiler = crate::alloc_telemetry::init_from_env();
    crate::debug_panic::panic_if_requested_from_env();

    let initial_spawns = initial_spawn_requests(&initial_agent, &launch_config);
    let mut mux = Multiplexer::new(rows, cols, launch_config)?;
    start_git_context_watcher(mux.launch_env.workdir.clone(), mux.control.event_tx.clone());
    // Defer the boot tabs until the first attach Hello has supplied
    // real outer-terminal dimensions. Later panes already spawn after
    // attach-time resize; routing the boot tabs through the same
    // path removes first-tab-only scrollback/chrome differences.
    let mut pending_initial_spawns = initial_spawns;

    let mut new_clients = socket::start_listener_at(socket_path)?;
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
                accept_attach_handshake(
                    &mut mux,
                    ready,
                    &mut pending_initial_spawns,
                    &cmd_tx,
                    &mut cmd_rx,
                )
                .await?;
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
                if handle_session_event(&mut mux, event).await {
                    return Ok(());
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

            // Account refresh scheduler. Provider work stays in the host broker;
            // Capsule renderers adopt only scoped relay projections.
            _ = usage_account_ticker.tick() => {
                let refreshed = mux.finish_usage_account_refresh_if_ready().await;
                mux.spawn_active_usage_account_refresh();
                if refreshed && mux.refresh_open_usage_dialog_from_cache() {
                    mux.invalidate(dialog_change_redraw_reason());
                }
            }

        }
    }
}
