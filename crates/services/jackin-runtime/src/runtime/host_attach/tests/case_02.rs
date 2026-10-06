// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let mut output = Vec::new();
    let request = HostAttachRequest {
        spawn_request: Some(SpawnRequest::Instance("codex".to_owned())),
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
            spawn: Some(SpawnRequest::Instance("codex".to_owned())),
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
    export.force_flush();
    let spans = export.finished_spans();
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "rpc.client")
            .count(),
        1
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "stream.operation")
            .count(),
        2
    );
    assert_eq!(export.error_span_count(), 0);
    assert!(export.contains_span_text("open"));
    assert!(export.contains_span_text("close"));
}

#[tokio::test]
async fn attach_protocol_marks_close_error_after_welcome_eof() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let (client, mut server) = duplex(4096);
    let (client_reader, client_writer) = tokio::io::split(client);
    let server_task = tokio::spawn(async move {
        let mut tag = [0u8; 1];
        server.read_exact(&mut tag).await.unwrap();
        drop(read_client_frame(&mut server, tag[0]).await.unwrap());
        server
            .write_all(&encode_server(ServerFrame::Welcome { session_count: 1 }))
            .await
            .unwrap();
    });
    let (_input_writer, input_reader) = duplex(64);
    let winch = signal(SignalKind::window_change()).unwrap();
    let result = run_attach_protocol(
        client_reader,
        client_writer,
        input_reader,
        Cursor::new(Vec::<u8>::new()),
        24,
        80,
        HostAttachRequest {
            spawn_request: None,
            focus_session: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            export_subdir: "jk-agent-smith".to_owned(),
        },
        Vec::new(),
        winch,
    )
    .await;
    server_task.await.unwrap();
    assert!(result.is_err());
    export.force_flush();
    let spans = export.finished_spans();
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "stream.operation")
            .count(),
        2
    );
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("rpc_error"));
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
