// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{Semaphore, mpsc},
};

use super::{ClientFrame, handle_attach_client};

#[tokio::test(flavor = "current_thread")]
async fn control_socket_exports_client_parent_server_and_completes_after_reply_write() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str("jackin.capsule.Control/Status"),
        },
    ];
    let client_operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs)
            .expect("client operation");
    let mut context = jackin_protocol::TelemetryContext::v1();
    client_operation
        .span()
        .in_scope(|| jackin_telemetry::propagation::inject(&mut context));
    let request = jackin_protocol::control::ControlRequest {
        ctx: context,
        msg: jackin_protocol::control::ClientMsg::Status,
    };
    let (mut server, mut client) = UnixStream::pair().expect("control socket pair");
    client
        .write_all(&jackin_protocol::control::frame(&request))
        .await
        .expect("write request");
    let first_tag = server.read_u8().await.expect("read dispatcher tag");

    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let permit = Arc::new(Semaphore::new(1))
        .acquire_owned()
        .await
        .expect("control permit");
    let handshake = tokio::spawn(super::perform_control_handshake(
        server,
        first_tag,
        permit,
        control_tx,
        std::time::Duration::from_secs(1),
    ));
    let dispatched = control_rx.recv().await.expect("daemon dispatch");
    let server_operation =
        crate::daemon::control_server_operation(&dispatched.ctx, &dispatched.msg)
            .expect("valid correlation");
    dispatched
        .reply_tx
        .send(super::ControlResponse {
            msg: jackin_protocol::control::ServerMsg::SessionList {
                sessions: Vec::new(),
            },
            operation: server_operation,
            outcome: jackin_telemetry::schema::enums::OutcomeValue::Success,
            error_type: None,
        })
        .expect("send daemon reply");

    let mut len = [0_u8; 4];
    client
        .read_exact(&mut len)
        .await
        .expect("read reply length");
    let mut body = vec![0_u8; u32::from_be_bytes(len) as usize];
    client.read_exact(&mut body).await.expect("read reply body");
    drop(
        serde_json::from_slice::<jackin_protocol::control::ServerMsg>(&body).expect("decode reply"),
    );
    handshake.await.expect("handshake task");
    client_operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    drop(guard);
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 2);
    let client_span = spans
        .iter()
        .find(|span| span.name == "rpc.client")
        .expect("client span");
    let server_span = spans
        .iter()
        .find(|span| span.name == "rpc.server")
        .expect("server span");
    assert_eq!(server_span.trace_id, client_span.trace_id);
    assert_eq!(server_span.parent_span_id, client_span.span_id);
    assert!(!client_span.error && !server_span.error);
}

#[tokio::test(flavor = "current_thread")]
async fn control_socket_marks_server_failure_when_peer_closes_before_reply() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let mut context = jackin_protocol::TelemetryContext::v1();
    context.traceparent =
        Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned());
    let request = jackin_protocol::control::ControlRequest {
        ctx: context,
        msg: jackin_protocol::control::ClientMsg::Status,
    };
    let (mut server, mut client) = UnixStream::pair().expect("control socket pair");
    client
        .write_all(&jackin_protocol::control::frame(&request))
        .await
        .expect("write request");
    let first_tag = server.read_u8().await.expect("read dispatcher tag");
    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let permit = Arc::new(Semaphore::new(1))
        .acquire_owned()
        .await
        .expect("control permit");
    let handshake = tokio::spawn(super::perform_control_handshake(
        server,
        first_tag,
        permit,
        control_tx,
        std::time::Duration::from_secs(1),
    ));
    let dispatched = control_rx.recv().await.expect("daemon dispatch");
    let server_operation =
        crate::daemon::control_server_operation(&dispatched.ctx, &dispatched.msg)
            .expect("valid correlation");
    drop(client);
    dispatched
        .reply_tx
        .send(super::ControlResponse {
            msg: jackin_protocol::control::ServerMsg::SessionList {
                sessions: Vec::new(),
            },
            operation: server_operation,
            outcome: jackin_telemetry::schema::enums::OutcomeValue::Success,
            error_type: None,
        })
        .expect("send daemon reply");
    handshake.await.expect("handshake task");
    drop(guard);
    export.force_flush();
    assert_eq!(export.error_span_count(), 1);
}

#[test]
fn control_server_operation_completes_from_socket_write_result() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _guard = tracing::subscriber::set_default(subscriber);
    let operation = jackin_telemetry::operation(
        &jackin_telemetry::operation::RPC_SERVER,
        &[
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
                value: jackin_telemetry::Value::Str("jackin"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
                value: jackin_telemetry::Value::Str("jackin.capsule.Control/Status"),
            },
        ],
    )
    .ok();
    super::ControlResponse {
        msg: jackin_protocol::control::ServerMsg::Ack,
        operation,
        outcome: jackin_telemetry::schema::enums::OutcomeValue::Success,
        error_type: None,
    }
    .complete(&Err(anyhow::anyhow!("peer closed")));
    export.force_flush();
    assert_eq!(export.error_span_count(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn conformance_attach_protocol_failure_and_expected_detach() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);

    let (server, mut client) = UnixStream::pair().expect("create attach socket pair");
    let (_out_tx, out_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    client.write_all(&[1]).await.expect("write frame tag");
    client.shutdown().await.expect("close frame mid-payload");
    handle_attach_client(server, out_rx, cmd_tx).await;
    assert!(matches!(cmd_rx.recv().await, Some(ClientFrame::Detach)));

    drop(guard);
    export.force_flush();
    assert_eq!(
        export.typed_error_count("error.typed", "attach_socket_eof"),
        1
    );
    assert_eq!(export.error_span_count(), 1);

    let (clean_export, clean_subscriber) =
        jackin_diagnostics::observability::test_capsule_layers(false);
    let clean_guard = tracing::subscriber::set_default(clean_subscriber);
    let (server, mut client) = UnixStream::pair().expect("create clean attach socket pair");
    let (_out_tx, out_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    client.shutdown().await.expect("close clean attach socket");
    handle_attach_client(server, out_rx, cmd_tx).await;
    assert!(matches!(cmd_rx.recv().await, Some(ClientFrame::Detach)));

    drop(clean_guard);
    clean_export.force_flush();
    assert_eq!(clean_export.error_span_count(), 0);
    assert_eq!(
        clean_export.typed_error_count("error.typed", "attach_socket_eof"),
        0
    );
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_uncontextual_control_is_rejected_before_daemon_dispatch() {
    let (server, mut client) = UnixStream::pair().expect("create attach socket pair");
    let (_out_tx, out_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    client
        .write_all(&jackin_protocol::attach::encode_client(ClientFrame::Detach).unwrap())
        .await
        .expect("write legacy detach");
    client.shutdown().await.expect("close attach socket");

    handle_attach_client(server, out_rx, cmd_tx).await;

    assert!(matches!(cmd_rx.recv().await, Some(ClientFrame::Detach)));
    cmd_rx.try_recv().unwrap_err();
}
