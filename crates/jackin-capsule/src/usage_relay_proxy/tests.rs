// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_PROTOCOL_VERSION, UsageBrokerOperation, UsageCoordinationError,
};
use tokio::io::BufReader;

#[tokio::test]
async fn broker_client_stdio_proxy_multiplexes_out_of_order_responses() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("usage.sock");
    let (mut host_response_writer, proxy_input) = tokio::io::duplex(64 * 1024);
    let (proxy_output, host_request_reader) = tokio::io::duplex(64 * 1024);
    let proxy_socket = socket.clone();
    let proxy = tokio::spawn(async move {
        run_at(
            &proxy_socket,
            DEFAULT_CAPSULE_SUPERVISOR_PID,
            proxy_input,
            proxy_output,
        )
        .await
    });
    wait_for_socket(&socket).await;

    let first = tokio::spawn(send_request(socket.clone(), "claude"));
    let second = tokio::spawn(send_request(socket, "codex"));
    let mut requests = BufReader::new(host_request_reader);
    let mut frames = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        requests.read_line(&mut line).await.unwrap();
        frames.push(serde_json::from_str::<UsageRelayTunnelRequest>(line.trim()).unwrap());
    }
    frames.reverse();
    for frame in frames {
        let surface = match frame.request.operation {
            UsageBrokerOperation::CurrentForSurface { surface_id } => surface_id,
            operation => panic!("unexpected operation: {operation:?}"),
        };
        let response = UsageRelayTunnelResponse {
            request_id: frame.request_id,
            response: UsageBrokerResponse::Error {
                error: UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::Unauthorized,
                    message: surface,
                },
            },
        };
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        host_response_writer.write_all(&bytes).await.unwrap();
    }

    assert_eq!(error_message(first.await.unwrap()), "claude");
    assert_eq!(error_message(second.await.unwrap()), "codex");
    proxy.abort();
}

async fn send_request(socket: std::path::PathBuf, surface: &str) -> UsageBrokerResponse {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        operation: UsageBrokerOperation::CurrentForSurface {
            surface_id: surface.to_owned(),
        },
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
