// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn fused_relay_allows_apple_supervisor_and_denies_with_distinct_messages() {
    let (authorization, account_a) = single_session_authorization(2_001, 2_001, "account-a");
    let supervisor_pid = jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID;

    // Root peer at the Apple supervisor PID with an in-launch capability is
    // relayed to the host instead of denied.
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (mut host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let proxy_authorization = authorization.clone();
    let proxy = tokio::spawn(async move {
        run_at_with_peer(
            &proxy_socket,
            supervisor(supervisor_pid),
            proxy_authorization,
            Some(PeerIdentity {
                pid: Some(supervisor_pid),
                start_time: Some(SUPERVISOR_START_TIME),
                uid: 0,
                gid: 0,
            }),
            proxy_input,
            proxy_output,
        )
        .await
    });
    wait_for_socket(&socket).await;
    let client = tokio::spawn(send_request(
        socket,
        UsageBrokerOperation::CurrentForCapability {
            capability: account_a.clone(),
        },
    ));
    let mut requests = BufReader::new(host_request_reader);
    let mut line = String::new();
    requests.read_line(&mut line).await.unwrap();
    let frame = serde_json::from_str::<UsageRelayTunnelRequest>(line.trim()).unwrap();
    let response = UsageRelayTunnelResponse {
        request_id: frame.request_id,
        response: UsageBrokerResponse::Error {
            error: UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "relayed".to_owned(),
            },
        },
    };
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    host_response_writer.write_all(&bytes).await.unwrap();
    assert_eq!(error_message(client.await.unwrap()), "relayed");
    proxy.abort();

    // Root peer at the wrong PID fails the supervisor gate.
    let denied = denied_relay_response(
        authorization.clone(),
        supervisor(supervisor_pid),
        PeerIdentity {
            pid: Some(9),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        },
        UsageBrokerOperation::CurrentForCapability {
            capability: account_a.clone(),
        },
    )
    .await;
    assert_eq!(
        error_message(denied),
        "usage relay peer is not the Capsule supervisor"
    );

    // Root peer reusing the supervisor PID with a different start time fails
    // the supervisor gate too.
    let denied = denied_relay_response(
        authorization.clone(),
        supervisor(supervisor_pid),
        root_peer(supervisor_pid, Some(SUPERVISOR_START_TIME + 1)),
        UsageBrokerOperation::CurrentForCapability {
            capability: account_a.clone(),
        },
    )
    .await;
    assert_eq!(
        error_message(denied),
        "usage relay peer is not the Capsule supervisor"
    );

    // Session peer presenting a foreign capability fails the capability gate.
    let denied = denied_relay_response(
        authorization,
        supervisor(supervisor_pid),
        PeerIdentity {
            pid: None,
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 2_001,
            gid: 2_001,
        },
        UsageBrokerOperation::CurrentForCapability {
            capability: capability("account-b"),
        },
    )
    .await;
    assert_eq!(
        error_message(denied),
        "usage account capability is not authorized"
    );
}

#[test]
fn supervisor_binding_rejects_pid_reuse_with_different_start_time() {
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let operation = UsageBrokerOperation::CurrentForCapability {
        capability: account,
    };
    let binding = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);

    // Same PID number, recycled by a later root process: rejected.
    let impostor = root_peer(
        DEFAULT_CAPSULE_SUPERVISOR_PID,
        Some(SUPERVISOR_START_TIME + 1),
    );
    assert!(!binding.matches(impostor));
    assert!(!supervisor_peer_allows(binding, Some(impostor)));
    assert!(!authorization.authorizes(binding, Some(impostor), &operation));

    // Pinned (pid, start_time) of the live supervisor: accepted.
    let legitimate = root_peer(DEFAULT_CAPSULE_SUPERVISOR_PID, Some(SUPERVISOR_START_TIME));
    assert!(binding.matches(legitimate));
    assert!(supervisor_peer_allows(binding, Some(legitimate)));
    assert!(authorization.authorizes(binding, Some(legitimate), &operation));
}

#[test]
fn supervisor_binding_fails_closed_when_start_time_unknown() {
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let operation = UsageBrokerOperation::CurrentForCapability {
        capability: account,
    };

    // Binding unverifiable at startup (`/proc` unreadable): even the exact
    // PID is denied the supervisor path.
    let unverified = SupervisorIdentity {
        pid: DEFAULT_CAPSULE_SUPERVISOR_PID,
        start_time: None,
    };
    let peer = root_peer(DEFAULT_CAPSULE_SUPERVISOR_PID, Some(SUPERVISOR_START_TIME));
    assert!(!unverified.matches(peer));
    assert!(!supervisor_peer_allows(unverified, Some(peer)));
    assert!(!authorization.authorizes(unverified, Some(peer), &operation));

    // Peer start time unreadable at accept time: denied against a verified
    // binding.
    let binding = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    let unknown_peer = root_peer(DEFAULT_CAPSULE_SUPERVISOR_PID, None);
    assert!(!binding.matches(unknown_peer));
    assert!(!supervisor_peer_allows(binding, Some(unknown_peer)));
    assert!(!authorization.authorizes(binding, Some(unknown_peer), &operation));
}

#[test]
fn supervisor_binding_still_requires_root_and_exact_pid() {
    let binding = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    assert!(!binding.matches(PeerIdentity {
        pid: Some(DEFAULT_CAPSULE_SUPERVISOR_PID),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    }));
    assert!(!binding.matches(PeerIdentity {
        pid: Some(DEFAULT_CAPSULE_SUPERVISOR_PID + 1),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 0,
        gid: 0,
    }));
    assert!(!binding.matches(PeerIdentity {
        pid: None,
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 0,
        gid: 0,
    }));
}

#[cfg(target_os = "linux")]
#[test]
fn process_start_time_is_stable_for_self_and_absent_for_missing_pid() {
    let pid = std::process::id();
    let first = process_start_time(pid);
    assert!(first.is_some_and(|start| start > 0));
    assert_eq!(first, process_start_time(pid));
    assert_eq!(process_start_time(u32::MAX), None);
}

#[cfg(target_os = "linux")]
#[test]
fn parse_proc_stat_start_time_skips_comm_with_parens_and_spaces() {
    // pid (comm with ) paren) state ppid ... starttime(v22) ...
    let mut fields = vec!["R".to_owned()];
    fields.extend((4..=21).map(|field| field.to_string()));
    fields.push("987654".to_owned());
    fields.extend(["0".to_owned(), "0".to_owned()]);
    let stat = format!("42 (my ) proc) {}", fields.join(" "));
    assert_eq!(parse_proc_stat_start_time(&stat), Some(987_654));
    assert_eq!(parse_proc_stat_start_time("bogus"), None);
}
