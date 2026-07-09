use super::*;
use jackin_protocol::control::{PaneSnapshot, TabSnapshot};

#[derive(Debug, Default)]
struct RecordingNotifier {
    notifications: Vec<AttentionNotification>,
}

impl AttentionNotifier for RecordingNotifier {
    fn notify(&mut self, notification: &AttentionNotification) -> Result<()> {
        self.notifications.push(notification.clone());
        Ok(())
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
fn control_socket_shape_accepts_attention_snapshot() {
    let request = DaemonRequest {
        id: "req-1".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        kind: DaemonRequestKind::AttentionSnapshot {
            container_name: "jk-agent-smith".to_owned(),
            panes: vec![pane(AgentState::Blocked)],
        },
    };
    let mut adapter = AttentionAdapter::new(RecordingNotifier::default());

    let response = handle_request_line(&serde_json::to_string(&request).unwrap(), &mut adapter);

    assert_eq!(
        response,
        DaemonResponse {
            id: "req-1".to_owned(),
            kind: DaemonResponseKind::AttentionAccepted { notifications: 1 },
        }
    );
    assert_eq!(adapter.into_notifier().notifications.len(), 1);
}

#[test]
fn protocol_version_mismatch_fails_closed() {
    let request = DaemonRequest {
        id: "req-2".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION + 1,
        kind: DaemonRequestKind::Hello,
    };
    let mut adapter = AttentionAdapter::new(RecordingNotifier::default());

    let response = handle_request_line(&serde_json::to_string(&request).unwrap(), &mut adapter);

    assert!(matches!(
        response.kind,
        DaemonResponseKind::Error { ref message }
            if message.contains("unsupported daemon protocol")
    ));
}
