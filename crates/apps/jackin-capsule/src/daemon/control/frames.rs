// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Client-frame dispatch, attach control, and frame coalescing.

use super::super::{
    ClientFrame, ClipboardImageInsertMode, Multiplexer, Result, explicit_redraw_reason,
    prefix_mode_for_mux_mode,
};

use jackin_protocol::attach::{AttachControlOperation, AttachControlRequest, AttachControlResult};

use super::{RPC_ERROR, send_attach_control_response};

fn attach_control_operation(
    request: &AttachControlRequest,
    method: &'static str,
) -> Result<Option<jackin_telemetry::operation::OperationGuard>, AttachControlResult> {
    let extracted = jackin_telemetry::propagation::extract(&request.context);
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        return Err(AttachControlResult::InvalidCorrelation);
    }
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(method),
        },
    ];
    let operation = match extracted {
        jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
            jackin_telemetry::operation_with_remote_parent(
                &jackin_telemetry::operation::RPC_SERVER,
                &attrs,
                &parent,
            )
        }
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
    }
    .ok();
    Ok(operation)
}

pub fn handle_client_frame(mux: &mut Multiplexer, frame: ClientFrame) {
    match frame {
        ClientFrame::AttachControl(request) => handle_attach_control(mux, request),
        ClientFrame::Hello { .. } => {
            // The initial Hello is consumed by the accept handler; any
            // further Hello on the same connection is a protocol error.
            let _error = jackin_telemetry::record_error(RPC_ERROR);
        }
        ClientFrame::Resize { rows, cols } => {
            // resize() records the Resize invalidation (and its wipe); the
            // render loop composes the resized frame on the next pass.
            mux.resize(rows, cols);
        }
        ClientFrame::Input(bytes) => {
            let events = mux.control.input_parser.parse(&bytes);
            for event in events {
                mux.handle_input(event);
            }
            let prefix_mode = prefix_mode_for_mux_mode(mux.mux_mode());
            if mux.status.status_bar.prefix_mode != prefix_mode {
                mux.status.status_bar.set_prefix_mode(prefix_mode);
                mux.invalidate(explicit_redraw_reason());
            }
        }
        ClientFrame::Command(_payload) => {
            // Reserved for future structured commands from the host CLI.
        }
        ClientFrame::ClipboardImage(image) => {
            mux.stage_clipboard_image_response(image);
        }
        ClientFrame::ClipboardImageStart(start) => {
            let size = start.size;
            if let Err(err) = mux.clipboard.clipboard_image_transfers.start(start) {
                let _error = jackin_telemetry::record_error(RPC_ERROR);
                mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
                mux.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
            } else if mux.clipboard.clipboard_image_insert_mode
                == ClipboardImageInsertMode::StageOnly
            {
                mux.set_clipboard_image_notice(format!("Image staging: receiving {size} bytes"));
            } else {
                mux.set_clipboard_image_notice(format!("Image paste: receiving {size} bytes"));
            }
        }
        ClientFrame::ClipboardImageChunk(chunk) => {
            if let Err(err) = mux.clipboard.clipboard_image_transfers.chunk(chunk) {
                let _error = jackin_telemetry::record_error(RPC_ERROR);
                mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
                mux.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
            }
        }
        ClientFrame::ClipboardImageEnd(end) => {
            match mux.clipboard.clipboard_image_transfers.end(end) {
                Ok(image) => {
                    mux.stage_clipboard_image_response(image);
                }
                Err(err) => {
                    let _error = jackin_telemetry::record_error(RPC_ERROR);
                    mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
                    mux.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
                }
            }
        }
        ClientFrame::ClipboardImageError(error) => {
            let _error_event = jackin_telemetry::record_error(RPC_ERROR);
            mux.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
            mux.set_clipboard_image_notice(format!("Image paste rejected: {error}"));
        }
        ClientFrame::HostNotice(message) => {
            mux.set_clipboard_image_notice(message);
        }
        ClientFrame::Detach => {
            mux.client_registry.detach_requested = true;
        }
        ClientFrame::FocusIn => {
            // Forward only when no dialog is intercepting input AND
            // the focused session actually asked for focus reports
            // (`?1004h`). Without the gate, normal-screen shells
            // surface `[I` as literal text at the prompt.
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.session_supervisor.sessions.get(focused)
                && s.focus_events_enabled()
            {
                let _sent = s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.session_supervisor.sessions.get(focused)
                && s.focus_events_enabled()
            {
                let _sent = s.send_input(b"\x1b[O");
            }
        }
    }
}

fn handle_pending_clipboard_transfer(
    mux: &mut Multiplexer,
    request: &AttachControlRequest,
) -> bool {
    let request_id = request.request_id;
    match &request.operation {
        AttachControlOperation::ClipboardImageChunk(chunk) => {
            let Some(pending) = mux
                .clipboard
                .attach_control_operations
                .get(&chunk.transfer_id)
            else {
                send_attach_control_response(mux, request_id, AttachControlResult::Rejected, None);
                return true;
            };
            if pending.request_id != request_id || pending.context != request.context {
                send_attach_control_response(mux, request_id, AttachControlResult::Rejected, None);
                return true;
            }
            let result = mux.clipboard.clipboard_image_transfers.chunk(chunk.clone());
            if result.is_err()
                && let Some(pending) = mux
                    .clipboard
                    .attach_control_operations
                    .remove(&chunk.transfer_id)
            {
                send_attach_control_response(
                    mux,
                    pending.request_id,
                    AttachControlResult::Rejected,
                    pending.operation,
                );
            }
            true
        }
        AttachControlOperation::ClipboardImageEnd(end) => {
            let Some(pending) = mux
                .clipboard
                .attach_control_operations
                .get(&end.transfer_id)
            else {
                send_attach_control_response(mux, request_id, AttachControlResult::Rejected, None);
                return true;
            };
            if pending.request_id != request_id || pending.context != request.context {
                send_attach_control_response(mux, request_id, AttachControlResult::Rejected, None);
                return true;
            }
            let Some(pending) = mux
                .clipboard
                .attach_control_operations
                .remove(&end.transfer_id)
            else {
                send_attach_control_response(mux, request_id, AttachControlResult::Rejected, None);
                return true;
            };
            let succeeded = mux
                .clipboard
                .clipboard_image_transfers
                .end(end.clone())
                .is_ok_and(|image| mux.stage_clipboard_image_response(image));
            send_attach_control_response(
                mux,
                pending.request_id,
                if succeeded {
                    AttachControlResult::Success
                } else {
                    AttachControlResult::Rejected
                },
                pending.operation,
            );
            true
        }
        _ => false,
    }
}

fn handle_attach_control(mux: &mut Multiplexer, request: AttachControlRequest) {
    let request_id = request.request_id;
    let method = match &request.operation {
        AttachControlOperation::Detach => "jackin.capsule.Attach/Detach",
        AttachControlOperation::FocusIn | AttachControlOperation::FocusOut => {
            "jackin.capsule.Attach/Focus"
        }
        _ => "jackin.capsule.Attach/ClipboardImageTransfer",
    };
    if matches!(
        jackin_telemetry::propagation::extract(&request.context),
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        let attrs = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
                value: jackin_telemetry::Value::Str("jackin"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
                value: jackin_telemetry::Value::Str(method),
            },
        ];
        let operation =
            jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs).ok();
        let record = || {
            let _error = jackin_telemetry::record_error(RPC_ERROR);
        };
        if let Some(operation) = operation.as_ref() {
            operation.span().in_scope(record);
        } else {
            record();
        }
        send_attach_control_response(
            mux,
            request_id,
            AttachControlResult::InvalidCorrelation,
            operation,
        );
        return;
    }
    if handle_pending_clipboard_transfer(mux, &request) {
        return;
    }

    let operation = match attach_control_operation(&request, method) {
        Ok(operation) => operation,
        Err(result) => {
            send_attach_control_response(mux, request_id, result, None);
            return;
        }
    };
    match request.operation {
        AttachControlOperation::Detach => {
            let attrs = [jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::UI_ACTION_NAME,
                value: jackin_telemetry::Value::Str(
                    jackin_telemetry::schema::enums::UiActionName::SessionDetach.as_str(),
                ),
            }];
            let action =
                jackin_telemetry::operation(&jackin_telemetry::operation::UI_ACTION, &attrs).ok();
            mux.client_registry.detach_requested = true;
            if let Some(action) = action {
                action.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
            }
            send_attach_control_response(mux, request_id, AttachControlResult::Success, operation);
        }
        AttachControlOperation::FocusIn => {
            handle_client_frame(mux, ClientFrame::FocusIn);
            send_attach_control_response(mux, request_id, AttachControlResult::Success, operation);
        }
        AttachControlOperation::FocusOut => {
            handle_client_frame(mux, ClientFrame::FocusOut);
            send_attach_control_response(mux, request_id, AttachControlResult::Success, operation);
        }
        AttachControlOperation::ClipboardImage(image) => {
            let succeeded = mux.stage_clipboard_image_response(image);
            send_attach_control_response(
                mux,
                request_id,
                if succeeded {
                    AttachControlResult::Success
                } else {
                    AttachControlResult::Rejected
                },
                operation,
            );
        }
        AttachControlOperation::ClipboardImageStart(start) => {
            let transfer_id = start.transfer_id;
            if mux.clipboard.clipboard_image_transfers.start(start).is_ok() {
                mux.clipboard.attach_control_operations.insert(
                    transfer_id,
                    super::super::PendingAttachControl {
                        request_id,
                        context: request.context.clone(),
                        operation,
                    },
                );
            } else {
                send_attach_control_response(
                    mux,
                    request_id,
                    AttachControlResult::Rejected,
                    operation,
                );
            }
        }
        AttachControlOperation::ClipboardImageError(error) => {
            handle_client_frame(mux, ClientFrame::ClipboardImageError(error));
            send_attach_control_response(mux, request_id, AttachControlResult::Rejected, operation);
        }
        AttachControlOperation::ClipboardImageChunk(_)
        | AttachControlOperation::ClipboardImageEnd(_) => unreachable!(),
    }
}

/// Coalesce a run of consecutive `Resize` frames into the latest size and
/// return the ordered frames the daemon must process, plus how many resizes
/// were coalesced away.
///
/// A non-`Resize` frame pulled from the channel while draining is preserved and
/// returned after the coalesced resize (previously it was silently dropped
/// because `try_recv()` removes a frame before the `while let` pattern rejects
/// it). Order is preserved because the stray frame may depend on the new
/// geometry.
pub(crate) fn coalesce_client_frames(
    first: ClientFrame,
    mut next: impl FnMut() -> Option<ClientFrame>,
) -> (Vec<ClientFrame>, u32) {
    if !matches!(first, ClientFrame::Resize { .. }) {
        return (vec![first], 0);
    }
    let mut latest = first;
    let mut coalesced: u32 = 0;
    loop {
        match next() {
            Some(ClientFrame::Resize { rows, cols }) => {
                latest = ClientFrame::Resize { rows, cols };
                coalesced = coalesced.saturating_add(1);
            }
            Some(other) => return (vec![latest, other], coalesced),
            None => return (vec![latest], coalesced),
        }
    }
}
