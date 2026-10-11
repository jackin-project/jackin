// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Attach-handshake acceptance: auth, boot tabs, and client takeover.

use anyhow::Result;

use tokio::sync::mpsc;

use crate::attach_protocol::{
    AttachHandshake, detach_attached_task, handle_attach_client_with_handshake, spawn_request_label,
};

use crate::protocol::attach::{ClientFrame, ServerFrame, SpawnRequest, encode_server};

use crate::tui::model::PointerShape;

use crate::tui::update::first_attach_redraw_reason;
use crate::tui::view::spawn_request_failure_message;

use super::{
    Multiplexer, RPC_ERROR, attach_peer_is_authorized, reject_invalid_attach_handshake,
    spawn_boot_tabs,
};

/// Accept a validated attach handshake: authorize the peer, adopt the
/// outer-terminal geometry, drain deferred boot tabs, take over from
/// any previous client, and wire the new attach task.
///
/// Returns the boot-spawn failure when a deferred tab fails to spawn.
pub(crate) async fn accept_attach_handshake(
    mux: &mut Multiplexer,
    ready: AttachHandshake,
    pending_initial_spawns: &mut Vec<SpawnRequest>,
    cmd_tx: &mpsc::UnboundedSender<ClientFrame>,
    cmd_rx: &mut mpsc::UnboundedReceiver<ClientFrame>,
) -> Result<()> {
    let AttachHandshake {
        stream,
        peer_uid,
        rows,
        cols,
        spawn,
        env,
        terminal,
        context,
        focus_session,
        client_permit,
    } = ready;
    if !attach_peer_is_authorized(mux, Some(peer_uid)) {
        let mut stream = stream;
        reject_invalid_attach_handshake(&mut stream).await;
        drop(client_permit);
        return Ok(());
    }
    let extracted = context.as_ref().map_or(
        jackin_telemetry::propagation::ExtractOutcome::LocalRoot,
        |ctx| jackin_telemetry::propagation::extract(ctx.as_ref()),
    );
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        let mut stream = stream;
        reject_invalid_attach_handshake(&mut stream).await;
        drop(client_permit);
        return Ok(());
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
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
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
        && !pending_initial_spawns.is_empty()
        && let Some(err) = spawn_boot_tabs(&mut *mux, &mut *pending_initial_spawns)
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
        use super::ports::{AttachPort, PORTS};
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
    use super::ports::{AttachPort, AttachTransition, PORTS};
    if PORTS.begin_attach(&mux.client_registry) == AttachTransition::Displace {
        detach_attached_task(&mut *mux, "takeover").await;
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
    while cmd_rx.try_recv().is_ok() {}
    let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (completion_tx, completion_rx) = mpsc::unbounded_channel();
    mux.client_registry
        .client
        .attach_with_completions(new_out_tx.clone(), completion_tx);
    // A send failure here means the receiver closed in a takeover
    // race during this tick; the attach boundary owns one error.
    let mut initial_frames = Vec::with_capacity(5);
    initial_frames.push(encode_server(ServerFrame::Welcome {
        session_count: u32::try_from(mux.session_supervisor.sessions.len()).unwrap_or(u32::MAX),
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
        let _error =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::RpcError);
    }
    let cmd_tx_for_task = cmd_tx.clone();
    mux.client_registry.attached_task = Some(jackin_telemetry::spawn::spawn_stream(
        "capsule.attach",
        async move {
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
        },
    ));
    Ok(())
}
