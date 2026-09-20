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
    let peer = PeerIdentity {
        pid: Some(9),
        uid: 2_001,
        gid: 2_001,
    };
    let authorization = UsageRelayAuthorization::for_peer(peer, shared.clone());
    let proxy = tokio::spawn(async move {
        run_at_with_peer(
            &proxy_socket,
            authorization,
            DEFAULT_CAPSULE_SUPERVISOR_PID,
            Some(peer),
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
        uid: 2_001,
        gid: 2_001,
    };
    let peer_b = PeerIdentity {
        pid: None,
        uid: 2_002,
        gid: 2_002,
    };

    assert!(authorization.authorizes(
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_a,
        },
        DEFAULT_CAPSULE_SUPERVISOR_PID,
    ));
    assert!(!authorization.authorizes(
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_b.clone(),
        },
        DEFAULT_CAPSULE_SUPERVISOR_PID,
    ));
    assert!(authorization.authorizes(
        Some(peer_b),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_b,
        },
        DEFAULT_CAPSULE_SUPERVISOR_PID,
    ));
    assert!(!authorization.authorizes(
        None,
        &UsageBrokerOperation::CurrentForCapability {
            capability: capability("account-a"),
        },
        DEFAULT_CAPSULE_SUPERVISOR_PID,
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

    let apple_supervisor = Some(PeerIdentity {
        pid: Some(2),
        uid: 0,
        gid: 0,
    });
    assert!(!authorization.authorizes(
        apple_supervisor,
        &operation,
        DEFAULT_CAPSULE_SUPERVISOR_PID,
    ));
    assert!(authorization.authorizes(apple_supervisor, &operation, 2));
    assert!(!authorization.authorizes(
        Some(PeerIdentity {
            pid: None,
            uid: 0,
            gid: 0,
        }),
        &operation,
        2,
    ));
    assert!(!authorization.authorizes(
        Some(PeerIdentity {
            pid: Some(1),
            uid: 0,
            gid: 1,
        }),
        &operation,
        1,
    ));
    assert!(authorization.authorizes(
        Some(PeerIdentity {
            pid: Some(1),
            uid: 0,
            gid: 0,
        }),
        &operation,
        DEFAULT_CAPSULE_SUPERVISOR_PID,
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
        Some(PeerIdentity {
            pid: Some(1),
            uid: 0,
            gid: 0,
        }),
        &operation,
        DEFAULT_CAPSULE_SUPERVISOR_PID,
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
    assert!(!supervisor_peer_allows(2, None));
    assert!(!supervisor_peer_allows(
        2,
        Some(PeerIdentity {
            pid: Some(1),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(!supervisor_peer_allows(
        2,
        Some(PeerIdentity {
            pid: Some(3),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(!supervisor_peer_allows(
        2,
        Some(PeerIdentity {
            pid: Some(2),
            uid: 2_001,
            gid: 0,
        }),
    ));
    assert!(!supervisor_peer_allows(
        2,
        Some(PeerIdentity {
            pid: Some(2),
            uid: 0,
            gid: 1,
        }),
    ));
    assert!(supervisor_peer_allows(
        2,
        Some(PeerIdentity {
            pid: Some(2),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(supervisor_peer_allows(
        1,
        Some(PeerIdentity {
            pid: Some(1),
            uid: 0,
            gid: 0,
        }),
    ));
    assert!(supervisor_peer_allows(
        2,
        Some(PeerIdentity {
            pid: Some(9),
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
