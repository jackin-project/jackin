// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Cursor;

use jackin_protocol::attach::{
    ClientFrame, ClientTerminal, ClipboardImageFormat, ServerFrame, SpawnRequest, encode_server,
    read_client_frame,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

use super::*;

#[test]
fn normalize_size_substitutes_zero_and_clamps_minimums() {
    assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
    assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
    assert_eq!(normalize_size(40, 120), (40, 120));
}

#[tokio::test]
async fn clipboard_image_writer_keeps_small_images_single_frame() {
    let (mut client, mut server) = duplex(4096);
    let image = ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: b"\x89PNG\r\n\x1a\nsmall".to_vec(),
    };
    let mut operations = HashMap::new();

    write_clipboard_image_frames(&mut client, &mut operations, image.clone())
        .await
        .unwrap();
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let frame = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(request) = frame else {
        panic!("expected contextual clipboard image");
    };
    assert_eq!(
        request.operation,
        AttachControlOperation::ClipboardImage(image)
    );
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn clipboard_image_writer_chunks_large_images_with_digest() {
    let mut bytes = vec![b'x'; MAX_CONTEXTUAL_CLIPBOARD_IMAGE_BYTES + 1];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    let capacity = bytes.len() + 4096;
    let (mut client, mut server) = duplex(capacity);
    let expected_digest: [u8; 32] = Sha256::digest(&bytes).into();
    let mut operations = HashMap::new();

    write_clipboard_image_frames(
        &mut client,
        &mut operations,
        ClipboardImage {
            format: ClipboardImageFormat::Png,
            bytes: bytes.clone(),
        },
    )
    .await
    .unwrap();
    drop(client);

    let mut tag = [0u8; 1];
    server.read_exact(&mut tag).await.unwrap();
    let start = read_client_frame(&mut server, tag[0])
        .await
        .unwrap()
        .unwrap();
    let ClientFrame::AttachControl(request) = start else {
        panic!("expected contextual chunked image start");
    };
    let AttachControlOperation::ClipboardImageStart(start) = request.operation else {
        panic!("expected chunked image start");
    };
    assert_eq!(start.format, ClipboardImageFormat::Png);
    assert_eq!(start.size, bytes.len() as u64);

    let mut received = Vec::new();
    loop {
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        match frame {
            ClientFrame::AttachControl(AttachControlRequest {
                operation: AttachControlOperation::ClipboardImageChunk(chunk),
                ..
            }) => {
                assert_eq!(chunk.transfer_id, start.transfer_id);
                assert_eq!(chunk.offset, received.len() as u64);
                assert!(chunk.bytes.len() <= MAX_CLIPBOARD_IMAGE_CHUNK_BYTES);
                received.extend(chunk.bytes);
            }
            ClientFrame::AttachControl(AttachControlRequest {
                operation: AttachControlOperation::ClipboardImageEnd(end),
                ..
            }) => {
                assert_eq!(end.transfer_id, start.transfer_id);
                assert_eq!(end.sha256, expected_digest);
                break;
            }
            other => panic!("unexpected frame {other:?}"),
        }
    }

    assert_eq!(received, bytes);
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn explicit_clipboard_image_request_returns_probe_error_to_capsule() {
    let (mut client, mut server) = duplex(4096);
    let mut operations = HashMap::new();

    write_clipboard_image_request_result(
        &mut client,
        &mut operations,
        Err(anyhow::anyhow!(
            "Linux host clipboard image reader needs WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip"
        )),
        "host clipboard does not contain a readable image",
        "host clipboard image probe failed",
        "host clipboard image response failed",
    )
    .await;
    drop(client);

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
        panic!("expected ClipboardImageError");
    };

    assert_eq!(error.reason_code(), "backend-unavailable");
    assert!(error.message().contains("xclip/wl-paste"));
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[tokio::test]
async fn explicit_clipboard_path_request_mentions_file_url_support() {
    let (mut client, mut server) = duplex(4096);
    let mut operations = HashMap::new();

    write_clipboard_image_request_result(
        &mut client,
        &mut operations,
        Ok(None),
        "host clipboard text is not an absolute readable image path or file:// image URL",
        "host clipboard image path probe failed",
        "host clipboard image path response failed",
    )
    .await;
    drop(client);

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
        panic!("expected ClipboardImageError");
    };

    assert_eq!(error.reason_code(), "io");
    assert!(error.message().contains("host I/O failed"));
    assert_eq!(server.read(&mut tag).await.unwrap(), 0);
}

#[test]
fn host_file_export_finalizes_after_digest_match() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"export me";
    let sha256: [u8; 32] = Sha256::digest(bytes).into();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 99,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: bytes.len() as u64,
                reveal_after_export: true,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 99,
            offset: 0,
            bytes: bytes.to_vec(),
        })
        .unwrap();
    let completed = exports
        .end(FileExportEnd {
            transfer_id: 99,
            sha256,
        })
        .unwrap();

    assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), bytes);
    assert_eq!(completed.final_path, root.path().join("report.txt"));
    assert_eq!(completed.bytes, bytes.len() as u64);
    assert!(completed.reveal_after_export);
}

#[test]
fn host_file_export_rejects_digest_mismatch_and_removes_temp() {
    let root = tempfile::tempdir().unwrap();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 100,
                source_path: "/workspace/report.txt".into(),
                file_name: "../bad:name.txt".into(),
                size: 3,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 100,
            offset: 0,
            bytes: b"bad".to_vec(),
        })
        .unwrap();
    let err = exports
        .end(FileExportEnd {
            transfer_id: 100,
            sha256: [0; 32],
        })
        .expect_err("digest mismatch should reject export");

    assert!(format!("{err:#}").contains("SHA-256 mismatch"));
    assert!(!root.path().join("__bad_name.txt").exists());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn host_file_export_drop_removes_interrupted_temp_file() {
    let root = tempfile::tempdir().unwrap();
    {
        let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
        exports
            .start_in_root(
                FileExportStart {
                    transfer_id: 102,
                    source_path: "/workspace/report.txt".into(),
                    file_name: "report.txt".into(),
                    size: 9,
                    reveal_after_export: false,
                    open_after_export: false,
                },
                root.path(),
            )
            .unwrap();
        exports
            .chunk(FileExportChunk {
                transfer_id: 102,
                offset: 0,
                bytes: b"partial".to_vec(),
            })
            .unwrap();

        assert!(root.path().join("report.txt.part").exists());
        assert!(!root.path().join("report.txt").exists());
    }

    assert!(!root.path().join("report.txt.part").exists());
    assert!(!root.path().join("report.txt").exists());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn host_file_export_abort_removes_temp_and_rejects_end() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"export me";
    let sha256: [u8; 32] = Sha256::digest(bytes).into();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 103,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: bytes.len() as u64,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 103,
            offset: 0,
            bytes: b"export".to_vec(),
        })
        .unwrap();

    let err = exports
        .chunk(FileExportChunk {
            transfer_id: 103,
            offset: 0,
            bytes: b"bad-offset".to_vec(),
        })
        .expect_err("bad offset should reject export chunk");
    assert!(format!("{err:#}").contains("did not match expected"));

    exports.abort(103);
    assert!(!root.path().join("report.txt.part").exists());
    assert!(!root.path().join("report.txt").exists());
    let err = exports
        .end(FileExportEnd {
            transfer_id: 103,
            sha256,
        })
        .expect_err("aborted transfer should not finalize");
    assert!(format!("{err:#}").contains("has no active start"));
}

#[test]
fn host_file_export_idle_cleanup_removes_stale_temp_file() {
    let root = tempfile::tempdir().unwrap();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 104,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: 9,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 104,
            offset: 0,
            bytes: b"partial".to_vec(),
        })
        .unwrap();
    exports.active.get_mut(&104).unwrap().last_activity =
        Instant::now().checked_sub(Duration::from_secs(10)).unwrap();

    assert!(root.path().join("report.txt.part").exists());
    assert_eq!(exports.abort_idle_before(Instant::now()), 1);
    assert!(!root.path().join("report.txt.part").exists());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());

    let err = exports
        .end(FileExportEnd {
            transfer_id: 104,
            sha256: [0; 32],
        })
        .expect_err("stale transfer cleanup should remove active export");
    assert!(format!("{err:#}").contains("has no active start"));
}

#[test]
fn host_file_export_idle_cleanup_keeps_fresh_temp_file() {
    let root = tempfile::tempdir().unwrap();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());
    exports
        .start_in_root(
            FileExportStart {
                transfer_id: 105,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: 9,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .unwrap();
    exports
        .chunk(FileExportChunk {
            transfer_id: 105,
            offset: 0,
            bytes: b"partial".to_vec(),
        })
        .unwrap();

    assert_eq!(
        exports.abort_idle_before(Instant::now().checked_sub(Duration::from_secs(10)).unwrap()),
        0
    );
    assert!(root.path().join("report.txt.part").exists());
}

#[test]
fn unique_export_path_appends_counter() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("report.txt"), b"existing").unwrap();
    assert_eq!(
        unique_export_path(root.path(), "report.txt"),
        root.path().join("report-1.txt")
    );
}

#[test]
fn host_file_export_root_uses_sanitized_instance_subdir() {
    let root = host_file_export_root("../jk:agent/smith")
        .expect("home or downloads should resolve in tests");

    assert!(root.ends_with(Path::new("jackin").join("_jk_agent_smith")));
}

#[test]
fn export_source_path_category_names_supported_buckets() {
    assert_eq!(
        export_source_path_category("/jackin/run/clipboard/image.png"),
        "jackin-run"
    );
    assert_eq!(
        export_source_path_category("/jackin/state/marker"),
        "jackin-owned"
    );
    assert_eq!(
        export_source_path_category("/workspace/report.txt"),
        "container-absolute"
    );
    assert_eq!(
        export_source_path_category("relative/report.txt"),
        "container-relative"
    );
}

#[test]
fn host_file_export_compact_line_omits_full_paths() {
    let line = host_file_export_compact_line("workspace", "report.md", 123);

    assert_eq!(
        line,
        "host-file-export: exported source_category=workspace basename=\"report.md\" bytes=123 destination_category=host-downloads-jackin-instance"
    );
    assert!(!line.contains("/workspace"));
    assert!(!line.contains("Downloads"));
    assert!(!line.contains("/jackin/run"));
}

#[test]
fn host_file_basename_omits_parent_directories() {
    assert_eq!(
        host_file_basename(Path::new("/Users/operator/Downloads/jackin/report.md")),
        "report.md"
    );
    assert_eq!(host_file_basename(Path::new("/")), "jackin-export");
}

#[test]
fn host_file_export_start_does_not_overwrite_stale_temp_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("report.txt.part"), b"stale").unwrap();
    let mut exports = HostFileExports::new("jk-agent-smith".to_owned());

    let err = exports
        .start_in_root(
            FileExportStart {
                transfer_id: 101,
                source_path: "/workspace/report.txt".into(),
                file_name: "report.txt".into(),
                size: 3,
                reveal_after_export: false,
                open_after_export: false,
            },
            root.path(),
        )
        .expect_err("stale temp file should not be overwritten");

    assert!(format!("{err:#}").contains("creating temporary host export"));
    assert_eq!(
        fs::read(root.path().join("report.txt.part")).unwrap(),
        b"stale"
    );
}

#[tokio::test]
async fn attach_protocol_sends_hello_with_spawn_focus_env_and_terminal() {
    let (_export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let mut output = Vec::new();
    let request = HostAttachRequest {
        spawn_request: Some(SpawnRequest::AgentWithProvider {
            slug: "codex".to_owned(),
            provider_label: "MiniMax".to_owned(),
        }),
        focus_session: Some(42),
        env: vec![("JACKIN_GIT_DCO".to_owned(), "1".to_owned())],
        terminal: ClientTerminal {
            term: Some("xterm-ghostty".to_owned()),
            term_program: Some("ghostty".to_owned()),
            colorterm: None,
            default_fg: None,
            default_bg: None,
            ..ClientTerminal::default()
        },
        export_subdir: "jk-agent-smith".to_owned(),
    };

    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        let frame = read_client_frame(&mut server, tag[0])
            .await
            .unwrap()
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Welcome { session_count: 1 }))
            .await
            .unwrap();
        server
            .write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
            .await
            .unwrap();
        frame
    });

    let (_input_writer, input_reader) = duplex(64);
    let winch = signal(SignalKind::window_change()).unwrap();
    run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(&mut output),
        30,
        100,
        request,
        Vec::new(),
        winch,
    )
    .await
    .unwrap();

    let mut received = server_task.await.unwrap();
    let ClientFrame::Hello { context, .. } = &mut received else {
        panic!("host attach must begin with Hello")
    };
    let propagated = context
        .as_ref()
        .expect("Hello must carry telemetry context");
    assert!(propagated.traceparent.is_some());
    *context = Some(Box::new(jackin_protocol::TelemetryContext::v1()));
    assert_eq!(
        received,
        ClientFrame::Hello {
            context: Some(Box::new(jackin_protocol::TelemetryContext::v1())),
            rows: 30,
            cols: 100,
            spawn: Some(SpawnRequest::AgentWithProvider {
                slug: "codex".to_owned(),
                provider_label: "MiniMax".to_owned(),
            }),
            env: vec![("JACKIN_GIT_DCO".to_owned(), "1".to_owned())],
            focus_session: Some(42),
            terminal: ClientTerminal {
                term: Some("xterm-ghostty".to_owned()),
                term_program: Some("ghostty".to_owned()),
                colorterm: None,
                default_fg: None,
                default_bg: None,
                ..ClientTerminal::default()
            },
        }
    );
}

#[tokio::test]
async fn attach_protocol_forwards_terminal_input_as_input_frames() {
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

    let (mut input_writer, input_reader) = duplex(64);
    input_writer.write_all(b"abc").await.unwrap();
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

    assert_eq!(
        server_task.await.unwrap(),
        ClientFrame::Input(b"abc".to_vec())
    );
}

#[tokio::test]
async fn attach_protocol_preserves_bracketed_paste_and_mouse_bytes() {
    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let request = HostAttachRequest {
        spawn_request: None,
        focus_session: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        export_subdir: "jk-agent-smith".to_owned(),
    };
    let raw_input = b"\x1b[200~/tmp/example.png\x1b[201~\x1b[<0;12;5M\x1b[<0;12;5m".to_vec();

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

    assert_eq!(server_task.await.unwrap(), ClientFrame::Input(raw_input));
}

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
    let mut operations = HashMap::new();

    send_clipboard_image_error(&mut client, &mut operations, &message)
        .await
        .unwrap();
    send_clipboard_image_error(&mut client, &mut operations, "   ")
        .await
        .unwrap();
    drop(client);

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
        panic!("expected ClipboardImageError");
    };
    assert_eq!(error.message().len(), MAX_CLIPBOARD_IMAGE_ERROR_BYTES);
    assert!(error.message().ends_with("..."));

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
