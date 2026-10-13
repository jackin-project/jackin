// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Attach shutdown and detach lifecycle.

use crate::daemon::Multiplexer;
use crate::protocol::attach::{ServerFrame, encode_server};
use tokio::time::Duration;

pub(crate) async fn drain_and_exit(mux: &mut Multiplexer) {
    drain_and_exit_with_reason(mux, None).await;
}

pub(crate) async fn drain_and_exit_with_reason(mux: &mut Multiplexer, reason: Option<String>) {
    if let Some(reason) = reason.as_deref() {
        mux.send_out_of_band(format!("\r\n[jackin-capsule] {reason}\r\n").into_bytes());
    }
    gracefully_detach_attached_task_with_reason(mux, "drain_and_exit", reason.as_deref()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
}

pub(crate) const ATTACH_SHUTDOWN_FLUSH_GRACE_MS: u64 = 50;
pub(crate) const ATTACH_SHUTDOWN_CLOSE_GRACE_MS: u64 = 1000;

pub(crate) fn send_attached_shutdown(mux: &mut Multiplexer, reason: Option<&str>) -> bool {
    mux.client_registry.client.flush_out_of_band();
    let Some(tx) = mux.client_registry.client.take() else {
        return false;
    };
    drop(tx.send(encode_server(ServerFrame::Shutdown {
        reason: reason.map(str::to_owned),
    })));
    true
}

/// Centralised detach for the currently-attached client. Take-then-
/// send-then-wait-then-abort, in that order, so a takeover/cancel race never
/// leaves `attached_task = Some` with a dead `attached_out`: take the
/// out-channel sender first (so the next frame queue allocation does
/// not race with the old receiver), send Shutdown best-effort, give
/// the attach task a brief writer-side drain window, then
/// abort the attach task so its reader stops pushing into the shared
/// `cmd_tx`. Used by SIGTERM / SIGINT shutdown, explicit detach, and
/// `drain_and_exit`.
pub(crate) async fn detach_attached_task(mux: &mut Multiplexer, context: &str) {
    detach_attached_task_with_reason(mux, context, None).await;
}

pub(crate) async fn detach_attached_task_with_reason(
    mux: &mut Multiplexer,
    _context: &str,
    reason: Option<&str>,
) {
    let had_sender = send_attached_shutdown(mux, reason);
    // The latch is paired with the sender's lifetime: clearing
    // `attached_out` invalidates the previous attach, so the next
    // assignment (in the takeover branch of `run_daemon`) starts from
    // a clean state regardless of which code path reassigns it.
    if had_sender {
        tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_FLUSH_GRACE_MS)).await;
    }
    if let Some(handle) = mux.client_registry.attached_task.take() {
        handle.abort();
    }
}

pub(crate) async fn gracefully_detach_attached_task_with_reason(
    mux: &mut Multiplexer,
    _context: &str,
    reason: Option<&str>,
) {
    let had_sender = send_attached_shutdown(mux, reason);
    let Some(mut handle) = mux.client_registry.attached_task.take() else {
        return;
    };
    if !had_sender {
        handle.abort();
        return;
    }
    tokio::select! {
        result = &mut handle => {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                let _error = jackin_telemetry::record_error(
                    jackin_telemetry::schema::enums::ErrorType::Panic,
                );
            }
        }
        () = tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_CLOSE_GRACE_MS)) => {
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::Timeout,
            );
            handle.abort();
        }
    }
}
