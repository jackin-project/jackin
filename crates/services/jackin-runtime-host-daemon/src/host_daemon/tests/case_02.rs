// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn telemetry_health_round_trip_is_typed_and_sanitized() {
    let (_temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    // The diagnostics lifecycle is process-global and other tests exercise
    // its flush/shutdown transitions. Capture the pre-request state instead
    // of assuming this test owns a fresh process.
    let health_before = jackin_diagnostics::telemetry_health_snapshot();
    // Scope spans privately: without a thread-local subscriber they reach
    // the process-global wire exporter owned by the concurrently running
    // wire conformance test.
    let (_export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
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
    // Scope spans privately: without a thread-local subscriber they reach
    // the process-global wire exporter owned by the concurrently running
    // wire conformance test.
    let (_export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
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
    // Scope spans privately: without a thread-local subscriber they reach
    // the process-global wire exporter owned by the concurrently running
    // wire conformance test.
    let (_export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
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
