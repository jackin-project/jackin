// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn ownership_exec_script_names_the_action_and_the_session() {
    assert_eq!(
        ownership_exec_script(7, "take-control"),
        "exec /jackin/runtime/jackin-capsule take-control 7"
    );
    assert_eq!(
        ownership_exec_script(7, "release-control"),
        "exec /jackin/runtime/jackin-capsule release-control 7"
    );
}

#[test]
fn ownership_confirmed_accepts_only_the_matching_ack() {
    assert!(ownership_confirmed(ServerMsg::Ack, 4).expect("an Ack confirms ownership"));
    let owned = ownership_confirmed(
        ServerMsg::SessionSendDenied {
            session: 4,
            reason: jackin_protocol::control::SessionSendRejection::OperatorOwned,
        },
        4,
    )
    .expect_err("an owned session must surface as an error");
    assert!(
        format!("{owned:#}").contains("owned by an operator"),
        "unexpected denial text: {owned:#}"
    );
    let missing = ownership_confirmed(
        ServerMsg::SessionSendDenied {
            session: 4,
            reason: jackin_protocol::control::SessionSendRejection::UnknownSession,
        },
        4,
    )
    .expect_err("an unknown session must surface as an error");
    assert!(
        format!("{missing:#}").contains("no such session"),
        "unexpected denial text: {missing:#}"
    );
    let wrong_kind = ownership_confirmed(
        ServerMsg::SessionSent {
            session: 4,
            bytes: 1,
        },
        4,
    )
    .expect_err("a byte receipt is not an ownership confirmation");
    assert!(
        format!("{wrong_kind:#}").contains("expected Ack"),
        "unexpected reply text: {wrong_kind:#}"
    );
}

#[test]
fn bytes_sent_reports_operator_ownership_as_an_error() {
    let refused = bytes_sent(ServerMsg::SessionSendDenied {
        session: 1,
        reason: jackin_protocol::control::SessionSendRejection::OperatorOwned,
    })
    .expect_err("a send blocked by operator ownership must be an error");
    assert!(
        format!("{refused:#}").contains("owned by an operator"),
        "unexpected refusal text: {refused:#}"
    );
}
