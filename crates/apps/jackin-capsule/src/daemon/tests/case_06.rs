// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{ClientMsg, ServerMsg};

#[tokio::test]
async fn control_reply_report_runtime_event_applies_to_session_and_acks() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let id = super::spawn_sleep_session(&mut mux);
    let reply = super::super::control_reply_for_request(
        &mut mux,
        ClientMsg::ReportRuntimeEvent {
            session_id: id,
            source_id: "hook".to_owned(),
            runtime: "opencode".to_owned(),
            event: "tool.execute.before".to_owned(),
            payload: None,
        },
    );
    assert!(matches!(reply, ServerMsg::Ack));
    let session = mux
        .session_supervisor
        .sessions
        .get(id)
        .expect("spawned session");
    assert!(session.authority.is_some());
    super::terminate_session(&mut mux, id);
}

#[test]
fn control_reply_runtime_event_and_capture_for_unknown_session_still_ack() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let reply = super::super::control_reply_for_request(
        &mut mux,
        ClientMsg::ReportRuntimeEvent {
            session_id: 999_999,
            source_id: "hook".to_owned(),
            runtime: "opencode".to_owned(),
            event: "tool.execute.before".to_owned(),
            payload: None,
        },
    );
    assert!(matches!(reply, ServerMsg::Ack));
    let reply = super::super::control_reply_for_request(
        &mut mux,
        ClientMsg::StatusCapture {
            session_id: 999_999,
        },
    );
    assert!(matches!(reply, ServerMsg::Ack));
}
