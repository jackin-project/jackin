// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Attach-peer authorization and control-request admission.

use super::super::{ClientMsg, Multiplexer};

/// Whether a kernel-authenticated peer may open the interactive attach path.
///
/// The socket is intentionally still owner-only, but `DAC_OVERRIDE` makes file
/// modes insufficient. Every admitted session identity is therefore denied at
/// the protocol boundary. A non-admitted UID is an operator/host peer and
/// retains the existing attach flow.
pub(crate) fn attach_peer_is_authorized(mux: &Multiplexer, peer_uid: Option<u32>) -> bool {
    let Some(peer_uid) = peer_uid else {
        return false;
    };
    !configured_session_uid(mux, peer_uid)
}

/// Authorize control requests using the peer credential supplied by the
/// kernel, never a caller-controlled session id. Operator peers retain the
/// existing control surface. A session peer may address only its own live
/// session through target-scoped, non-administrative requests.
pub(crate) fn control_request_allowed(
    mux: &Multiplexer,
    peer_uid: Option<u32>,
    session_capability: Option<&str>,
    message: &ClientMsg,
) -> bool {
    let Some(peer_uid) = peer_uid else {
        return false;
    };

    // The in-container MCP/`jackin-exec` path has no target session field in
    // its wire shape. Infer exactly one authorized session from the kernel
    // peer UID plus its daemon-issued capability; never let a session peer
    // enter the operator/global credential picker without both.
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
        // Global inventory, usage, telemetry, execution, and unfiltered event
        // streams are operator/admin surfaces. Session peers never receive
        // them, even when the message has no obvious secret fields.
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

/// Compare the fixed-format session bearer without an early exit on the
/// secret bytes. The socket is local, but keeping the comparison uniform costs
/// nothing and avoids turning the authorization branch into a token oracle.
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
