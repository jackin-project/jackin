// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[derive(Debug, Default)]
pub(super) struct RecordingNotifier {
    pub(super) notifications: Vec<AttentionNotification>,
    pub(super) muted: bool,
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
pub(super) struct RecordingDispatcher {
    pub(super) commands: Vec<NotificationCommand>,
}

impl NotificationDispatcher for RecordingDispatcher {
    fn dispatch(&mut self, command: &NotificationCommand) -> Result<()> {
        self.commands.push(command.clone());
        Ok(())
    }
}

pub(super) fn layout() -> (tempfile::TempDir, JackinPaths, DaemonLayout) {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let layout = DaemonLayout::new(&paths);
    (temp, paths, layout)
}

pub(super) fn serialized_daemon_spans(
    context: TelemetryContext,
) -> Vec<jackin_diagnostics::TestSpanSnapshot> {
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

pub(super) fn snapshot(state: AgentState) -> InstanceSnapshot {
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

pub(super) fn pane(state: AgentState) -> AttentionPaneStatus {
    AttentionPaneStatus {
        session_id: 7,
        label: "Codex".to_owned(),
        agent: Some("codex".to_owned()),
        state,
    }
}
