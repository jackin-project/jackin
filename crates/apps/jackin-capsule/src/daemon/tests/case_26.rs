// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn take_control_of_an_unknown_session_denies_without_side_effects() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let reply = control_reply_for_request(&mut mux, ClientMsg::TakeControl { session: 999 });

    assert!(matches!(
        reply,
        ServerMsg::SessionSendDenied {
            session: 999,
            reason: jackin_protocol::control::SessionSendRejection::UnknownSession,
        }
    ));
    assert!(
        matches!(input_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "a misaddressed takeover must never reach another session's PTY"
    );
    assert!(
        !mux.session_supervisor
            .sessions
            .get(1)
            .expect("session is registered")
            .operator_owned(),
        "a misaddressed takeover must not flip another session's ownership"
    );
}

#[test]
fn automatic_send_is_rejected_while_an_operator_owns_the_session() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let reply = control_reply_for_request(&mut mux, ClientMsg::TakeControl { session: 1 });
    assert!(matches!(reply, ServerMsg::Ack));
    assert!(
        mux.session_supervisor
            .sessions
            .get(1)
            .expect("session is registered")
            .operator_owned(),
        "takeover must flag the session before the acknowledgement"
    );

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 1,
            text: "go\r".to_owned(),
        },
    );
    assert!(matches!(
        reply,
        ServerMsg::SessionSendDenied {
            session: 1,
            reason: jackin_protocol::control::SessionSendRejection::OperatorOwned,
        }
    ));
    assert!(
        matches!(input_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "rejected automatic input must never reach the PTY"
    );

    let reply = control_reply_for_request(&mut mux, ClientMsg::TakeControl { session: 1 });
    assert!(
        matches!(reply, ServerMsg::Ack),
        "a repeated takeover is idempotent while ownership is held"
    );
}

#[test]
fn release_control_reopens_automatic_input() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    drop(control_reply_for_request(
        &mut mux,
        ClientMsg::TakeControl { session: 1 },
    ));
    let reply = control_reply_for_request(&mut mux, ClientMsg::ReleaseControl { session: 1 });
    assert!(matches!(reply, ServerMsg::Ack));
    assert!(
        !mux.session_supervisor
            .sessions
            .get(1)
            .expect("session is registered")
            .operator_owned(),
        "release must clear the ownership flag"
    );

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 1,
            text: "go\r".to_owned(),
        },
    );
    assert!(matches!(reply, ServerMsg::SessionSent { session: 1, .. }));
    assert_eq!(
        input_rx.try_recv().expect("payload reached the PTY"),
        b"go\r".to_vec()
    );
}

#[test]
fn release_control_of_an_unknown_session_denies() {
    let mut mux = single_pane_tab_mux();
    let (session, _input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let reply = control_reply_for_request(&mut mux, ClientMsg::ReleaseControl { session: 999 });

    assert!(matches!(
        reply,
        ServerMsg::SessionSendDenied {
            session: 999,
            reason: jackin_protocol::control::SessionSendRejection::UnknownSession,
        }
    ));
}
