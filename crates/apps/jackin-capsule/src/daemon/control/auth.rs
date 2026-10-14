// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{ClientMsg, Multiplexer};

pub(crate) fn attach_peer_is_authorized(mux: &Multiplexer, peer_uid: Option<u32>) -> bool {
    let Some(peer_uid) = peer_uid else {
        return false;
    };
    !configured_session_uid(mux, peer_uid)
}

pub(crate) fn control_request_allowed(
    mux: &Multiplexer,
    peer_uid: Option<u32>,
    session_capability: Option<&str>,
    message: &ClientMsg,
) -> bool {
    let Some(peer_uid) = peer_uid else {
        return false;
    };

    if matches!(message, ClientMsg::ExecCommand { .. }) {
        if peer_uid == 0 {
            return true;
        }
        return mux.session_supervisor.sessions.values().any(|session| {
            session.identity.uid == peer_uid
                && capability_matches(&session.control_capability, session_capability)
        });
    }

    if !configured_session_uid(mux, peer_uid) {
        return true;
    }

    let target = match message {
        ClientMsg::ReportRuntimeEvent { session_id, .. }
        | ClientMsg::StatusCapture { session_id }
        | ClientMsg::TokenUsage { session_id }
        | ClientMsg::SessionSend {
            session: session_id,
            ..
        } => Some(*session_id),
        ClientMsg::Events {
            session: Some(session_id),
        } => Some(*session_id),

        ClientMsg::Events { session: None }
        | ClientMsg::TelemetryHealth
        | ClientMsg::Status
        | ClientMsg::Snapshot
        | ClientMsg::Agents
        | ClientMsg::UsageFocused
        | ClientMsg::UsageRefreshFocused
        | ClientMsg::UsageAccountList
        | ClientMsg::ExecCommand { .. }
        | ClientMsg::Unknown => None,
    };

    target.is_some_and(|session_id| {
        mux.session_supervisor
            .sessions
            .get(session_id)
            .is_some_and(|session| {
                session.identity.uid == peer_uid
                    && capability_matches(&session.control_capability, session_capability)
            })
    })
}

fn capability_matches(expected: &str, presented: Option<&str>) -> bool {
    let Some(presented) = presented else {
        return false;
    };
    let expected = expected.as_bytes();
    let presented = presented.as_bytes();
    let max = expected.len().max(presented.len());
    let mut difference = expected.len() ^ presented.len();
    for index in 0..max {
        let left = expected.get(index).copied().unwrap_or(0);
        let right = presented.get(index).copied().unwrap_or(0);
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

fn configured_session_uid(mux: &Multiplexer, peer_uid: u32) -> bool {
    mux.launch_env
        .launch_config
        .instance_identities
        .values()
        .any(|identity| identity.uid == peer_uid)
        || mux
            .launch_env
            .launch_config
            .shell_identity
            .is_some_and(|identity| identity.uid == peer_uid)
}
