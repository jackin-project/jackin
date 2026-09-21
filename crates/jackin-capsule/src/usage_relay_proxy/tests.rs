// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability, UsageBrokerOperation, UsageCatalogEntry,
    UsageCoordinationError,
};
use jackin_protocol::{CapsuleConfig, SessionIdentity};
use std::collections::BTreeMap;
use tokio::io::BufReader;

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
fn usage_relay_rejects_host_only_catalog_reconciliation() {
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
    let operation = UsageBrokerOperation::ReconcileCatalog {
        expected_projection_id: None,
        catalog_revision: "catalog-2".to_owned(),
        entries: vec![UsageCatalogEntry {
            capability: account,
            revision: "credential-2".to_owned(),
        }],
    };

    assert!(!authorization.authorizes(
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

fn capability(account_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: "claude".to_owned(),
    }
}

async fn send_request(
    socket: std::path::PathBuf,
    operation: UsageBrokerOperation,
) -> UsageBrokerResponse {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        operation,
        launch_credential_scope: None,
    };
    let mut bytes = serde_json::to_vec(&request).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).await.unwrap();
    stream.shutdown().await.unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await.unwrap();
    serde_json::from_str(line.trim()).unwrap()
}

fn error_message(response: UsageBrokerResponse) -> String {
    let UsageBrokerResponse::Error { error } = response else {
        panic!("expected error response");
    };
    error.message
}

async fn wait_for_socket(socket: &Path) {
    for _ in 0..100 {
        if socket.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("usage proxy socket was not created");
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

fn single_session_authorization(
    uid: u32,
    gid: u32,
    account_id: &str,
) -> (UsageRelayAuthorization, UsageAccountCapability) {
    let account = capability(account_id);
    let config = CapsuleConfig {
        instances: vec!["session-a".to_owned()],
        usage_capabilities: BTreeMap::from([("session-a".to_owned(), account.clone())]),
        instance_identities: BTreeMap::from([(
            "session-a".to_owned(),
            SessionIdentity { uid, gid },
        )]),
        ..CapsuleConfig::default()
    };
    (
        UsageRelayAuthorization::from_config(&config).unwrap(),
        account,
    )
}

async fn denied_relay_response(
    authorization: UsageRelayAuthorization,
    supervisor: SupervisorIdentity,
    peer: PeerIdentity,
    operation: UsageBrokerOperation,
) -> UsageBrokerResponse {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (_host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, _host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let proxy = tokio::spawn(async move {
        run_at_with_peer(
            &proxy_socket,
            supervisor,
            authorization,
            Some(peer),
            proxy_input,
            proxy_output,
        )
        .await
    });
    wait_for_socket(&socket).await;
    let response = send_request(socket, operation).await;
    proxy.abort();
    response
}

const SUPERVISOR_START_TIME: u64 = 123_456;

fn supervisor(pid: u32) -> SupervisorIdentity {
    SupervisorIdentity {
        pid,
        start_time: Some(SUPERVISOR_START_TIME),
    }
}

fn root_peer(pid: u32, start_time: Option<u64>) -> PeerIdentity {
    PeerIdentity {
        pid: Some(pid),
        start_time,
        uid: 0,
        gid: 0,
    }
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
