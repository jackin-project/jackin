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

const SUPERVISOR_STARTTIME: u64 = 12_345;
const IMPOSTOR_STARTTIME: u64 = 67_890;

fn supervisor(pid: u32) -> SupervisorIdentity {
    SupervisorIdentity {
        pid,
        starttime: SUPERVISOR_STARTTIME,
    }
}

fn session_peer(uid: u32, gid: u32) -> PeerIdentity {
    PeerIdentity {
        pid: Some(9),
        uid,
        gid,
        starttime: None,
    }
}

fn supervisor_peer(pid: u32) -> PeerIdentity {
    PeerIdentity {
        pid: Some(pid),
        uid: 0,
        gid: 0,
        starttime: Some(SUPERVISOR_STARTTIME),
    }
}

fn impostor_peer(pid: u32) -> PeerIdentity {
    PeerIdentity {
        pid: Some(pid),
        uid: 0,
        gid: 0,
        starttime: Some(IMPOSTOR_STARTTIME),
    }
}

#[tokio::test]
async fn broker_client_stdio_proxy_multiplexes_out_of_order_responses() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (mut host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let shared = capability("shared");
    let forced_peer = session_peer(2_001, 2_001);
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
        uid: 2_001,
        gid: 2_001,
        starttime: None,
    };
    let peer_b = PeerIdentity {
        pid: None,
        uid: 2_002,
        gid: 2_002,
        starttime: None,
    };
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);

    assert!(authorization.authorizes(
        &supervisor,
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_a,
        },
    ));
    assert!(!authorization.authorizes(
        &supervisor,
        Some(peer_a),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_b.clone(),
        },
    ));
    assert!(authorization.authorizes(
        &supervisor,
        Some(peer_b),
        &UsageBrokerOperation::CurrentForCapability {
            capability: account_b,
        },
    ));
    assert!(!authorization.authorizes(
        &supervisor,
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
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);

    assert!(!authorization.authorizes(
        &supervisor,
        Some(PeerIdentity {
            pid: Some(2),
            uid: 0,
            gid: 0,
            starttime: Some(SUPERVISOR_STARTTIME),
        }),
        &operation,
    ));
    assert!(!authorization.authorizes(
        &supervisor,
        Some(PeerIdentity {
            pid: None,
            uid: 0,
            gid: 0,
            starttime: Some(SUPERVISOR_STARTTIME),
        }),
        &operation,
    ));
    assert!(!authorization.authorizes(
        &supervisor,
        Some(PeerIdentity {
            pid: Some(1),
            uid: 0,
            gid: 1,
            starttime: Some(SUPERVISOR_STARTTIME),
        }),
        &operation,
    ));
    assert!(authorization.authorizes(
        &supervisor,
        Some(supervisor_peer(DEFAULT_CAPSULE_SUPERVISOR_PID)),
        &operation,
    ));
}

#[test]
fn usage_relay_rejects_pid_reuse_impostor_with_supervisor_pid() {
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
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);

    // Another root process reusing the supervisor PID after a restart has a
    // different starttime: both gates must deny it launch-wide capabilities.
    let impostor = impostor_peer(DEFAULT_CAPSULE_SUPERVISOR_PID);
    assert!(!authorization.authorizes(&supervisor, Some(impostor), &operation));
    assert!(!supervisor_peer_allows(&supervisor, Some(impostor)));

    // A root peer whose starttime could not be read is denied as well.
    let unreadable = PeerIdentity {
        pid: Some(DEFAULT_CAPSULE_SUPERVISOR_PID),
        uid: 0,
        gid: 0,
        starttime: None,
    };
    assert!(!authorization.authorizes(&supervisor, Some(unreadable), &operation));
    assert!(!supervisor_peer_allows(&supervisor, Some(unreadable)));

    // The bound supervisor itself is still admitted by both gates.
    let genuine = supervisor_peer(DEFAULT_CAPSULE_SUPERVISOR_PID);
    assert!(authorization.authorizes(&supervisor, Some(genuine), &operation));
    assert!(supervisor_peer_allows(&supervisor, Some(genuine)));
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
        &supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        Some(supervisor_peer(DEFAULT_CAPSULE_SUPERVISOR_PID)),
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
    let bound = supervisor(2);
    assert!(!supervisor_peer_allows(&bound, None));
    assert!(!supervisor_peer_allows(&bound, Some(supervisor_peer(1)),));
    assert!(!supervisor_peer_allows(&bound, Some(supervisor_peer(3)),));
    assert!(!supervisor_peer_allows(
        &bound,
        Some(PeerIdentity {
            pid: Some(2),
            uid: 2_001,
            gid: 0,
            starttime: Some(SUPERVISOR_STARTTIME),
        }),
    ));
    assert!(!supervisor_peer_allows(
        &bound,
        Some(PeerIdentity {
            pid: Some(2),
            uid: 0,
            gid: 1,
            starttime: Some(SUPERVISOR_STARTTIME),
        }),
    ));
    assert!(!supervisor_peer_allows(&bound, Some(impostor_peer(2)),));
    assert!(supervisor_peer_allows(&bound, Some(supervisor_peer(2)),));
    let bound_one = supervisor(1);
    assert!(supervisor_peer_allows(&bound_one, Some(supervisor_peer(1)),));
    assert!(supervisor_peer_allows(
        &bound,
        Some(session_peer(2_001, 2_001)),
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
fn parse_linux_stat_starttime_reads_field_22() {
    // Fields 3..22: state ppid pgrp session tty_nr tpgid flags minflt cminflt
    // majflt cmajflt utime stime cutime cstime priority nice num_threads
    // itrealvalue starttime.
    let stat = "1 (supervisor) S 0 1 1 0 -1 4194304 100 0 0 0 10 5 0 0 20 0 1 0 4242 1000000 0";
    assert_eq!(parse_linux_stat_starttime(stat), Some(4_242));
    // `comm` may contain spaces and parentheses; the split must use the last `)`.
    let tricky = "99 (my (tricky) sup) R 1 99 99 0 -1 4194304 0 0 0 0 0 0 0 0 20 0 2 0 777 500 0";
    assert_eq!(parse_linux_stat_starttime(tricky), Some(777));
    assert_eq!(parse_linux_stat_starttime("garbage"), None);
    assert_eq!(parse_linux_stat_starttime("1 (x) S"), None);
}

#[test]
fn supervisor_bind_fails_closed_for_missing_pid() {
    let _missing = SupervisorIdentity::bind(u32::MAX).unwrap_err();
}

#[cfg(target_os = "linux")]
#[test]
fn supervisor_bind_reads_live_starttime_on_linux() {
    let identity = SupervisorIdentity::bind(std::process::id()).unwrap();
    assert_eq!(identity.pid, std::process::id());
    assert_eq!(
        Some(identity.starttime),
        process_starttime(std::process::id())
    );
}

#[test]
fn usage_relay_supervisor_pid_parameter_selects_the_root_bypass() {
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let operation = UsageBrokerOperation::CurrentForCapability {
        capability: account,
    };
    let apple_supervisor = Some(supervisor_peer(
        jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID,
    ));
    assert!(authorization.authorizes(
        &supervisor(jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID),
        apple_supervisor,
        &operation,
    ));
    assert!(!authorization.authorizes(
        &supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID),
        apple_supervisor,
        &operation,
    ));
    // Same PID but a different bound starttime is a different process.
    let rebound = SupervisorIdentity {
        pid: jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID,
        starttime: IMPOSTOR_STARTTIME,
    };
    assert!(!authorization.authorizes(&rebound, apple_supervisor, &operation));
}

#[tokio::test]
async fn fused_relay_allows_apple_supervisor_and_denies_with_distinct_messages() {
    let (authorization, account_a) = single_session_authorization(2_001, 2_001, "account-a");
    let pid = jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID;
    let supervisor = supervisor(pid);

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
            supervisor,
            proxy_authorization,
            Some(supervisor_peer(pid)),
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
        supervisor,
        PeerIdentity {
            pid: Some(9),
            uid: 0,
            gid: 0,
            starttime: Some(SUPERVISOR_STARTTIME),
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

    // Root peer reusing the supervisor PID with a different starttime fails too.
    let denied = denied_relay_response(
        authorization.clone(),
        supervisor,
        impostor_peer(pid),
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
        supervisor,
        PeerIdentity {
            pid: None,
            uid: 2_001,
            gid: 2_001,
            starttime: None,
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
