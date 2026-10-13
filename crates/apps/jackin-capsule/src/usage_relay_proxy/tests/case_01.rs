// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn broker_client_stdio_proxy_multiplexes_out_of_order_responses() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (mut host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let shared = capability("shared");
    let forced_peer = PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let authorization = UsageRelayAuthorization::for_peer(forced_peer, shared.clone());
    let proxy = tokio::spawn(async move {
        run_at_with_peer(
            &proxy_socket,
            supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
            authorization,
            Some(forced_peer),
            proxy_input,
            proxy_output,
        )
        .await
    });
    wait_for_socket(&socket).await;

    let first = tokio::spawn(send_request(
        socket.clone(),
        UsageBrokerOperation::CurrentForCapability {
            capability: shared.clone(),
        },
    ));
    let second = tokio::spawn(send_request(
        socket,
        UsageBrokerOperation::RefreshForCapability {
            capability: shared,
            observed_generation: 0,
            force: true,
        },
    ));
    let mut requests = BufReader::new(host_request_reader);
    let mut frames = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        requests.read_line(&mut line).await.unwrap();
        frames.push(serde_json::from_str::<UsageRelayTunnelRequest>(line.trim()).unwrap());
    }
    frames.reverse();
    for frame in frames {
        let message = match frame.request.operation {
            UsageBrokerOperation::CurrentForCapability { .. } => "current",
            UsageBrokerOperation::RefreshForCapability { .. } => "refresh",
            operation => panic!("unexpected operation: {operation:?}"),
        };
        let response = UsageRelayTunnelResponse {
            request_id: frame.request_id,
            response: UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::Unauthorized,
                    message: message.to_owned(),
                },
            },
        };
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        host_response_writer.write_all(&bytes).await.unwrap();
    }

    assert_eq!(error_message(first.await.unwrap()), "current");
    assert_eq!(error_message(second.await.unwrap()), "refresh");
    proxy.abort();
}

#[test]
fn usage_relay_binds_session_peer_to_its_capability() {
    let account_a = capability("account-a");
    let account_b = capability("account-b");
    let config = CapsuleConfig {
        instances: vec!["session-a".to_owned(), "session-b".to_owned()],
        usage_capabilities: BTreeMap::from([
            ("session-a".to_owned(), account_a.clone()),
            ("session-b".to_owned(), account_b.clone()),
        ]),
        instance_identities: BTreeMap::from([
            (
                "session-a".to_owned(),
                SessionIdentity {
                    uid: 2_001,
                    gid: 2_001,
                },
            ),
            (
                "session-b".to_owned(),
                SessionIdentity {
                    uid: 2_002,
                    gid: 2_002,
                },
            ),
        ]),
        ..CapsuleConfig::default()
    };
    let authorization = UsageRelayAuthorization::from_config(&config).unwrap();
    let peer_a = PeerIdentity {
        pid: None,
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let peer_b = PeerIdentity {
        pid: None,
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_002,
        gid: 2_002,
    };

    assert!(authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_a,
        },
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_b.clone(),
        },
    ));
    assert!(authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(peer_b),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_b,
        },
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        None,
        &UsageBrokerOperation::CurrentForCapability {
            capability: capability("account-a"),
        },
    ));
}

#[test]
fn usage_relay_rejects_agent_root_but_accepts_capsule_supervisor() {
    let account = capability("account-a");
    let config = CapsuleConfig {
        instances: vec!["session-a".to_owned()],
        usage_capabilities: BTreeMap::from([("session-a".to_owned(), account.clone())]),
        instance_identities: BTreeMap::from([(
            "session-a".to_owned(),
            SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        )]),
        ..CapsuleConfig::default()
    };
    let authorization = UsageRelayAuthorization::from_config(&config).unwrap();
    let operation = UsageBrokerOperation::CurrentForCapability {
        capability: account,
    };

    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(PeerIdentity {
            pid: Some(2),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
        &operation,
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(PeerIdentity {
            pid: None,
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
        &operation,
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(PeerIdentity {
            pid: Some(1),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 1,
        }),
        &operation,
    ));
    assert!(authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(PeerIdentity {
            pid: Some(1),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
        &operation,
    ));
}

#[test]
fn supervisor_peer_allows_only_exact_root_supervisor() {
    assert!(!supervisor_peer_allows(supervisor(2), None));
    assert!(!supervisor_peer_allows(
        supervisor(2),
        Some(PeerIdentity {
            pid: Some(1),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(!supervisor_peer_allows(
        supervisor(2),
        Some(PeerIdentity {
            pid: Some(3),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(!supervisor_peer_allows(
        supervisor(2),
        Some(PeerIdentity {
            pid: Some(2),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 2_001,
            gid: 0,
        }),
    ));
    assert!(!supervisor_peer_allows(
        supervisor(2),
        Some(PeerIdentity {
            pid: Some(2),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 1,
        }),
    ));
    assert!(supervisor_peer_allows(
        supervisor(2),
        Some(PeerIdentity {
            pid: Some(2),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(supervisor_peer_allows(
        supervisor(1),
        Some(PeerIdentity {
            pid: Some(1),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(supervisor_peer_allows(
        supervisor(2),
        Some(PeerIdentity {
            pid: Some(9),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 2_001,
            gid: 2_001,
        }),
    ));
}

#[test]
fn parse_supervisor_pid_defaults_and_rejects_invalid_values() {
    assert_eq!(
        parse_supervisor_pid(Err(std::env::VarError::NotPresent)).unwrap(),
        DEFAULT_CAPSULE_SUPERVISOR_PID
    );
    assert_eq!(parse_supervisor_pid(Ok("2".to_owned())).unwrap(), 2);
    let _zero = parse_supervisor_pid(Ok("0".to_owned())).unwrap_err();
    let _garbage = parse_supervisor_pid(Ok("nope".to_owned())).unwrap_err();
}

#[test]
fn usage_relay_supervisor_pid_parameter_selects_the_root_bypass() {
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let operation = UsageBrokerOperation::CurrentForCapability {
        capability: account,
    };
    let apple_supervisor = Some(PeerIdentity {
        pid: Some(jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 0,
        gid: 0,
    });
    assert!(authorization.authorizes(
        supervisor(jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID),
        apple_supervisor,
        &operation,
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        apple_supervisor,
        &operation,
    ));
}
