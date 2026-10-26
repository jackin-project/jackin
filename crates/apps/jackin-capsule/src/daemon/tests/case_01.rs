// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn provider_probe_noop_tick_exports_no_span() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    record_skipped_provider_probe();
    drop(guard);
    export.force_flush();
    assert!(export.finished_spans().is_empty());
}

#[test]
fn quiet_agent_status_pass_exports_no_span() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    tracing::subscriber::with_default(subscriber, || {
        record_agent_status_tick(
            &session,
            crate::session::StatusTick {
                transition: None,
                stuck: false,
                flap: false,
            },
        );
    });
    export.force_flush();
    assert!(export.finished_spans().is_empty());
    assert!(
        !export.contains_span_text(jackin_telemetry::schema::attrs::JOB_ID),
        "metric-only cycle ticks must never acquire job identity"
    );
}

#[test]
fn substantive_agent_status_pass_exports_one_autonomous_root() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    tracing::subscriber::with_default(subscriber, || {
        record_agent_status_tick(
            &session,
            crate::session::StatusTick {
                transition: Some(crate::session::StatusTransition {
                    previous: crate::protocol::AgentState::Unknown,
                    effective: crate::protocol::AgentState::Working,
                    winner: crate::agent_status::evidence::EvidenceWinner::Unknown,
                }),
                stuck: false,
                flap: false,
            },
        );
    });
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans[0].name,
        jackin_telemetry::schema::spans::BACKGROUND_CYCLE
    );
    assert_eq!(spans[0].parent_span_id, "0000000000000000");
    assert!(export.contains_span_text("agent_status"));
    assert!(
        !export.contains_span_text(jackin_telemetry::schema::attrs::JOB_ID),
        "substantive cycles must not carry job.id"
    );
    assert_eq!(
        export.event_count(jackin_telemetry::schema::events::AGENT_STATE_CHANGED),
        1
    );
    assert_eq!(
        export.traced_event_count(jackin_telemetry::schema::events::AGENT_STATE_CHANGED),
        1
    );
}

#[test]
fn watchdog_demotion_without_transition_is_still_substantive() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    tracing::subscriber::with_default(subscriber, || {
        record_agent_status_tick(
            &session,
            crate::session::StatusTick {
                transition: None,
                stuck: true,
                flap: false,
            },
        );
    });
    export.force_flush();
    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(
        export.event_count(jackin_telemetry::schema::events::AGENT_STATE_CHANGED),
        0
    );
}

#[test]
fn conformance_serialized_control_propagation_matrix_preserves_parentage_and_rejection() {
    let trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";
    let parent_id = "00f067aa0ba902b7";
    let mut sampled = jackin_protocol::TelemetryContext::v1();
    sampled.traceparent = Some(format!("00-{trace_id}-{parent_id}-01"));
    let (accepted, spans) = serialized_control_spans(sampled);
    assert!(accepted);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].trace_id, trace_id);
    assert_eq!(spans[0].parent_span_id, parent_id);
    assert!(spans[0].sampled);

    for context in [
        jackin_protocol::TelemetryContext::v1(),
        jackin_protocol::TelemetryContext {
            traceparent: Some("malformed".to_owned()),
            ..jackin_protocol::TelemetryContext::v1()
        },
    ] {
        let (accepted, spans) = serialized_control_spans(context);
        assert!(accepted);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].parent_span_id, "0000000000000000");
    }

    let mut unsampled = jackin_protocol::TelemetryContext::v1();
    unsampled.traceparent = Some(format!("00-{trace_id}-{parent_id}-00"));
    let (accepted, spans) = serialized_control_spans(unsampled);
    assert!(accepted);
    assert!(
        spans.is_empty(),
        "unsampled remote parent must suppress export"
    );

    let bad_id = jackin_protocol::TelemetryContext {
        invocation_id: Some("not-a-uuid".to_owned()),
        ..jackin_protocol::TelemetryContext::v1()
    };
    let (accepted, spans) = serialized_control_spans(bad_id);
    assert!(!accepted);
    assert!(spans.is_empty());

    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Codex", 1, "test");
    let wire = serde_json::to_vec(&jackin_protocol::control::ControlRequest {
        ctx: jackin_protocol::TelemetryContext {
            invocation_id: Some("not-a-uuid".to_owned()),
            ..jackin_protocol::TelemetryContext::v1()
        },
        session_capability: None,
        msg: ClientMsg::UsageRefreshFocused,
    })
    .unwrap();
    let decoded: jackin_protocol::control::ControlRequest = serde_json::from_slice(&wire).unwrap();
    if control_server_operation(&decoded.ctx, &decoded.msg).is_ok() {
        control_reply_for_request(&mut mux, decoded.msg);
    }
    assert!(
        mux.usage.pending_usage_refresh.is_none(),
        "rejected correlation must not queue the provider refresh side effect"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_real_capsule_control_status_preserves_parent_and_delivery() {
    if crate::process_telemetry::run_wire_test_in_child(
        "daemon::tests::case_01::conformance_wire_real_capsule_control_status_preserves_parent_and_delivery",
        "JACKIN_DAEMON_CONTROL_WIRE_CHILD",
    )
    .expect("dispatch isolated daemon control wire test")
    {
        return;
    }
    let _telemetry_guard = crate::support::telemetry_test_guard_async().await;
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");
    let trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";
    let parent_id = "00f067aa0ba902b7";
    let request = jackin_protocol::control::ControlRequest {
        ctx: jackin_protocol::TelemetryContext {
            traceparent: Some(format!("00-{trace_id}-{parent_id}-01")),
            ..jackin_protocol::TelemetryContext::v1()
        },
        session_capability: None,
        msg: ClientMsg::Status,
    };
    let wire = serde_json::to_vec(&request).expect("serialize control request");
    let decoded: jackin_protocol::control::ControlRequest =
        serde_json::from_slice(&wire).expect("decode control request");
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let mut mux = single_pane_tab_mux();

    handle_control_request(
        &mut mux,
        ControlRequest {
            ctx: decoded.ctx,
            session_capability: decoded.session_capability,
            msg: decoded.msg,
            peer_uid: 0,
            reply: crate::attach_protocol::ControlReply::Once(reply_tx),
        },
    );
    let response = reply_rx.await.expect("receive control response");
    assert!(matches!(response.msg, ServerMsg::SessionList { .. }));
    response.complete(&Ok(()));
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = Instant::now() + Duration::from_secs(2);
    let spans = loop {
        let spans = testbed
            .spans()
            .into_iter()
            .filter(|span| span.name == "rpc.server")
            .collect::<Vec<_>>();
        if spans.len() == 1 {
            break spans;
        }
        assert!(
            Instant::now() < deadline,
            "Capsule control wire span did not arrive exactly once"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(spans[0].trace_id, hex::decode(trace_id).unwrap());
    assert_eq!(spans[0].parent_span_id, hex::decode(parent_id).unwrap());
    let wire_text = format!("{spans:?}");
    for expected in ["jackin", "status", "success"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

#[test]
fn conformance_exec_command_rpc_spans_exclude_command_and_args() {
    let command_secret = "PRIVATE_EXECUTABLE_PAYLOAD";
    let argument_secret = "PRIVATE_ARGUMENT_PAYLOAD";
    let request = jackin_protocol::control::ControlRequest {
        ctx: jackin_protocol::TelemetryContext::v1(),
        session_capability: None,
        msg: ClientMsg::ExecCommand {
            command: command_secret.to_owned(),
            args: vec![argument_secret.to_owned()],
        },
    };
    let wire = serde_json::to_vec(&request).unwrap();
    let decoded: jackin_protocol::control::ControlRequest = serde_json::from_slice(&wire).unwrap();
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    if let Ok(Some(server)) = control_server_operation(&decoded.ctx, &decoded.msg) {
        server.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    }
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(decoded.msg.rpc_method()),
        },
    ];
    if let Ok(client) =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs)
    {
        client.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    }
    drop(guard);
    export.force_flush();
    assert!(!export.contains_span_text(command_secret));
    assert!(!export.contains_span_text(argument_secret));
}

#[test]
fn conformance_invalid_attach_control_has_no_detach_side_effect() {
    let mut mux = test_mux(24, 80);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();
    mux.client_registry
        .client
        .attach_with_completions(out_tx, completion_tx);
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        handle_client_frame(
            &mut mux,
            ClientFrame::AttachControl(jackin_protocol::attach::AttachControlRequest {
                request_id: 77,
                context: jackin_protocol::TelemetryContext {
                    invocation_id: Some("not-a-uuid".to_owned()),
                    ..jackin_protocol::TelemetryContext::v1()
                },
                operation: jackin_protocol::attach::AttachControlOperation::Detach,
            }),
        );
        completion_rx
            .try_recv()
            .expect("rejection must retain its RPC owner through delivery")
            .complete(&Ok(()));
    });
    export.force_flush();
    assert!(!mux.client_registry.detach_requested);
    let encoded = out_rx
        .try_recv()
        .expect("rejection response must be queued");
    let response = jackin_protocol::attach::decode_server(encoded[0], encoded[5..].to_vec())
        .expect("response must decode");
    assert_eq!(
        response,
        ServerFrame::AttachControlResponse(jackin_protocol::attach::AttachControlResponse {
            request_id: 77,
            result: jackin_protocol::attach::AttachControlResult::InvalidCorrelation,
        })
    );
    let spans = export.finished_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "rpc.server");
    assert!(spans[0].error);
    assert_eq!(export.typed_error_count("error.typed", "rpc_error"), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn conformance_invalid_attach_handshake_has_one_typed_write_owner() {
    let (mut client, mut server) = UnixStream::pair().expect("attach socket pair");
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    reject_invalid_attach_handshake(&mut server).await;
    let mut tag = [0_u8; 1];
    client.read_exact(&mut tag).await.expect("shutdown tag");
    let response = read_server_frame(&mut client, tag[0])
        .await
        .expect("read shutdown")
        .expect("shutdown frame");
    drop(guard);
    export.force_flush();

    assert!(matches!(
        response,
        ServerFrame::Shutdown { reason: Some(reason) } if reason == "invalid correlation"
    ));
    let spans = export.finished_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "rpc.server");
    assert!(spans[0].error);
    assert_eq!(export.typed_error_count("error.typed", "rpc_error"), 1);
}
