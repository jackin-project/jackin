use super::*;
use jackin_protocol::control::{AgentState, PaneSnapshot, TabSnapshot};

#[derive(Debug, Default)]
struct RecordingNotifier {
    notifications: Vec<AttentionNotification>,
    muted: bool,
}

impl AttentionNotifier for RecordingNotifier {
    fn notify(&mut self, notification: &AttentionNotification) -> Result<()> {
        self.notifications.push(notification.clone());
        Ok(())
    }

    fn muted(&self) -> bool {
        self.muted
    }
}

#[derive(Debug, Default)]
struct RecordingDispatcher {
    commands: Vec<NotificationCommand>,
}

impl NotificationDispatcher for RecordingDispatcher {
    fn dispatch(&mut self, command: &NotificationCommand) -> Result<()> {
        self.commands.push(command.clone());
        Ok(())
    }
}

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

fn layout() -> (tempfile::TempDir, JackinPaths, DaemonLayout) {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let layout = DaemonLayout::new(&paths);
    (temp, paths, layout)
}

fn serialized_daemon_spans(context: TelemetryContext) -> Vec<jackin_diagnostics::TestSpanSnapshot> {
    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let request = DaemonRequest {
        id: "matrix".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: context,
        kind: DaemonRequestKind::Status,
    };
    let wire = serde_json::to_string(&request).unwrap();
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let response = handle_request_line(
        &wire,
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );
    assert!(matches!(response.kind, DaemonResponseKind::Status(_)));
    drop(guard);
    export.force_flush();
    export.finished_spans()
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

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let spans = runtime.block_on(async {
        loop {
            let spans = testbed
                .spans()
                .into_iter()
                .filter(|span| {
                    matches!(
                        span.name.as_str(),
                        "rpc.client" | "rpc.server" | "connection.attempt"
                    )
                })
                .collect::<Vec<_>>();
            if spans.len() == 3 {
                break spans;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "daemon RPC wire spans did not arrive exactly once"
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
    let connection = spans
        .iter()
        .find(|span| span.name == "connection.attempt")
        .expect("connection span");
    assert_eq!(server_span.trace_id, client.trace_id);
    assert_eq!(server_span.parent_span_id, client.span_id);
    assert_eq!(connection.trace_id, client.trace_id);
    assert_eq!(connection.parent_span_id, client.span_id);
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
    jackin_diagnostics::shutdown_capsule_tracing();
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

#[test]
fn telemetry_health_round_trip_is_typed_and_sanitized() {
    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let request = DaemonRequest {
        id: "health".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind: DaemonRequestKind::TelemetryHealth,
    };
    let response = handle_request_line(
        &serde_json::to_string(&request).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );
    let DaemonResponseKind::TelemetryHealth(report) = response.kind else {
        panic!("expected typed telemetry health response");
    };
    assert_eq!(
        report.health.active_signals,
        report.fingerprint.active_signals
    );
    assert_eq!(report.fingerprint.service_name, "jackin-daemon");
    assert_eq!(report.fingerprint.app_mode, "daemon");
    assert_eq!(report.fingerprint.compression, "gzip");
    assert_eq!(report.fingerprint.sampler, "parentbased_always_on");
    assert_eq!(report.config_failure, None);
    assert_eq!(report.health.flush, TelemetryFlushStatus::Pending);
    assert!(!report.health.shutdown_timed_out);
    let json = serde_json::to_string(&report).unwrap().to_ascii_lowercase();
    assert!(!json.contains("authorization"));
    assert!(!json.contains("header"));
    assert!(!json.contains("certificate"));
}

#[test]
fn protocol_and_build_mismatch_fail_closed() {
    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let protocol = DaemonRequest {
        id: "proto".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION + 1,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind: DaemonRequestKind::Status,
    };
    let build = DaemonRequest {
        id: "build".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "old-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind: DaemonRequestKind::Status,
    };

    let response = handle_request_line(
        &serde_json::to_string(&protocol).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );
    assert!(matches!(
        response.kind,
        DaemonResponseKind::Error { ref message }
            if message.contains("unsupported daemon protocol")
    ));

    let response = handle_request_line(
        &serde_json::to_string(&build).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    );
    assert!(matches!(
        response.kind,
        DaemonResponseKind::Error { ref message }
            if message.contains("daemon build mismatch")
    ));
}

#[test]
fn attention_adapter_notifies_on_blocked_and_done_edges_only() {
    let mut adapter = AttentionAdapter::new(RecordingNotifier::default());

    assert_eq!(
        adapter
            .ingest_snapshot("jk-agent-smith", &snapshot(AgentState::Working))
            .unwrap(),
        0
    );
    assert_eq!(
        adapter
            .ingest_snapshot("jk-agent-smith", &snapshot(AgentState::Blocked))
            .unwrap(),
        1
    );
    assert_eq!(
        adapter
            .ingest_snapshot("jk-agent-smith", &snapshot(AgentState::Blocked))
            .unwrap(),
        0
    );
    assert_eq!(
        adapter
            .ingest_snapshot("jk-agent-smith", &snapshot(AgentState::Done))
            .unwrap(),
        1
    );

    let notifier = adapter.into_notifier();
    assert_eq!(notifier.notifications.len(), 2);
    assert_eq!(notifier.notifications[0].state, AgentState::Blocked);
    assert_eq!(notifier.notifications[1].state, AgentState::Done);
}

#[test]
fn attention_adapter_rejects_invalid_container_identity() {
    let mut adapter = AttentionAdapter::new(RecordingNotifier::default());

    let error = adapter
        .ingest_snapshot("invalid/container", &snapshot(AgentState::Blocked))
        .unwrap_err();

    assert!(error.to_string().contains("validating attention snapshot"));
}

#[test]
fn attention_snapshot_request_reports_muted_without_dispatch_count() {
    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier {
        muted: true,
        ..RecordingNotifier::default()
    });
    let request = DaemonRequest {
        id: "attention".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind: DaemonRequestKind::AttentionSnapshot {
            container_name: "jk-agent-smith".to_owned(),
            panes: vec![pane(AgentState::Blocked)],
        },
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
            id: "attention".to_owned(),
            kind: DaemonResponseKind::AttentionAccepted {
                notifications: 0,
                muted: true,
            },
        }
    );
    assert_eq!(attention.into_notifier().notifications.len(), 1);
}

#[test]
fn host_notifier_dispatches_command_when_enabled() {
    let dispatcher = RecordingDispatcher::default();
    let mut notifier = HostAttentionNotifier::new(dispatcher, true);

    notifier
        .notify(&AttentionNotification {
            container_name: "jk-agent-smith".to_owned(),
            session_id: 7,
            agent: Some("codex".to_owned()),
            label: "Codex".to_owned(),
            state: AgentState::Blocked,
        })
        .unwrap();

    assert_eq!(notifier.dispatcher.commands.len(), 1);
}

#[test]
fn host_notifier_is_quiet_when_muted() {
    let dispatcher = RecordingDispatcher::default();
    let mut notifier = HostAttentionNotifier::new(dispatcher, false);

    notifier
        .notify(&AttentionNotification {
            container_name: "jk-agent-smith".to_owned(),
            session_id: 7,
            agent: Some("codex".to_owned()),
            label: "Codex".to_owned(),
            state: AgentState::Done,
        })
        .unwrap();

    assert!(notifier.dispatcher.commands.is_empty());
}

#[test]
fn notification_command_uses_supported_host_backend() {
    let command = notification_command_for_host("Title", "Body");
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        assert!(command.is_some());
    } else {
        assert!(command.is_none());
    }
}

fn snapshot(state: AgentState) -> InstanceSnapshot {
    InstanceSnapshot {
        active_tab: 0,
        tabs: vec![TabSnapshot {
            label: "agent".to_owned(),
            focused_pane: 7,
            panes: vec![PaneSnapshot {
                session_id: 7,
                label: "Codex".to_owned(),
                agent: Some("codex".to_owned()),
                state,
                agent_status_report: None,
            }],
        }],
    }
}

fn pane(state: AgentState) -> AttentionPaneStatus {
    AttentionPaneStatus {
        session_id: 7,
        label: "Codex".to_owned(),
        agent: Some("codex".to_owned()),
        state,
    }
}

#[test]
fn unit_files_target_explicit_daemon_serve() {
    let (_temp, paths, _layout) = layout();
    let units = render_unit_files(&paths, Path::new("/bin/jackin"));

    assert!(units.launchd_plist.contains("<string>daemon</string>"));
    assert!(units.launchd_plist.contains("<string>serve</string>"));
    assert!(
        units
            .systemd_unit
            .contains("ExecStart=/bin/jackin daemon serve")
    );
    assert!(units.systemd_unit.contains("StandardOutput=null"));
    assert!(!units.systemd_unit.contains("jackin-daemon.log"));
}
