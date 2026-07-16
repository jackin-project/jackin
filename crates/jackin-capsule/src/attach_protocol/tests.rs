// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use tokio::{io::AsyncWriteExt, net::UnixStream, sync::mpsc};

use super::{ClientFrame, handle_attach_client};

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
