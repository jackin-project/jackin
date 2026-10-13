// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn notification_dispatch_exports_spawn_failure_without_command_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let mut dispatcher = StdNotificationDispatcher;
        dispatcher
            .dispatch(&NotificationCommand {
                program: "operator-secret-missing-notifier".into(),
                args: vec!["operator-secret-notification-body".into()],
            })
            .unwrap_err();
    });
    export.force_flush();

    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(!export.contains_span_text("operator-secret-missing-notifier"));
    assert!(!export.contains_span_text("operator-secret-notification-body"));
}

#[test]
fn conformance_serialized_daemon_propagation_matrix_preserves_parentage_sampling_and_rejection() {
    let trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";
    let parent_id = "00f067aa0ba902b7";
    let mut sampled = TelemetryContext::v1();
    sampled.traceparent = Some(format!("00-{trace_id}-{parent_id}-01"));
    let spans = serialized_daemon_spans(sampled);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].trace_id, trace_id);
    assert_eq!(spans[0].parent_span_id, parent_id);
    assert!(spans[0].sampled);

    for context in [
        TelemetryContext::v1(),
        TelemetryContext {
            traceparent: Some("malformed".to_owned()),
            ..TelemetryContext::v1()
        },
    ] {
        let spans = serialized_daemon_spans(context);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].parent_span_id, "0000000000000000");
    }

    let mut unsampled = TelemetryContext::v1();
    unsampled.traceparent = Some(format!("00-{trace_id}-{parent_id}-00"));
    assert!(serialized_daemon_spans(unsampled).is_empty());

    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let bad_id = DaemonRequest {
        id: "bad-id".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext {
            invocation_id: Some("not-a-uuid".to_owned()),
            ..TelemetryContext::v1()
        },
        kind: DaemonRequestKind::AttentionSnapshot {
            container_name: "must-not-notify".to_owned(),
            panes: Vec::new(),
        },
    };
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let response = handle_request_line(
        &serde_json::to_string(&bad_id).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );
    assert!(matches!(response.kind, DaemonResponseKind::Error { .. }));
    assert!(attention.notifier.notifications.is_empty());
    let malformed = handle_request_line(
        "{not-json",
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );
    assert!(matches!(malformed.kind, DaemonResponseKind::Error { .. }));
    drop(guard);
    export.force_flush();
    assert!(export.finished_spans().is_empty());
}

#[test]
fn daemon_socket_exports_client_parent_server_and_completes_after_response_write() {
    let (_temp, _paths, layout) = layout();
    ensure_run_dir(&layout).unwrap();
    let listener = UnixListener::bind(&layout.socket_path).expect("bind daemon socket");
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    let server_layout = layout.clone();
    let server = std::thread::spawn(move || {
        tracing::dispatcher::with_default(&dispatcher, || {
            let (mut stream, _) = listener.accept().expect("accept daemon client");
            let mut attention = AttentionAdapter::new(RecordingNotifier::default());
            handle_stream(
                &mut stream,
                &server_layout,
                "test-build",
                &CoredumpPolicy::Disabled,
                &mut attention,
            )
            .expect("serve daemon request")
        })
    });

    let response = request(&layout.socket_path, "test-build", DaemonRequestKind::Status)
        .expect("daemon request");
    assert!(matches!(response.kind, DaemonResponseKind::Status(_)));
    server.join().expect("server thread");
    drop(guard);
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 3);
    let client = spans
        .iter()
        .find(|span| span.name == "rpc.client")
        .expect("client span");
    let server = spans
        .iter()
        .find(|span| span.name == "rpc.server")
        .expect("server span");
    let connection = spans
        .iter()
        .find(|span| span.name == "connection.attempt")
        .expect("connection attempt span");
    assert_eq!(server.trace_id, client.trace_id);
    assert_eq!(server.parent_span_id, client.span_id);
    assert_eq!(connection.trace_id, client.trace_id);
    assert_eq!(connection.parent_span_id, client.span_id);
    assert!(!client.error && !server.error && !connection.error);
}

#[test]
fn conformance_wire_real_daemon_socket_exports_bounded_parented_rpc() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::DAEMON,
    )?;
    let (temp, _paths, layout) = layout();
    ensure_run_dir(&layout)?;
    let listener = UnixListener::bind(&layout.socket_path)?;
    let server_layout = layout.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept daemon client");
        let mut attention = AttentionAdapter::new(RecordingNotifier::default());
        handle_stream(
            &mut stream,
            &server_layout,
            "wire-private-daemon-build",
            &CoredumpPolicy::Disabled,
            &mut attention,
        )
        .expect("serve daemon request")
    });

    let response = request(
        &layout.socket_path,
        "wire-private-daemon-build",
        DaemonRequestKind::Status,
    )?;
    assert!(matches!(response.kind, DaemonResponseKind::Status(_)));
    server.join().expect("server thread");
    jackin_diagnostics::flush_wire_test_export()?;

    // Flush acknowledges spans already owned by the providers. Final shutdown
    // owns the providers and their runtime, including the exporter-owned
    // physical-channel connection span created while flushing.
    jackin_diagnostics::shutdown_capsule_tracing();

    // Wait for the complete owned export set before asserting exactness. The
    // deadline is only a failure detector; no span filtering or count
    // relaxation is allowed here.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let spans = runtime.block_on(async {
        loop {
            let spans = testbed.spans();
            if spans.len() == 4 {
                break spans;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "daemon RPC/export wire spans did not arrive exactly once: got {} ({:?})",
                spans.len(),
                spans
                    .iter()
                    .map(|span| span.name.as_str())
                    .collect::<Vec<_>>()
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });
    let client = spans
        .iter()
        .find(|span| span.name == "rpc.client")
        .expect("client span");
    let server_span = spans
        .iter()
        .find(|span| span.name == "rpc.server")
        .expect("server span");
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "rpc.client")
            .count(),
        1
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "rpc.server")
            .count(),
        1
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "connection.attempt")
            .count(),
        2,
        "one daemon and one exporter connection attempt are owned by this export"
    );
    let connection = spans
        .iter()
        .find(|span| span.name == "connection.attempt" && span.trace_id == client.trace_id)
        .expect("connection span");
    assert_eq!(server_span.trace_id, client.trace_id);
    assert_eq!(server_span.parent_span_id, client.span_id);
    assert_eq!(connection.trace_id, client.trace_id);
    assert_eq!(connection.parent_span_id, client.span_id);
    assert!(spans.iter().any(|span| {
        span.name == "connection.attempt"
            && span.trace_id != client.trace_id
            && span.parent_span_id.is_empty()
    }));
    let wire_text = format!("{spans:?}");
    for expected in ["rpc.client", "rpc.server", "connection.attempt", "status"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let private_root = temp.path().to_string_lossy().into_owned();
    let prohibited = ["wire-private-daemon-build", private_root.as_str()];
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    Ok(())
}

#[test]
fn daemon_socket_marks_server_failure_when_peer_closes_before_response() {
    use std::net::Shutdown;
    use std::os::fd::AsFd;

    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut context = TelemetryContext::v1();
    context.traceparent =
        Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned());
    let request = DaemonRequest {
        id: "write-failure".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: context,
        kind: DaemonRequestKind::Status,
    };
    let (mut client, mut server) = UnixStream::pair().expect("daemon socket pair");
    serde_json::to_writer(&mut client, &request).expect("write daemon request");
    client.write_all(b"\n").expect("terminate daemon request");
    // WHY: Shutdown::Both alone lets macOS accept short response writes into the
    // kernel buffer (write "succeeds", test flakes). SO_LINGER=0 RST forces EPIPE
    // on the peer write path portably.
    let linger = nix::libc::linger {
        l_onoff: 1,
        l_linger: 0,
    };
    nix::sys::socket::setsockopt(&client.as_fd(), nix::sys::socket::sockopt::Linger, &linger)
        .expect("SO_LINGER");
    client
        .shutdown(Shutdown::Both)
        .expect("close daemon client");
    drop(client);
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    handle_stream(
        &mut server,
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    )
    .expect_err("closed client must fail the daemon response write");
    drop(guard);
    export.force_flush();
    assert_eq!(export.error_span_count(), 1);
}

#[test]
fn daemon_layout_uses_private_run_dir() {
    let (_temp, _paths, layout) = layout();

    ensure_run_dir(&layout).unwrap();

    let mode = fs::metadata(&layout.run_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    assert_eq!(layout.socket_path, layout.run_dir.join(SOCKET_FILE_NAME));
}

#[test]
fn hello_reports_protocol_without_adapters() {
    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    // Scope spans privately: without a thread-local subscriber they reach
    // the process-global wire exporter owned by the concurrently running
    // wire conformance test.
    let (_export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let request = DaemonRequest {
        id: "r1".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind: DaemonRequestKind::Hello,
    };

    let response = handle_request_line(
        &serde_json::to_string(&request).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );

    assert_eq!(
        response,
        DaemonResponse {
            id: "r1".to_owned(),
            kind: DaemonResponseKind::Hello {
                protocol_version: DAEMON_PROTOCOL_VERSION,
                build_id: "test-build".to_owned(),
                capabilities: Vec::new(),
            },
        }
    );
}
