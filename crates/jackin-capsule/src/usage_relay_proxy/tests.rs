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
    let mut authorization = UsageRelayAuthorization::for_peer(forced_peer, shared.clone());
    authorization
        .by_instance
        .insert("session-b".to_owned(), shared.clone());
    authorization
        .by_peer
        .insert((2_002, 2_002), ("session-b".to_owned(), shared.clone()));
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
    let mut requests = BufReader::new(host_request_reader);
    let denied = send_request(
        socket.clone(),
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-b".to_owned(),
            capability: shared.clone(),
        },
    )
    .await;
    assert!(matches!(
        denied,
        UsageBrokerResponse::Error { error }
            if error.kind == UsageCoordinationErrorKind::Unauthorized
    ));
    assert!(
        tokio::time::timeout(
            Duration::from_millis(50),
            read_frame::<_, UsageRelayTunnelMessage>(&mut requests),
        )
        .await
        .is_err()
    );

    let first = tokio::spawn(send_request(
        socket.clone(),
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-a".to_owned(),
            capability: shared.clone(),
        },
    ));
    let second = tokio::spawn(send_request(
        socket,
        UsageBrokerOperation::RefreshForCapability {
            instance_id: "session-a".to_owned(),
            capability: shared,
            observed_generation: 0,
            force: true,
        },
    ));
    let mut frames = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        requests.read_line(&mut line).await.unwrap();
        let UsageRelayTunnelMessage::Request { request } =
            serde_json::from_str(line.trim()).unwrap()
        else {
            panic!("unexpected cancellation");
        };
        assert_eq!(request.instance_id.as_deref(), Some("session-a"));
        match &request.request.operation {
            UsageBrokerOperation::CurrentForCapability { instance_id, .. }
            | UsageBrokerOperation::RefreshForCapability { instance_id, .. } => {
                assert_eq!(instance_id, "session-a");
            }
            operation => panic!("unexpected operation: {operation:?}"),
        }
        frames.push(request);
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
            instance_id: "session-a".to_owned(),
            capability: account_a,
        },
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-a".to_owned(),
            capability: account_b.clone(),
        },
    ));
    assert!(authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(peer_b),
        &UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-b".to_owned(),
            capability: account_b,
        },
    ));
    assert!(!authorization.authorizes(
        supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        None,
        &UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-a".to_owned(),
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
        instance_id: "session-a".to_owned(),
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
            canonical_identity: None,
            provenance_count: 0,
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
        instance_id: "session-a".to_owned(),
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
            instance_id: "session-a".to_owned(),
            capability: account_a.clone(),
        },
    ));
    let mut requests = BufReader::new(host_request_reader);
    let mut line = String::new();
    requests.read_line(&mut line).await.unwrap();
    let UsageRelayTunnelMessage::Request { request: frame } =
        serde_json::from_str(line.trim()).unwrap()
    else {
        panic!("unexpected cancellation");
    };
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
            instance_id: "session-a".to_owned(),
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
            instance_id: "session-a".to_owned(),
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
            instance_id: "session-a".to_owned(),
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
        instance_id: "session-a".to_owned(),
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
        instance_id: "session-a".to_owned(),
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

#[test]
fn workspace_inventory_requires_exact_supervisor_even_without_launch_instances() {
    let authorization = UsageRelayAuthorization::from_config(&CapsuleConfig::default()).unwrap();
    let root = PeerIdentity {
        pid: Some(DEFAULT_CAPSULE_SUPERVISOR_PID),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 0,
        gid: 0,
    };
    let operation = UsageBrokerOperation::CurrentProjectionForSurface;
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    assert!(authorization.authorizes(supervisor, Some(root), &operation));
    assert!(!authorization.authorizes(
        supervisor,
        Some(PeerIdentity {
            uid: 2_001,
            gid: 2_001,
            ..root
        }),
        &operation
    ));
    assert!(!authorization.authorizes(
        supervisor,
        Some(PeerIdentity {
            start_time: Some(SUPERVISOR_START_TIME + 1),
            ..root
        }),
        &operation
    ));
    assert!(!authorization.authorizes(supervisor, None, &operation));
    assert!(!authorization.authorizes(
        supervisor,
        Some(root),
        &UsageBrokerOperation::RequestRefreshForSurface {
            force: true,
            observed_projection_id: None
        }
    ));
}

#[tokio::test]
async fn local_deadline_covers_partial_frame_without_tunnel_dispatch() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let (requests, mut request_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    client.write_all(b"{\"operation\":").await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        handle_local(
            server,
            LocalRequestOwner {
                request_id: 1,
                requests,
                pending: Arc::clone(&pending),
                supervisor: supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
                authorization: Arc::new(UsageRelayAuthorization::default()),
                peer: None,
                deadline: Instant::now() + Duration::from_millis(20),
                cancellations: mpsc::channel(TUNNEL_CAPACITY).0,
                failed_cancellation: mpsc::channel(1).0,
            },
        ),
    )
    .await
    .unwrap();
    assert!(pending.lock().await.is_empty());
    assert!(request_rx.recv().await.is_none());
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).await.unwrap(), 0);
}

#[tokio::test]
async fn local_deadline_covers_saturated_queue_and_removes_pending() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let (requests, mut request_rx) = mpsc::channel(1);
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let peer = PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let account = capability("shared");
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        operation: UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-a".to_owned(),
            capability: account.clone(),
        },
        launch_credential_scope: None,
    };
    requests
        .send((
            Instant::now() + RESPONSE_TIMEOUT,
            UsageRelayTunnelRequest {
                instance_id: Some("session-a".to_owned()),
                expires_at_unix_ms: u64::MAX,
                request_id: 0,
                request: request.clone(),
            },
        ))
        .await
        .unwrap();
    write_frame(&mut client, &request).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        handle_local(
            server,
            LocalRequestOwner {
                request_id: 1,
                requests,
                pending: Arc::clone(&pending),
                supervisor: supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
                authorization: Arc::new(UsageRelayAuthorization::for_peer(peer, account)),
                peer: Some(peer),
                deadline: Instant::now() + Duration::from_millis(20),
                cancellations: mpsc::channel(TUNNEL_CAPACITY).0,
                failed_cancellation: mpsc::channel(1).0,
            },
        ),
    )
    .await
    .unwrap();
    assert!(pending.lock().await.is_empty());
    assert_eq!(request_rx.recv().await.unwrap().1.request_id, 0);
    assert!(request_rx.recv().await.is_none());
}

#[tokio::test]
async fn proxy_admission_bounds_partial_clients_and_owner_drop_closes_them() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (_host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, _host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let proxy = tokio::spawn(async move {
        run_at_with_peer(
            &proxy_socket,
            supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
            UsageRelayAuthorization::default(),
            None,
            proxy_input,
            proxy_output,
        )
        .await
    });
    wait_for_socket(&socket).await;
    let mut clients = Vec::new();
    for _ in 0..TUNNEL_CAPACITY {
        clients.push(UnixStream::connect(&socket).await.unwrap());
    }
    let mut overflow = UnixStream::connect(&socket).await.unwrap();
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), overflow.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    proxy.abort();
    drop(proxy.await);
    for mut client in clients {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), client.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}

#[tokio::test(start_paused = true)]
async fn guest_deadline_emits_cancel_for_already_forwarded_request_on_live_tunnel() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (_host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let peer = PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let account = capability("pending");
    let authorization = UsageRelayAuthorization::for_peer(peer, account.clone());
    let proxy = tokio::spawn(async move {
        run_at_with_peer(
            &proxy_socket,
            supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
            authorization,
            Some(peer),
            proxy_input,
            proxy_output,
        )
        .await
    });
    wait_for_socket(&socket).await;
    let mut client = UnixStream::connect(&socket).await.unwrap();
    write_frame(
        &mut client,
        &UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            operation: UsageBrokerOperation::RefreshForCapability {
                instance_id: "session-a".to_owned(),
                capability: account,
                observed_generation: 0,
                force: true,
            },
            launch_credential_scope: None,
        },
    )
    .await
    .unwrap();
    let mut tunnel = BufReader::new(host_request_reader);
    let UsageRelayTunnelMessage::Request { request } = read_frame(&mut tunnel).await.unwrap()
    else {
        panic!("expected forwarded request");
    };
    tokio::time::advance(RESPONSE_TIMEOUT).await;
    let cancellation: UsageRelayTunnelMessage = read_frame(&mut tunnel).await.unwrap();
    assert_eq!(
        cancellation,
        UsageRelayTunnelMessage::Cancel {
            request_id: request.request_id
        }
    );
    assert!(
        !proxy.is_finished(),
        "request expiry must preserve a writable tunnel"
    );
    proxy.abort();
    drop(proxy.await);
}

#[tokio::test]
async fn forwarded_local_full_close_cancels_without_waiting_for_deadline() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let (requests, mut request_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let (cancellations, mut cancellation_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let peer = PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let account = capability("pending");
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let handler = tokio::spawn(handle_local(
        server,
        LocalRequestOwner {
            request_id: 77,
            requests,
            pending: Arc::clone(&pending),
            supervisor: supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
            authorization: Arc::new(UsageRelayAuthorization::for_peer(peer, account.clone())),
            peer: Some(peer),
            deadline: Instant::now() + RESPONSE_TIMEOUT,
            cancellations,
            failed_cancellation: mpsc::channel(1).0,
        },
    ));
    write_frame(
        &mut client,
        &UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            operation: UsageBrokerOperation::CurrentForCapability {
                instance_id: "session-a".to_owned(),
                capability: account,
            },
            launch_credential_scope: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(request_rx.recv().await.unwrap().1.request_id, 77);
    drop(client);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), cancellation_rx.recv())
            .await
            .unwrap(),
        Some(77)
    );
    handler.await.unwrap();
    assert!(pending.lock().await.is_empty());
}

#[tokio::test]
async fn protocol_write_half_close_keeps_request_and_response_writer_live() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let (requests, mut request_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let (cancellations, mut cancellation_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let peer = PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let account = capability("pending");
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let handler = tokio::spawn(handle_local(
        server,
        LocalRequestOwner {
            request_id: 78,
            requests,
            pending: Arc::clone(&pending),
            supervisor: supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
            authorization: Arc::new(UsageRelayAuthorization::for_peer(peer, account.clone())),
            peer: Some(peer),
            deadline: Instant::now() + RESPONSE_TIMEOUT,
            cancellations,
            failed_cancellation: mpsc::channel(1).0,
        },
    ));
    write_frame(
        &mut client,
        &UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            operation: UsageBrokerOperation::CurrentForCapability {
                instance_id: "session-a".to_owned(),
                capability: account,
            },
            launch_credential_scope: None,
        },
    )
    .await
    .unwrap();
    client.shutdown().await.unwrap();
    request_rx.recv().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), cancellation_rx.recv())
            .await
            .is_err()
    );
    assert!(!handler.is_finished());
    let expected = unavailable_response();
    pending
        .lock()
        .await
        .remove(&78)
        .unwrap()
        .send(expected.clone())
        .unwrap();
    let response: UsageBrokerResponse = tokio::time::timeout(
        Duration::from_secs(1),
        read_frame(&mut BufReader::new(client)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response, expected);
    handler.await.unwrap();
    assert!(cancellation_rx.recv().await.is_none());
}

#[test]
fn usage_relay_checks_peer_instance_and_supervisor_selector() {
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let session_peer = Some(PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    });
    let supervisor_peer = Some(PeerIdentity {
        pid: Some(1),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 0,
        gid: 0,
    });
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    for mut operation in [
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-b".to_owned(),
            capability: account.clone(),
        },
        UsageBrokerOperation::RefreshForCapability {
            instance_id: "session-b".to_owned(),
            capability: account.clone(),
            observed_generation: 0,
            force: true,
        },
        UsageBrokerOperation::JoinForCapability {
            instance_id: "session-b".to_owned(),
            capability: account.clone(),
            generation: 1,
            timeout_ms: 100,
        },
    ] {
        assert!(!authorization.authorizes(supervisor, supervisor_peer, &operation));
        assert_eq!(
            authorization.stamp_operation(supervisor, session_peer, &mut operation),
            None,
        );
        match &mut operation {
            UsageBrokerOperation::CurrentForCapability { instance_id, .. }
            | UsageBrokerOperation::RefreshForCapability { instance_id, .. }
            | UsageBrokerOperation::JoinForCapability { instance_id, .. } => {
                assert_eq!(instance_id, "session-b");
                instance_id.clear();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            authorization.stamp_operation(supervisor, session_peer, &mut operation),
            None,
        );
        match &mut operation {
            UsageBrokerOperation::CurrentForCapability { instance_id, .. }
            | UsageBrokerOperation::RefreshForCapability { instance_id, .. }
            | UsageBrokerOperation::JoinForCapability { instance_id, .. } => {
                *instance_id = "session-a".to_owned();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            authorization.stamp_operation(supervisor, session_peer, &mut operation),
            Some(Some("session-a".to_owned())),
        );
        match &operation {
            UsageBrokerOperation::CurrentForCapability { instance_id, .. }
            | UsageBrokerOperation::RefreshForCapability { instance_id, .. }
            | UsageBrokerOperation::JoinForCapability { instance_id, .. } => {
                assert_eq!(instance_id, "session-a");
            }
            _ => unreachable!(),
        }
        assert!(authorization.authorizes(supervisor, supervisor_peer, &operation));
    }
    for operation in [
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-b".to_owned(),
            capability: account.clone(),
        },
        UsageBrokerOperation::CurrentForCapability {
            instance_id: "session-a".to_owned(),
            capability: capability("wrong-account"),
        },
        UsageBrokerOperation::Current {
            capability: account.clone(),
        },
        UsageBrokerOperation::Refresh {
            capability: account.clone(),
            observed_generation: 0,
            force: true,
        },
        UsageBrokerOperation::Join {
            capability: account,
            generation: 1,
            timeout_ms: 100,
        },
    ] {
        assert!(!authorization.authorizes(supervisor, supervisor_peer, &operation));
    }
    assert_eq!(
        authorization.stamp_operation(
            supervisor,
            session_peer,
            &mut UsageBrokerOperation::Current {
                capability: capability("account-a"),
            },
        ),
        Some(Some("session-a".to_owned())),
    );
    assert_eq!(
        authorization.stamp_operation(
            supervisor,
            supervisor_peer,
            &mut UsageBrokerOperation::CurrentProjectionForSurface,
        ),
        Some(None),
    );
    assert!(!authorization.authorizes(
        supervisor,
        session_peer,
        &UsageBrokerOperation::CurrentProjectionForSurface,
    ));
}

#[tokio::test(start_paused = true)]
async fn admitted_guest_request_first_polled_after_expiry_never_queues() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let peer = PeerIdentity {
        pid: Some(9),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let account = capability("expired-admitted");
    write_frame(
        &mut client,
        &UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            operation: UsageBrokerOperation::RefreshForCapability {
                capability: account.clone(),
                instance_id: "session-a".to_owned(),
                observed_generation: 0,
                force: true,
            },
            launch_credential_scope: None,
        },
    )
    .await
    .unwrap();
    server.readable().await.unwrap();
    let (requests, mut request_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let (cancellations, mut cancellation_rx) = mpsc::channel(TUNNEL_CAPACITY);
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let deadline = Instant::now() + Duration::from_secs(1);
    let call = handle_local(
        server,
        LocalRequestOwner {
            request_id: 90,
            requests,
            pending: Arc::clone(&pending),
            supervisor: supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
            authorization: Arc::new(UsageRelayAuthorization::for_peer(peer, account)),
            peer: Some(peer),
            deadline,
            cancellations,
            failed_cancellation: mpsc::channel(1).0,
        },
    );
    tokio::time::advance(Duration::from_secs(2)).await;
    call.await;
    assert!(
        request_rx.recv().await.is_none(),
        "expired guest work must never reach tunnel queue"
    );
    assert!(pending.lock().await.is_empty());
    assert_eq!(cancellation_rx.recv().await, Some(90));
}
