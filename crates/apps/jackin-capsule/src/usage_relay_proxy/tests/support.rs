// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn capability(account_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: "claude".to_owned(),
    }
}

pub(super) async fn send_request(
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

pub(super) fn error_message(response: UsageBrokerResponse) -> String {
    let UsageBrokerResponse::Error { error } = response else {
        panic!("expected error response");
    };
    error.message
}

pub(super) async fn wait_for_socket(socket: &Path) {
    for _ in 0..100 {
        if socket.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("usage proxy socket was not created");
}

pub(super) fn single_session_authorization(
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

pub(super) async fn denied_relay_response(
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

pub(super) const SUPERVISOR_START_TIME: u64 = 123_456;

pub(super) fn supervisor(pid: u32) -> SupervisorIdentity {
    SupervisorIdentity {
        pid,
        start_time: Some(SUPERVISOR_START_TIME),
    }
}

pub(super) fn root_peer(pid: u32, start_time: Option<u64>) -> PeerIdentity {
    PeerIdentity {
        pid: Some(pid),
        start_time,
        uid: 0,
        gid: 0,
    }
}
