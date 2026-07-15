// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use tokio::{io::AsyncWriteExt, net::UnixStream, sync::mpsc};

use super::{ClientFrame, handle_attach_client};

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
        export.typed_error_count("capsule.attach", "attach_socket_eof"),
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
        clean_export.typed_error_count("capsule.attach", "attach_socket_eof"),
        0
    );
}
