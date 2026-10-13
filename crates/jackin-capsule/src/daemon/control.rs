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
use jackin_protocol::control::SessionSendRejection;
use jackin_telemetry::ResultTelemetryExt as _;

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

/// Whether a kernel-authenticated peer may open the interactive attach path.
///
/// The socket is intentionally still owner-only, but `DAC_OVERRIDE` makes file
/// modes insufficient. Every admitted session identity is therefore denied at
/// the protocol boundary. A non-admitted UID is an operator/host peer and
/// retains the existing attach flow.
pub(crate) fn attach_peer_is_authorized(mux: &Multiplexer, peer_uid: Option<u32>) -> bool {
    let Some(peer_uid) = peer_uid else {
        return false;
    };
    !configured_session_uid(mux, peer_uid)
}

/// Authorize control requests using the peer credential supplied by the
/// kernel, never a caller-controlled session id. Operator peers retain the
/// existing control surface. A session peer may address only its own live
/// session through target-scoped, non-administrative requests.
pub(crate) fn control_request_allowed(
    mux: &Multiplexer,
    peer_uid: Option<u32>,
    session_capability: Option<&str>,
    message: &ClientMsg,
) -> bool {
    let Some(peer_uid) = peer_uid else {
        return false;
    };

    // The in-container MCP/`jackin-exec` path has no target session field in
    // its wire shape. Infer exactly one authorized session from the kernel
    // peer UID plus its daemon-issued capability; never let a session peer
    // enter the operator/global credential picker without both.
    if matches!(message, ClientMsg::ExecCommand { .. }) {
        if peer_uid == 0 {
            return true;
        }
        return mux.session_supervisor.sessions.values().any(|session| {
            session.identity.uid == peer_uid
                && capability_matches(&session.control_capability, session_capability)
        });
    }

    if !configured_session_uid(mux, peer_uid) {
        return true;
    }

    let target = match message {
        ClientMsg::ReportRuntimeEvent { session_id, .. }
        | ClientMsg::StatusCapture { session_id }
        | ClientMsg::TokenUsage { session_id }
        | ClientMsg::SessionSend {
            session: session_id,
            ..
        } => Some(*session_id),
        ClientMsg::Events {
            session: Some(session_id),
        } => Some(*session_id),
        // Global inventory, usage, telemetry, execution, and unfiltered event
        // streams are operator/admin surfaces. Session peers never receive
        // them, even when the message has no obvious secret fields.
        ClientMsg::Events { session: None }
        | ClientMsg::TelemetryHealth
        | ClientMsg::Status
        | ClientMsg::Snapshot
        | ClientMsg::Agents
        | ClientMsg::UsageFocused
        | ClientMsg::UsageRefreshFocused
        | ClientMsg::UsageAccountList
        | ClientMsg::ExecCommand { .. }
        | ClientMsg::Unknown => None,
    };

    target.is_some_and(|session_id| {
        mux.session_supervisor
            .sessions
            .get(session_id)
            .is_some_and(|session| {
                session.identity.uid == peer_uid
                    && capability_matches(&session.control_capability, session_capability)
            })
    })
}

/// Compare the fixed-format session bearer without an early exit on the
/// secret bytes. The socket is local, but keeping the comparison uniform costs
/// nothing and avoids turning the authorization branch into a token oracle.
fn capability_matches(expected: &str, presented: Option<&str>) -> bool {
    let Some(presented) = presented else {
        return false;
    };
    let expected = expected.as_bytes();
    let presented = presented.as_bytes();
    let max = expected.len().max(presented.len());
    let mut difference = expected.len() ^ presented.len();
    for index in 0..max {
        let left = expected.get(index).copied().unwrap_or(0);
        let right = presented.get(index).copied().unwrap_or(0);
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

fn configured_session_uid(mux: &Multiplexer, peer_uid: u32) -> bool {
    mux.launch_env
        .launch_config
        .instance_identities
        .values()
        .any(|identity| identity.uid == peer_uid)
        || mux
            .launch_env
            .launch_config
            .shell_identity
            .is_some_and(|identity| identity.uid == peer_uid)
}

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
/// # Errors
///
/// Returns an error when the capture directory or either fixture file cannot
/// be created or written.
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
        // `session.send`: write the payload into the addressed PTY verbatim.
        // Input never authors state — the terminal-observation arbitration
        // decides what the agent is doing — so this only records the input as
        // recency evidence and reports what it wrote.
        ClientMsg::SessionSend { session, text } => {
            let bytes = text.len() as u64;
            let mut unblocked = false;
            let outcome = match mux.session_supervisor.sessions.get_mut(session) {
                None => {
                    let _error = jackin_telemetry::record_error(RPC_ERROR);
                    Err(SessionSendRejection::UnknownSession)
                }
                Some(target) if !target.send_input(text.as_bytes()) => {
                    let _error = jackin_telemetry::record_error(RPC_ERROR);
                    Err(SessionSendRejection::WriterClosed)
                }
                Some(target) => {
                    unblocked = target.mark_operator_input();
                    Ok(bytes)
                }
            };
            // Same chrome refresh the keyboard path takes: input that clears a
            // latched blocked pane must repaint it, whether an operator typed
            // it or a host client sent it.
            if let Some(reason) = crate::tui::update::pane_data_redraw_reason(false, unblocked) {
                mux.invalidate(reason);
            }
            super::events::session_send_reply(session, outcome)
        }
        ClientMsg::Events { .. } => {
            // Defensive only: `Events` is a subscription, intercepted before
            // this synchronous path by `handle_control_request` (it registers
            // the stream instead of producing one reply). Fail closed if a
            // future caller ever routes it here.
            let _error = jackin_telemetry::record_error(RPC_ERROR);
            ServerMsg::Unknown
        }
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
