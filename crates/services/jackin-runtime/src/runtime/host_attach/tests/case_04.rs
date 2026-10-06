// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn attach_protocol_forwards_initial_query_leftovers_as_input() {
    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let input = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        input
    });

    let (_input_writer, input_reader) = duplex(64);
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        request,
        b"typed-before-attach".to_vec(),
        winch,
    )
    .await
    .unwrap();

    assert_eq!(
        server_task.await.unwrap(),
        ClientFrame::Input(b"typed-before-attach".to_vec())
    );
}

#[tokio::test]
async fn attach_protocol_writes_osc52_output_unchanged() {
    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let mut output = Vec::new();
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    let osc52 = b"\x1b]52;c;c2VsZWN0ZWQ=\x07".to_vec();

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Output(osc52)))
            .await
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
    });

    let (_input_writer, input_reader) = duplex(64);
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(&mut output),
        24,
        80,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();
    server_task.await.unwrap();

    assert_eq!(output, b"\x1b]52;c;c2VsZWN0ZWQ=\x07");
}

#[tokio::test]
async fn host_notice_writer_sends_typed_protocol_frame() {
    let (mut client, mut server) = duplex(4096);

    send_host_notice(&mut client, "File exported: ~/Downloads/jackin/report.txt")
        .await
        .unwrap();
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        frame,
        ClientFrame::HostNotice("File exported: ~/Downloads/jackin/report.txt".to_owned())
    );
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn host_notice_writer_bounds_overlong_message() {
    let (mut client, mut server) = duplex(MAX_HOST_NOTICE_BYTES + 64);
    let message = format!("{}{}", "a".repeat(MAX_HOST_NOTICE_BYTES), "é");

    send_host_notice(&mut client, &message).await.unwrap();
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();

    let ClientFrame::HostNotice(message) = frame else {
        panic!("expected HostNotice");
    };
    assert_eq!(message.len(), MAX_HOST_NOTICE_BYTES);
    assert!(message.ends_with("..."));
}

#[tokio::test]
async fn clipboard_image_error_writer_bounds_empty_and_overlong_message() {
    let (mut client, mut server) = duplex(MAX_CLIPBOARD_IMAGE_ERROR_BYTES + 64);
    let message = format!("{}{}", "b".repeat(MAX_CLIPBOARD_IMAGE_ERROR_BYTES), "é");
    // The bounded peer must drain while the writer is active: production peers
    // read concurrently, and a sequential test peer deadlocks on the second
    // frame when contextual attach telemetry fills the duplex buffer.

    let writer = async {
        let mut operations = HashMap::new();
        send_clipboard_image_error(&mut client, &mut operations, &message).await?;
        send_clipboard_image_error(&mut client, &mut operations, "   ").await
    };
    let first_frame = async {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await?;
        read_client_frame(&mut server, tag[0])
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected first ClipboardImageError frame"))
    };
    let (write_result, first_result) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(writer, first_frame)
    })
    .await
    .expect("clipboard image error writer and reader must make progress together");
    write_result.unwrap();
    let frame = first_result.unwrap();

    let ClientFrame::AttachControl(AttachControlRequest {
        operation: AttachControlOperation::ClipboardImageError(error),
        ..
    }) = frame
    else {
        panic!("expected ClipboardImageError");
    };
    assert_eq!(error.message().len(), MAX_CLIPBOARD_IMAGE_ERROR_BYTES);
    assert!(error.message().ends_with("..."));

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(AttachControlRequest {
        operation: AttachControlOperation::ClipboardImageError(error),
        ..
    }) = frame
    else {
        panic!("expected contextual ClipboardImageError");
    };
    assert_eq!(error.message(), "Host action failed");
}
