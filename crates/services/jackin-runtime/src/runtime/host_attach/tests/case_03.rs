// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn attach_protocol_auto_stages_bracketed_image_path_paste() {
    let temp = tempfile::tempdir().unwrap();
    let image_path = temp.path().join("shot.png");
    fs::write(&image_path, b"\x89PNG\r\n\x1a\npayload").unwrap();

    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    let mut raw_input = b"\x1b[200~".to_vec();
    raw_input.extend_from_slice(image_path.display().to_string().as_bytes());
    raw_input.extend_from_slice(b"\x1b[201~");

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        frame
    });

    let (mut input_writer, input_reader) = duplex(128);
    input_writer.write_all(&raw_input).await.unwrap();
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();

    // The pasted host image path is staged as an image frame, not forwarded
    // as the raw path text.
    match server_task.await.unwrap() {
        ClientFrame::AttachControl(AttachControlRequest {
            operation: AttachControlOperation::ClipboardImage(image),
            ..
        }) => {
            assert_eq!(image.format, ClipboardImageFormat::Png);
            assert_eq!(image.bytes, b"\x89PNG\r\n\x1a\npayload");
        }
        other => panic!("expected staged ClipboardImage frame, got {other:?}"),
    }
}

#[tokio::test]
async fn attach_protocol_forwards_bytes_around_a_staged_paste() {
    let temp = tempfile::tempdir().unwrap();
    let image_path = temp.path().join("shot.png");
    fs::write(&image_path, b"\x89PNG\r\n\x1a\npayload").unwrap();

    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    // A mouse report shares the read after the paste end marker.
    let mut raw_input = b"\x1b[200~".to_vec();
    raw_input.extend_from_slice(image_path.display().to_string().as_bytes());
    raw_input.extend_from_slice(b"\x1b[201~\x1b[<0;1;1M");

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let image = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let trailing = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        (image, trailing)
    });

    let (mut input_writer, input_reader) = duplex(128);
    input_writer.write_all(&raw_input).await.unwrap();
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();

    // The image stages, and the coincident mouse report is forwarded rather
    // than dropped with the consumed paste body.
    let (image, trailing) = server_task.await.unwrap();
    assert!(matches!(
        image,
        ClientFrame::AttachControl(AttachControlRequest {
            operation: AttachControlOperation::ClipboardImage(_),
            ..
        })
    ));
    assert_eq!(trailing, ClientFrame::Input(b"\x1b[<0;1;1M".to_vec()));
}

#[tokio::test]
async fn attach_protocol_forwards_typed_prefix_before_a_staged_paste() {
    let temp = tempfile::tempdir().unwrap();
    let image_path = temp.path().join("shot.png");
    fs::write(&image_path, b"\x89PNG\r\n\x1a\npayload").unwrap();

    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    // Type-ahead bytes precede the paste in the same read.
    let mut raw_input = b"ab\x1b[200~".to_vec();
    raw_input.extend_from_slice(image_path.display().to_string().as_bytes());
    raw_input.extend_from_slice(b"\x1b[201~");

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let first = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let second = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        (first, second)
    });

    let (mut input_writer, input_reader) = duplex(128);
    input_writer.write_all(&raw_input).await.unwrap();
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();

    // The typed prefix reaches the agent BEFORE the staged image, preserving
    // wire order.
    let (first, second) = server_task.await.unwrap();
    assert_eq!(first, ClientFrame::Input(b"ab".to_vec()));
    assert!(matches!(
        second,
        ClientFrame::AttachControl(AttachControlRequest {
            operation: AttachControlOperation::ClipboardImage(_),
            ..
        })
    ));
}

#[tokio::test]
async fn attach_protocol_forwards_prefix_image_suffix_in_wire_order() {
    let temp = tempfile::tempdir().unwrap();
    let image_path = temp.path().join("shot.png");
    fs::write(&image_path, b"\x89PNG\r\n\x1a\npayload").unwrap();

    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    // Type-ahead before the paste and a mouse report after, all one read.
    let mut raw_input = b"ab\x1b[200~".to_vec();
    raw_input.extend_from_slice(image_path.display().to_string().as_bytes());
    raw_input.extend_from_slice(b"\x1b[201~\x1b[<0;1;1M");

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        let mut frames = Vec::new();
        for _ in 0..3 {
            server.read_exact(&mut tag).await.unwrap();
            frames.push(
                read_client_frame(&mut server, tag[0])
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        frames
    });

    let (mut input_writer, input_reader) = duplex(128);
    input_writer.write_all(&raw_input).await.unwrap();
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();

    // Exactly three frames, in wire order: prefix, image, suffix.
    let frames = server_task.await.unwrap();
    assert_eq!(frames[0], ClientFrame::Input(b"ab".to_vec()));
    assert!(matches!(
        frames[1],
        ClientFrame::AttachControl(AttachControlRequest {
            operation: AttachControlOperation::ClipboardImage(_),
            ..
        })
    ));
    assert_eq!(frames[2], ClientFrame::Input(b"\x1b[<0;1;1M".to_vec()));
}

#[tokio::test]
async fn attach_protocol_forwards_unresolved_image_path_paste_as_text() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.png");

    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    let mut raw_input = b"\x1b[200~".to_vec();
    raw_input.extend_from_slice(missing.display().to_string().as_bytes());
    raw_input.extend_from_slice(b"\x1b[201~");

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let _hello = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        frame
    });

    let (mut input_writer, input_reader) = duplex(128);
    input_writer.write_all(&raw_input).await.unwrap();
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();

    // A recognized-but-unresolved image path is forwarded verbatim as text,
    // never silently eaten.
    assert_eq!(server_task.await.unwrap(), ClientFrame::Input(raw_input));
}
