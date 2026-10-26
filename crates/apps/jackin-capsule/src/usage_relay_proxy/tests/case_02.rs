// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::task::{Context, Poll};

use tokio::io::ReadBuf;

#[tokio::test]
async fn fused_relay_allows_apple_supervisor_and_denies_with_distinct_messages() {
    let (authorization, account_a) = single_session_authorization(2_001, 2_001, "account-a");
    let supervisor_pid = jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID;

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
            Arc::new(|_| true),
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

    let impostor = root_peer(
        DEFAULT_CAPSULE_SUPERVISOR_PID,
        Some(SUPERVISOR_START_TIME + 1),
    );
    assert!(!binding.matches(impostor));
    assert!(!supervisor_peer_allows(binding, Some(impostor)));
    assert!(!authorization.authorizes(binding, Some(impostor), &operation));

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

    let unverified = SupervisorIdentity {
        pid: DEFAULT_CAPSULE_SUPERVISOR_PID,
        start_time: None,
    };
    let peer = root_peer(DEFAULT_CAPSULE_SUPERVISOR_PID, Some(SUPERVISOR_START_TIME));
    assert!(!unverified.matches(peer));
    assert!(!supervisor_peer_allows(unverified, Some(peer)));
    assert!(!authorization.authorizes(unverified, Some(peer), &operation));

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

#[test]
fn relay_authorization_rejects_host_monitor_operations_for_sessions_and_supervisor() {
    let (authorization, _) = single_session_authorization(2_001, 2_001, "account-a");
    let operation = UsageBrokerOperation::Monitor {
        request: MonitorOperation::ServiceStatus,
    };
    let binding = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    let session_peer = PeerIdentity {
        pid: Some(77),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let supervisor_peer = root_peer(DEFAULT_CAPSULE_SUPERVISOR_PID, Some(SUPERVISOR_START_TIME));

    assert!(!authorization.authorizes(binding, Some(session_peer), &operation));
    assert!(!authorization.authorizes(binding, Some(supervisor_peer), &operation));
}

#[test]
fn capsule_proxy_rejects_host_launch_capability_resolution() {
    let (authorization, _) = single_session_authorization(2_001, 2_001, "account-a");
    let operation = UsageBrokerOperation::ResolveRelayCapabilities {
        scope_label: "workspace fixture role reviewer".to_owned(),
        forwarded_sources: jackin_protocol::usage_broker::UsageRelayForwardedSourcesV1::default(),
    };
    let binding = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    let session_peer = PeerIdentity {
        pid: Some(77),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let supervisor_peer = root_peer(DEFAULT_CAPSULE_SUPERVISOR_PID, Some(SUPERVISOR_START_TIME));

    assert!(!authorization.authorizes(binding, Some(session_peer), &operation));
    assert!(!authorization.authorizes(binding, Some(supervisor_peer), &operation));
}

#[cfg(target_os = "linux")]
#[test]
fn process_start_time_is_stable_for_self_and_absent_for_missing_pid() {
    let pid = std::process::id();
    let first = process_start_time(pid);
    assert!(first.is_some_and(|start| start > 0));
    assert_eq!(first, process_start_time(pid));
    assert_eq!(process_start_time(u32::MAX), None);
    assert!(
        SupervisorIdentity {
            pid,
            start_time: first,
        }
        .is_current()
    );
    assert!(
        !SupervisorIdentity {
            pid,
            start_time: first.map(|start| start.saturating_add(1)),
        }
        .is_current()
    );
    assert!(
        !SupervisorIdentity {
            pid,
            start_time: None,
        }
        .is_current()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn parse_proc_stat_start_time_skips_comm_with_parens_and_spaces() {
    let mut fields = vec!["R".to_owned()];
    fields.extend((4..=21).map(|field| field.to_string()));
    fields.push("987654".to_owned());
    fields.extend(["0".to_owned(), "0".to_owned()]);
    let stat = format!("42 (my ) proc) {}", fields.join(" "));
    assert_eq!(parse_proc_stat_start_time(&stat), Some(987_654));
    assert_eq!(parse_proc_stat_start_time("bogus"), None);
}

#[cfg(target_os = "linux")]
#[test]
fn proc_stat_start_time_rejects_dead_and_malformed_process_state() {
    let stat = |state: &str, start_time: &str| {
        let mut fields = vec![state.to_owned()];
        fields.extend((4..=21).map(|field| field.to_string()));
        fields.push(start_time.to_owned());
        format!("42 (my ) proc) {}", fields.join(" "))
    };

    for state in ["Z", "X", "x", "?", "RR"] {
        assert_eq!(
            parse_proc_stat_start_time(&stat(state, "987654")),
            None,
            "state {state:?} must fail closed"
        );
    }
    assert_eq!(parse_proc_stat_start_time(&stat("R", "not-a-number")), None);
    assert_eq!(parse_proc_stat_start_time("42 (proc) R 4 5"), None);
}

#[tokio::test]
async fn usage_relay_delayed_first_request_rejects_ended_supervisor_without_dispatch() {
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let peers = [
        root_peer(supervisor.pid, supervisor.start_time),
        PeerIdentity {
            pid: Some(77),
            start_time: Some(SUPERVISOR_START_TIME),
            uid: 2_001,
            gid: 2_001,
        },
    ];

    for peer in peers {
        let live = Arc::new(AtomicBool::new(true));
        let checks = Arc::new(AtomicUsize::new(0));
        let correct_identity = Arc::new(AtomicBool::new(false));
        let callback_live = Arc::clone(&live);
        let callback_checks = Arc::clone(&checks);
        let callback_identity = Arc::clone(&correct_identity);
        let liveness: SupervisorLiveness = Arc::new(move |actual| {
            callback_checks.fetch_add(1, AtomicOrdering::SeqCst);
            callback_identity.store(actual == supervisor, AtomicOrdering::SeqCst);
            callback_live.load(AtomicOrdering::SeqCst)
        });
        let (server, mut client) = UnixStream::pair().unwrap();
        let (pending_read_tx, pending_read_rx) = oneshot::channel();
        let server = PendingReadSignal {
            inner: server,
            pending: Some(pending_read_tx),
        };
        let (requests, mut request_rx) = mpsc::channel(1);
        let requests_keepalive = requests.clone();
        let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
        let handler = tokio::spawn(handle_local(
            server,
            1,
            requests,
            Arc::clone(&pending),
            Arc::new(authorization.clone()),
            Some(peer),
            SupervisorCheck {
                identity: supervisor,
                is_current: liveness,
            },
        ));

        tokio::time::timeout(Duration::from_secs(5), pending_read_rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(checks.load(AtomicOrdering::SeqCst), 0);
        live.store(false, AtomicOrdering::SeqCst);

        let request = current_request(account.clone());
        write_request(&mut client, &request).await;
        let response = tokio::time::timeout(Duration::from_secs(5), read_response(&mut client))
            .await
            .unwrap();
        assert_eq!(
            error_message(response),
            "usage relay peer is not the Capsule supervisor"
        );
        handler.await.unwrap();

        assert_eq!(checks.load(AtomicOrdering::SeqCst), 1);
        assert!(correct_identity.load(AtomicOrdering::SeqCst));
        assert!(pending.lock().await.is_empty());
        assert!(matches!(
            request_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        drop(requests_keepalive);
    }
}

#[tokio::test]
async fn usage_relay_live_supervisor_dispatches_exact_authorized_request() {
    let supervisor = supervisor(DEFAULT_CAPSULE_SUPERVISOR_PID);
    let (authorization, account) = single_session_authorization(2_001, 2_001, "account-a");
    let peer = PeerIdentity {
        pid: Some(77),
        start_time: Some(SUPERVISOR_START_TIME),
        uid: 2_001,
        gid: 2_001,
    };
    let checks = Arc::new(AtomicUsize::new(0));
    let callback_checks = Arc::clone(&checks);
    let correct_identity = Arc::new(AtomicBool::new(false));
    let callback_identity = Arc::clone(&correct_identity);
    let liveness: SupervisorLiveness = Arc::new(move |actual| {
        callback_checks.fetch_add(1, AtomicOrdering::SeqCst);
        callback_identity.store(actual == supervisor, AtomicOrdering::SeqCst);
        true
    });
    let (server, mut client) = UnixStream::pair().unwrap();
    let (pending_read_tx, pending_read_rx) = oneshot::channel();
    let server = PendingReadSignal {
        inner: server,
        pending: Some(pending_read_tx),
    };
    let (requests, mut request_rx) = mpsc::channel(1);
    let requests_keepalive = requests.clone();
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let handler = tokio::spawn(handle_local(
        server,
        1,
        requests,
        Arc::clone(&pending),
        Arc::new(authorization),
        Some(peer),
        SupervisorCheck {
            identity: supervisor,
            is_current: liveness,
        },
    ));

    tokio::time::timeout(Duration::from_secs(5), pending_read_rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checks.load(AtomicOrdering::SeqCst), 0);

    let request = current_request(account);
    write_request(&mut client, &request).await;
    let tunneled = tokio::time::timeout(Duration::from_secs(5), request_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tunneled.request, request);
    assert_eq!(checks.load(AtomicOrdering::SeqCst), 1);
    assert!(correct_identity.load(AtomicOrdering::SeqCst));
    assert!(matches!(
        request_rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));

    let response = UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "live request completed".to_owned(),
        },
    };
    let waiter = pending.lock().await.remove(&tunneled.request_id).unwrap();
    drop(waiter.send(response));
    let response = tokio::time::timeout(Duration::from_secs(5), read_response(&mut client))
        .await
        .unwrap();
    assert_eq!(error_message(response), "live request completed");
    handler.await.unwrap();
    assert!(pending.lock().await.is_empty());
    assert!(matches!(
        request_rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    drop(requests_keepalive);
}

struct PendingReadSignal<S> {
    inner: S,
    pending: Option<oneshot::Sender<()>>,
}

impl<S> AsyncRead for PendingReadSignal<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_read(context, buffer);
        if result.is_pending()
            && let Some(pending) = this.pending.take()
        {
            let _send_result = pending.send(());
        }
        result
    }
}

impl<S> AsyncWrite for PendingReadSignal<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(context, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(context)
    }
}

fn current_request(capability: UsageAccountCapability) -> UsageBrokerRequest {
    UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        operation: UsageBrokerOperation::CurrentForCapability { capability },
        launch_credential_scope: None,
    }
}

async fn write_request(stream: &mut UnixStream, request: &UsageBrokerRequest) {
    let mut bytes = serde_json::to_vec(request).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).await.unwrap();
    stream.shutdown().await.unwrap();
}

async fn read_response(stream: &mut UnixStream) -> UsageBrokerResponse {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await.unwrap();
    serde_json::from_str(line.trim()).unwrap()
}
