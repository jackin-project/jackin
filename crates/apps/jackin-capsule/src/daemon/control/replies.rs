// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Control replies, status capture, and telemetry health reports.

use super::super::{
    ClientMsg, Instant, Multiplexer, PathBuf, Result, ServerMsg, Session, TokenTotals,
};
use super::RPC_ERROR;
use jackin_core::container_paths;
use jackin_protocol::attach::{AttachControlResponse, AttachControlResult};
use jackin_protocol::control::SessionSendRejection;
use jackin_telemetry::ResultTelemetryExt as _;

pub(crate) fn send_attach_control_response(
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
            use super::super::ports::{ControlPort, PORTS, RuntimeEvent};
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
            super::super::events::session_send_reply(session, outcome)
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
