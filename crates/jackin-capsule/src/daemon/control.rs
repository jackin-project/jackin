// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Control reply + status capture helpers extracted from the daemon
//! coordinator: `write_status_capture`, `control_reply_for_request`, and the
//! related reply builders.

use super::{
    ClientFrame, ClientMsg, ClipboardImageInsertMode, Instant, Multiplexer, PathBuf, Result,
    ServerMsg, Session, TokenTotals, explicit_redraw_reason, prefix_mode_for_mux_mode,
};
use jackin_core::container_paths;
use jackin_protocol::attach::{
    AttachControlOperation, AttachControlRequest, AttachControlResponse, AttachControlResult,
};
use jackin_telemetry::ResultTelemetryExt as _;

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

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

pub(super) fn send_attach_control_response(
    mux: &mut Multiplexer,
    request_id: u64,
    result: AttachControlResult,
    operation: Option<jackin_telemetry::operation::OperationGuard>,
) {
    let failed = result != AttachControlResult::Success;
    mux.client_registry.client.send_attach_response(
        AttachControlResponse { request_id, result },
        crate::attach_protocol::AttachResponseCompletion {
            request_id,
            operation,
            outcome: if failed {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            },
            error_type: failed.then_some(RPC_ERROR),
        },
    );
}

/// Write a capture fixture for `session`: its live visible grid (`visible.txt`)
/// and the current evidence report (`evidence.json`), under
/// `/jackin/state/agent-status/captures/<id>-<seq>/`. Turns a live
/// mis-detection into a regression fixture in one command.
pub fn write_status_capture(session_id: u64, session: &Session) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CAPTURE_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = CAPTURE_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(container_paths::AGENT_STATUS_CAPTURES_DIR)
        .join(format!("{session_id}-{seq}"));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("visible.txt"),
        session.visible_screen_rows().join("\n"),
    )?;
    let report = session.status.report(session.agent.clone());
    std::fs::write(
        dir.join("evidence.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    Ok(())
}

pub fn control_reply_for_request(mux: &mut Multiplexer, msg: ClientMsg) -> ServerMsg {
    match msg {
        ClientMsg::TelemetryHealth => {
            let health = jackin_diagnostics::telemetry_health_snapshot();
            ServerMsg::TelemetryHealth {
                report: Box::new(telemetry_health_report(health)),
            }
        }
        ClientMsg::Status => ServerMsg::SessionList {
            sessions: mux.session_infos(),
        },
        ClientMsg::Snapshot => ServerMsg::Snapshot {
            tabs: mux.tab_snapshots(),
            active_tab: u32::try_from(mux.session_supervisor.active_tab).unwrap_or(0),
        },
        ClientMsg::Agents => ServerMsg::AgentRegistry {
            records: mux.agent_registry_snapshot(),
        },
        // Forwarded in-container reporter event: apply it to the addressed
        // session's authority and Ack immediately (never block the agent hook).
        ClientMsg::ReportRuntimeEvent {
            session_id,
            source_id,
            runtime,
            event,
            payload,
        } => {
            use super::ports::{ControlPort, PORTS, RuntimeEvent};
            PORTS.report_runtime_event(
                &mut mux.session_supervisor.sessions,
                RuntimeEvent {
                    session_id,
                    source_id: &source_id,
                    runtime: &runtime,
                    event: &event,
                    payload: payload.as_deref(),
                    observed_at: Instant::now(),
                },
            )
        }
        // Contributor diagnostic: snapshot the live grid + evidence to a fixture.
        ClientMsg::StatusCapture { session_id } => {
            if let Some(session) = mux.session_supervisor.sessions.get(session_id) {
                drop(
                    write_status_capture(session_id, session).record_telemetry_error(
                        jackin_telemetry::schema::enums::ErrorType::IoError,
                    ),
                );
            } else {
                let _error = jackin_telemetry::record_error(RPC_ERROR);
            }
            ServerMsg::Ack
        }
        ClientMsg::UsageFocused => ServerMsg::UsageFocused {
            usage: Box::new(mux.focused_usage_snapshot()),
        },
        ClientMsg::UsageRefreshFocused => {
            mux.request_usage_refresh_for_provider(None);
            ServerMsg::UsageFocused {
                usage: Box::new(mux.focused_usage_snapshot()),
            }
        }
        ClientMsg::UsageAccountList => ServerMsg::UsageAccounts {
            accounts: mux.usage.cache().account_snapshot_views(),
        },
        ClientMsg::ExecCommand { .. } => {
            // Defensive only: `ExecCommand` is intercepted by the control loop
            // (it opens `Dialog::ExecPicker` and replies after the operator
            // confirms/cancels), so it never reaches this synchronous path.
            // Fail closed if it ever does.
            ServerMsg::ExecDenied {
                reason: "jackin-exec must be dispatched through the credential picker".to_owned(),
            }
        }
        ClientMsg::TokenUsage { session_id } => ServerMsg::TokenUsage {
            summary: mux
                .usage
                .token_monitor
                .totals(session_id)
                .map(TokenTotals::to_summary),
        },
        ClientMsg::Unknown => {
            let _error = jackin_telemetry::record_error(RPC_ERROR);
            ServerMsg::Unknown
        }
    }
}

fn telemetry_health_report(
    health: jackin_diagnostics::TelemetryHealth,
) -> jackin_protocol::control::TelemetryHealthReport {
    fn signal(
        health: jackin_diagnostics::TelemetrySignalHealth,
    ) -> jackin_protocol::control::TelemetrySignalHealth {
        jackin_protocol::control::TelemetrySignalHealth {
            attempts: health.attempts,
            successes: health.successes,
            failures: health.failures,
        }
    }
    let flush = match health.flush {
        jackin_diagnostics::TelemetryFlushStatus::Pending => {
            jackin_protocol::control::TelemetryFlushStatus::Pending
        }
        jackin_diagnostics::TelemetryFlushStatus::Succeeded => {
            jackin_protocol::control::TelemetryFlushStatus::Succeeded
        }
        jackin_diagnostics::TelemetryFlushStatus::Failed => {
            jackin_protocol::control::TelemetryFlushStatus::Failed
        }
    };
    let capsule_export = match health.capsule_export {
        jackin_diagnostics::CapsuleExportCoverage::Enabled => {
            jackin_protocol::control::CapsuleExportCoverage::Enabled
        }
        jackin_diagnostics::CapsuleExportCoverage::DisabledNoEndpoint => {
            jackin_protocol::control::CapsuleExportCoverage::DisabledNoEndpoint
        }
        jackin_diagnostics::CapsuleExportCoverage::DisabledNetworkNone => {
            jackin_protocol::control::CapsuleExportCoverage::DisabledNetworkNone
        }
        jackin_diagnostics::CapsuleExportCoverage::DisabledUnclassifiedEndpoint => {
            jackin_protocol::control::CapsuleExportCoverage::DisabledUnclassifiedEndpoint
        }
        jackin_diagnostics::CapsuleExportCoverage::DisabledUnclassifiedAuth => {
            jackin_protocol::control::CapsuleExportCoverage::DisabledUnclassifiedAuth
        }
        jackin_diagnostics::CapsuleExportCoverage::NotApplicable => {
            jackin_protocol::control::CapsuleExportCoverage::NotApplicable
        }
    };
    let (resolved, config_failure) = match jackin_diagnostics::resolved_otlp_config_fingerprint() {
        Ok(config) => (config, None),
        Err(failure) => (None, Some(telemetry_config_failure(failure))),
    };
    let config_signal = |value: jackin_diagnostics::OtlpSignalFingerprint| {
        jackin_protocol::control::TelemetrySignalConfigFingerprint {
            authority: value.authority,
            tls: value.tls,
        }
    };
    let (traces, logs, metrics, compression, sampler) = resolved.map_or_else(
        || {
            (
                None,
                None,
                None,
                "gzip".to_owned(),
                "parentbased_always_on".to_owned(),
            )
        },
        |config| {
            (
                Some(config_signal(config.traces)),
                Some(config_signal(config.logs)),
                Some(config_signal(config.metrics)),
                config.compression.to_owned(),
                config.sampler.to_owned(),
            )
        },
    );
    jackin_protocol::control::TelemetryHealthReport {
        fingerprint: jackin_protocol::control::SanitizedConfigFingerprint {
            traces,
            logs,
            metrics,
            compression,
            sampler,
            active_signals: health.active_signals,
            service_name: "jackin-capsule".to_owned(),
            app_mode: "capsule".to_owned(),
        },
        config_failure,
        health: jackin_protocol::control::TelemetryHealthSnapshot {
            active_signals: health.active_signals,
            traces: signal(health.traces),
            logs: signal(health.logs),
            metrics: signal(health.metrics),
            facade_rejections: health.facade_rejections,
            capsule_export,
            flush,
            shutdown_completed: health.shutdown_completed,
            shutdown_succeeded: health.shutdown_succeeded,
            shutdown_timed_out: health.shutdown_timed_out,
        },
    }
}

const fn telemetry_config_failure(
    failure: jackin_diagnostics::TelemetryConfigFailure,
) -> jackin_protocol::control::TelemetryConfigFailure {
    use jackin_diagnostics::TelemetryConfigFailure as Source;
    use jackin_protocol::control::TelemetryConfigFailure as Target;

    match failure {
        Source::MissingSignalEndpoint => Target::MissingSignalEndpoint,
        Source::UnsupportedProtocol => Target::UnsupportedProtocol,
        Source::ConflictingSampler => Target::ConflictingSampler,
        Source::UnsupportedCompression => Target::UnsupportedCompression,
        Source::InvalidTimeout => Target::InvalidTimeout,
        Source::InvalidHeaders => Target::InvalidHeaders,
        Source::InvalidResourceAttributes => Target::InvalidResourceAttributes,
        Source::InvalidEndpoint => Target::InvalidEndpoint,
        Source::EmptyValue => Target::EmptyValue,
        Source::IncompleteClientIdentity => Target::IncompleteClientIdentity,
    }
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
                s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if !mux.dialog_captures_input()
                && let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.session_supervisor.sessions.get(focused)
                && s.focus_events_enabled()
            {
                s.send_input(b"\x1b[O");
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
    if matches!(
        jackin_telemetry::propagation::extract(&request.context),
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        let _error = jackin_telemetry::record_error(RPC_ERROR);
        send_attach_control_response(
            mux,
            request_id,
            AttachControlResult::InvalidCorrelation,
            None,
        );
        return;
    }
    if handle_pending_clipboard_transfer(mux, &request) {
        return;
    }

    let method = match request.operation {
        AttachControlOperation::Detach => "jackin.capsule.Attach/Detach",
        AttachControlOperation::FocusIn | AttachControlOperation::FocusOut => {
            "jackin.capsule.Attach/Focus"
        }
        _ => "jackin.capsule.Attach/ClipboardImageTransfer",
    };
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
                    super::PendingAttachControl {
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

#[cfg(test)]
mod tests;
