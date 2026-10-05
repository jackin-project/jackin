use super::*;
use jackin_protocol::control::{AgentState, PaneSnapshot, TabSnapshot};

mod wire_overlap;

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
    let overlap = wire_overlap::ParentOverlap::start_if_requested()?;
    // A global subscriber owns the whole process, including concurrent tests.
    // Give the real exporter its own process so every received span is ours.
    const WIRE_CHILD: &str = "JACKIN_DAEMON_WIRE_TEST_CHILD";
    if std::env::var_os(WIRE_CHILD).is_none() {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .arg("--exact")
            .arg("host_daemon::tests::conformance_wire_real_daemon_socket_exports_bounded_parented_rpc")
            .arg("--nocapture")
            .env(WIRE_CHILD, "1");
        if let Some(overlap) = overlap.as_ref() {
            command.env(wire_overlap::ENDPOINT, overlap.endpoint());
        }
        let status = command.status()?;
        if let Some(overlap) = overlap {
            overlap.finish()?;
        }
        anyhow::ensure!(status.success(), "isolated daemon wire test failed");
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::DAEMON,
    )?;
    wire_overlap::collector_ready(overlap.as_ref())?;
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
    if let Some(overlap) = overlap {
        overlap.finish()?;
    }
    Ok(())
}

#[test]
fn daemon_wire_fixture_isolates_unrelated_production_request() -> Result<()> {
    wire_overlap::assert_isolated_collector()
}

#[test]
fn daemon_response_write_failure_marks_server_failure() {
    assert_daemon_response_write_failure(false, "writing daemon response");
}

#[test]
fn daemon_response_terminator_failure_marks_server_failure() {
    assert_daemon_response_write_failure(true, "terminating daemon response");
}

fn assert_daemon_response_write_failure(reject_terminator: bool, expected_context: &str) {
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
    let mut wire = serde_json::to_vec(&request).expect("serialize daemon request");
    wire.push(b'\n');
    // AF_UNIX peer closure and SO_LINGER do not guarantee an immediate failed
    // write on macOS. Inject the write error at the actual response boundary.
    struct BrokenResponseWriter {
        reject_terminator: bool,
        accepted: Vec<u8>,
    }
    impl Write for BrokenResponseWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if !self.reject_terminator || bytes == b"\n" {
                return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
            }
            self.accepted.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = BrokenResponseWriter {
        reject_terminator,
        accepted: Vec::new(),
    };
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let error = handle_request_io(
        wire.as_slice(),
        &mut writer,
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        &mut attention,
    )
    .expect_err("failed daemon response write must propagate");
    assert_eq!(error.to_string(), expected_context);
    if reject_terminator {
        let response: DaemonResponse = serde_json::from_slice(&writer.accepted)
            .expect("complete JSON response precedes the failed terminator");
        assert_eq!(response.id, request.id);
        assert!(matches!(response.kind, DaemonResponseKind::Status(_)));
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::BrokenPipe
        );
    } else {
        assert!(writer.accepted.is_empty());
        assert_eq!(
            error
                .downcast_ref::<serde_json::Error>()
                .unwrap()
                .io_error_kind(),
            Some(std::io::ErrorKind::BrokenPipe)
        );
    }
    drop(guard);
    export.force_flush();
    let spans = export.finished_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "rpc.server");
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
    // The diagnostics lifecycle is process-global and other tests exercise
    // its flush/shutdown transitions. Capture the pre-request state instead
    // of assuming this test owns a fresh process.
    let health_before = jackin_diagnostics::telemetry_health_snapshot();
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
    let expected_flush = match health_before.flush {
        jackin_diagnostics::TelemetryFlushStatus::Pending => TelemetryFlushStatus::Pending,
        jackin_diagnostics::TelemetryFlushStatus::Succeeded => TelemetryFlushStatus::Succeeded,
        jackin_diagnostics::TelemetryFlushStatus::Failed => TelemetryFlushStatus::Failed,
    };
    assert_eq!(report.health.flush, expected_flush);
    assert_eq!(
        report.health.shutdown_timed_out,
        health_before.shutdown_timed_out
    );
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

#[test]
fn linux_notify_args_put_hostile_positionals_after_sentinel() {
    let args = linux_notify_args("--version\n", "body\ttext");

    assert_eq!(args, ["--", "--version", "bodytext"]);
}

#[test]
fn linux_notify_args_strip_controls_and_clamp_by_character() {
    let long_title = format!("{}\r\n", "界".repeat(201));
    let long_body = format!("{}\u{0007}", "b".repeat(201));

    let args = linux_notify_args(&long_title, &long_body);

    assert_eq!(args.len(), 3);
    assert_eq!(args[1].chars().count(), 200);
    assert_eq!(args[2].chars().count(), 200);
    assert!(
        args[1..]
            .iter()
            .all(|value| !value.chars().any(char::is_control))
    );
}

fn snapshot(state: AgentState) -> InstanceSnapshot {
    InstanceSnapshot {
        active_tab: 0,
        tabs: vec![TabSnapshot {
            label: "agent".to_owned(),
            instance: Some("codex-main".to_owned()),
            account_id: Some("acc-1".to_owned()),
            focused_pane: 7,
            panes: vec![PaneSnapshot {
                session_id: 7,
                label: "Codex".to_owned(),
                agent: Some("codex".to_owned()),
                account_id: Some("acc-1".to_owned()),
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
