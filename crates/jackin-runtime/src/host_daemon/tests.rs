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

fn layout() -> (tempfile::TempDir, JackinPaths, DaemonLayout) {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let layout = DaemonLayout::new(&paths);
    (temp, paths, layout)
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
    let json = serde_json::to_string(&report).unwrap().to_ascii_lowercase();
    assert!(!json.contains("authorization"));
    assert!(!json.contains("header"));
    assert!(!json.contains("certificate"));
}

#[test]
fn endpoint_fingerprint_keeps_authority_only() {
    assert_eq!(
        endpoint_authority("https://token@example.test:4317/private/path"),
        Some("example.test:4317".to_owned())
    );
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
