// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use anyhow::Result;

use crate::attach_protocol::{detach_client, drain_and_exit, perform_handshake};
use crate::clipboard::cleanup_clipboard_run_dir;
use crate::tui::subscriptions::RENDER_TICK_INTERVAL;
use crate::tui::update::dialog_change_redraw_reason;

use super::super::{
    accept_attach_handshake, coalesce_client_frames, handle_client_frame, handle_control_request,
    handle_last_session_exit, handle_session_event, handle_state_tick,
};
use super::DaemonLoop;

impl DaemonLoop {
    pub(super) async fn run(mut self) -> Result<()> {
        loop {
            if let Some(action) = self.mux.control.exit_request.take() {
                if let Err(error) = crate::exit_assess::write_exit_action(action) {
                    let _warning = jackin_telemetry::record_recovered_degradation();
                    crate::output::stderr_line(format_args!(
                        "[daemon] exit: failed to write exit-action file, retrying: {error}"
                    ));
                    self.mux.control.exit_request = Some(action);
                } else {
                    drain_and_exit(&mut self.mux).await;
                    return Ok(());
                }
            }
            if self.mux.control.input_parser.esc_pending() {
                if self.esc_deadline.is_none() {
                    self.esc_deadline = Some(tokio::time::Instant::now() + self.escape_time);
                }
            } else {
                self.esc_deadline = None;
            }
            let render_deadline: Option<tokio::time::Instant> = if self.mux.has_pending_render()
                || self.mux.client_registry.client.has_out_of_band()
            {
                Some(
                    self.last_frame_at
                        .map_or_else(tokio::time::Instant::now, |last| {
                            (last + RENDER_TICK_INTERVAL).max(tokio::time::Instant::now())
                        }),
                )
            } else {
                None
            };
            tokio::select! {
                biased;

                _ = self.sigterm.recv() => {
                    detach_client(&mut self.mux).await;
                    cleanup_clipboard_run_dir();
                    return Ok(());
                }
                _ = self.sigint.recv() => {
                    detach_client(&mut self.mux).await;
                    cleanup_clipboard_run_dir();
                    return Ok(());
                }

                Some((stream, client_permit)) = self.new_clients.recv() => {
                    let handshake_tx = self.handshake_tx.clone();
                    let control_tx = self.control_tx.clone();
                    jackin_telemetry::spawn::spawn_detached_with_completion(
                        &jackin_telemetry::operation::CONNECTION_ATTEMPT,
                        perform_handshake(stream, client_permit, handshake_tx, control_tx),
                    );
                }

                Some(request) = self.control_rx.recv() => handle_control_request(&mut self.mux, request),

                Some(ready) = self.handshake_rx.recv() => {
                    accept_attach_handshake(
                        &mut self.mux,
                        ready,
                        &mut self.pending_initial_spawns,
                        &self.cmd_tx,
                        &mut self.cmd_rx,
                    )
                    .await?;
                }

                Some(frame) = self.cmd_rx.recv() => {
                    let (frames, _coalesced) =
                        coalesce_client_frames(frame, || self.cmd_rx.try_recv().ok());
                    for frame in frames {
                        handle_client_frame(&mut self.mux, frame);
                        if self.mux.client_registry.detach_requested {
                            break;
                        }
                    }
                    if self.mux.client_registry.detach_requested {
                        self.mux.client_registry.detach_requested = false;
                        detach_client(&mut self.mux).await;
                    }
                    if self.mux.no_live_sessions()
                        && handle_last_session_exit(&mut self.mux, None).await
                    {
                        cleanup_clipboard_run_dir();
                        return Ok(());
                    }
                }

                _ = self.state_ticker.tick() => {
                    handle_state_tick(&mut self.mux, self.rule_registry.as_ref()).await;
                }

                Some(event) = self.mux.control.event_rx.recv() => {
                    if handle_session_event(&mut self.mux, event).await {
                        return Ok(());
                    }
                }

                () = async {
                    match self.esc_deadline {
                        Some(d) => tokio::time::sleep_until(d).await,
                        None => std::future::pending().await,
                    }
                }, if self.esc_deadline.is_some() => {
                    self.esc_deadline = None;
                    let events = self.mux.control.input_parser.flush_pending_esc();
                    for event in events {
                        self.mux.handle_input(event);
                    }
                }

                () = async {
                    match render_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                }, if render_deadline.is_some() => {
                    let frame_data = self.mux.compose_pending_frame();
                    self.mux.send_frame(frame_data);
                    self.last_frame_at = Some(tokio::time::Instant::now());
                }

                _ = self.branch_context_ticker.tick() => {
                    self.mux.maybe_spawn_git_branch_context_lookup(Instant::now());
                }

                _ = self.usage_account_ticker.tick() => {
                    let refreshed = self.mux.finish_usage_account_refresh_if_ready().await;
                    self.mux.spawn_active_usage_account_refresh();
                    if refreshed && self.mux.refresh_open_usage_dialog_from_cache() {
                        self.mux.invalidate(dialog_change_redraw_reason());
                    }
                }

            }
        }
    }
}
