// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn contextual_attach_control_and_response_roundtrip() {
    let request = ClientFrame::AttachControl(AttachControlRequest {
        request_id: 41,
        context: TelemetryContext::v1(),
        operation: AttachControlOperation::FocusIn,
    });
    let encoded = encode_client(request.clone()).unwrap();
    assert_eq!(
        decode_client(encoded[0], encoded[5..].to_vec()).unwrap(),
        request
    );

    let response = ServerFrame::AttachControlResponse(AttachControlResponse {
        request_id: 41,
        result: AttachControlResult::Success,
    });
    let encoded = encode_server(response.clone());
    assert_eq!(
        decode_server(encoded[0], encoded[5..].to_vec()).unwrap(),
        response
    );
}

#[test]
fn hot_path_output_avoids_base64_and_json() {
    // Regression for the first attempt's `base64-inside-JSON` hot path:
    // a 4 KiB chunk of raw PTY bytes must travel through the attach
    // channel with only 5 bytes of overhead (tag + length).
    let payload = vec![0xCDu8; 4096];
    let frame = encode_server(ServerFrame::Output(payload.clone()));
    assert_eq!(frame.len(), 5 + payload.len());
    assert_eq!(frame[0], TAG_OUTPUT);
    assert_eq!(&frame[1..5], &(payload.len() as u32).to_be_bytes());
    assert_eq!(&frame[5..], &payload[..]);
}

#[test]
fn hello_roundtrips() {
    let bytes = encode_client(ClientFrame::Hello {
        rows: 42,
        cols: 100,
        spawn: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .unwrap();
    // First byte is tag, never `0x00` (which is reserved for the
    // control-channel JSON length high byte).
    assert_eq!(bytes[0], TAG_HELLO);
    assert_ne!(bytes[0], 0x00);
}

#[test]
fn hello_with_spawn_shell_roundtrips() {
    let bytes = encode_client(ClientFrame::Hello {
        rows: 50,
        cols: 200,
        spawn: Some(SpawnRequest::Shell),
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .unwrap();
    let payload = bytes[5..].to_vec();
    let frame = decode_client(TAG_HELLO, payload).unwrap();
    assert_eq!(
        frame,
        ClientFrame::Hello {
            rows: 50,
            cols: 200,
            spawn: Some(SpawnRequest::Shell),
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
            context: None,
        }
    );
}

#[test]
fn hello_with_spawn_agent_and_env_roundtrips() {
    let bytes = encode_client(ClientFrame::Hello {
        rows: 50,
        cols: 200,
        spawn: Some(SpawnRequest::Instance("codex".to_owned())),
        env: vec![
            ("JACKIN_GIT_COAUTHOR_TRAILER".to_owned(), "1".to_owned()),
            ("JACKIN_GIT_DCO".to_owned(), "1".to_owned()),
        ],
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .unwrap();
    // Decode skips the 4-byte length prefix that `encode_client` writes
    // after the tag; reconstruct the payload to feed `decode_client`.
    let payload = bytes[5..].to_vec();
    let frame = decode_client(TAG_HELLO, payload).unwrap();
    assert_eq!(
        frame,
        ClientFrame::Hello {
            rows: 50,
            cols: 200,
            spawn: Some(SpawnRequest::Instance("codex".to_owned())),
            env: vec![
                ("JACKIN_GIT_COAUTHOR_TRAILER".to_owned(), "1".to_owned()),
                ("JACKIN_GIT_DCO".to_owned(), "1".to_owned()),
            ],
            terminal: ClientTerminal::default(),
            focus_session: None,
            context: None,
        }
    );
}

#[test]
fn hello_rejects_removed_provider_spawn_kind() {
    let mut payload = vec![0, 24, 0, 80, 3, 0, 6];
    payload.extend(b"claude");
    let error = decode_client(TAG_HELLO, payload).unwrap_err();
    assert!(error.to_string().contains("unknown hello spawn kind 3"));
}

#[test]
fn hello_rejects_oversized_agent_len() {
    // spawn_kind=agent, agent_len=99 but payload only carries
    // 12 bytes of "only-7-bytes".
    // decode must bail rather than slice past the buffer.
    let mut payload = vec![0, 42, 0, 100, 2, 0, 99];
    payload.extend(b"only-7-bytes");
    decode_client(TAG_HELLO, payload).unwrap_err();
}

#[test]
fn hello_rejects_non_utf8_agent_bytes() {
    let mut payload = vec![0, 42, 0, 100, 2, 0, 3];
    payload.extend(&[0xFF, 0xFE, 0xFD]);
    decode_client(TAG_HELLO, payload).unwrap_err();
}

#[test]
fn hello_rejects_truncated_env_value() {
    let mut payload = vec![0, 42, 0, 100, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 99];
    payload.extend(b"KEY");
    payload.extend(b"short");
    decode_client(TAG_HELLO, payload).unwrap_err();
}

#[test]
fn hello_rejects_truncated_4_byte_payload() {
    let payload = vec![0, 24, 0, 80];
    decode_client(TAG_HELLO, payload).unwrap_err();
}

#[test]
fn hello_shell_with_non_empty_agent_slug_rejected() {
    // spawn_kind=1 (Shell), agent_len=5 ("claude"-ish bytes).
    // Shell + slug is structurally invalid; decode must bail.
    let mut payload = vec![0, 24, 0, 80, 1, 0, 5];
    payload.extend(b"claud");
    payload.extend(&[0, 0]);
    payload.push(0);
    decode_client(TAG_HELLO, payload).unwrap_err();
}

#[test]
fn hello_with_trailing_bytes_rejected() {
    // Extra byte after the focus_kind tail must fail rather than be
    // tolerated — the wire format is closed, future fields go via a
    // versioned schema bump.
    let mut bytes = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .expect("encode_client for a valid Hello must succeed");
    bytes.push(0xFF);
    let payload = bytes[5..].to_vec();
    decode_client(TAG_HELLO, payload).unwrap_err();
}

#[test]
fn welcome_decodes_session_count() {
    let bytes = encode_server(ServerFrame::Welcome { session_count: 7 });
    let payload = bytes[5..].to_vec();
    let frame = decode_server(TAG_WELCOME, payload).unwrap();
    assert_eq!(frame, ServerFrame::Welcome { session_count: 7 });
}

#[test]
fn welcome_rejects_truncated_payload() {
    decode_server(TAG_WELCOME, vec![0, 0]).unwrap_err();
}

#[test]
fn server_frames_roundtrip() {
    for frame in [
        ServerFrame::Output(b"raw bytes".to_vec()),
        ServerFrame::SessionList(br#"[{"id":1}]"#.to_vec()),
        ServerFrame::Shutdown { reason: None },
        ServerFrame::Shutdown {
            reason: Some("agent exited with code 2".to_owned()),
        },
        ServerFrame::Bell,
        ServerFrame::HostOpenUrl("https://github.com/jackin-project/jackin/actions/runs/1".into()),
        ServerFrame::HostOpenUrl("mailto:operator@example.com".into()),
        ServerFrame::HostRevealPath("/Users/operator/Documents/report.txt".into()),
        ServerFrame::HostStageImageFromClipboardPath,
        ServerFrame::HostPasteImageFromClipboard,
        ServerFrame::HostStageImageFromClipboard,
    ] {
        let bytes = encode_server(frame.clone());
        let tag = bytes[0];
        let payload = bytes[5..].to_vec();
        assert_eq!(decode_server(tag, payload).unwrap(), frame);
    }
}

#[test]
fn host_reveal_path_rejects_empty_and_oversized_payloads() {
    assert!(
        decode_server(TAG_HOST_REVEAL_PATH, Vec::new())
            .unwrap_err()
            .to_string()
            .contains("empty")
    );
    assert!(
        decode_server(
            TAG_HOST_REVEAL_PATH,
            vec![b'x'; MAX_HOST_REVEAL_PATH_BYTES + 1],
        )
        .unwrap_err()
        .to_string()
        .contains("exceeds cap")
    );
}

#[test]
fn file_export_server_frames_roundtrip() {
    let digest = [0x5au8; FILE_EXPORT_DIGEST_BYTES];
    for frame in [
        ServerFrame::FileExportStart(FileExportStart {
            transfer_id: 7,
            source_path: "/workspace/report.txt".into(),
            file_name: "report.txt".into(),
            size: 11,
            reveal_after_export: true,
            open_after_export: false,
        }),
        ServerFrame::FileExportChunk(FileExportChunk {
            transfer_id: 7,
            offset: 0,
            bytes: b"hello".to_vec(),
        }),
        ServerFrame::FileExportEnd(FileExportEnd {
            transfer_id: 7,
            sha256: digest,
        }),
    ] {
        let bytes = encode_server(frame.clone());
        let tag = bytes[0];
        let payload = bytes[5..].to_vec();
        assert_eq!(decode_server(tag, payload).unwrap(), frame);
    }
}

#[test]
fn file_export_decode_rejects_malformed_payloads() {
    decode_server(TAG_FILE_EXPORT_START, Vec::new()).unwrap_err();
    decode_server(TAG_FILE_EXPORT_CHUNK, vec![0; 16]).unwrap_err();
    decode_server(TAG_FILE_EXPORT_END, vec![0; 8]).unwrap_err();

    let mut bad_reveal_flag = Vec::new();
    bad_reveal_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_reveal_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_reveal_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_reveal_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_reveal_flag.push(2);
    bad_reveal_flag.extend_from_slice(b"s");
    bad_reveal_flag.extend_from_slice(b"n");
    decode_server(TAG_FILE_EXPORT_START, bad_reveal_flag).unwrap_err();

    let mut bad_open_flag = Vec::new();
    bad_open_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_open_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_open_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_open_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_open_flag.push(0);
    bad_open_flag.push(2);
    bad_open_flag.extend_from_slice(b"s");
    bad_open_flag.extend_from_slice(b"n");
    decode_server(TAG_FILE_EXPORT_START, bad_open_flag).unwrap_err();
}

#[test]
fn clipboard_image_transfer_client_frames_roundtrip() {
    let start = ClientFrame::ClipboardImageStart(ClipboardImageStart {
        transfer_id: 42,
        format: ClipboardImageFormat::Png,
        size: 12,
    });
    let chunk = ClientFrame::ClipboardImageChunk(ClipboardImageChunk {
        transfer_id: 42,
        offset: 0,
        bytes: b"\x89PNG\r\n\x1a\nrest".to_vec(),
    });
    let end = ClientFrame::ClipboardImageEnd(ClipboardImageEnd {
        transfer_id: 42,
        sha256: [7; FILE_EXPORT_DIGEST_BYTES],
    });

    for frame in [start, chunk, end] {
        let bytes = encode_client(frame.clone()).unwrap();
        let decoded = decode_client(bytes[0], bytes[5..].to_vec()).unwrap();
        assert_eq!(decoded, frame);
    }
}

#[test]
fn clipboard_image_error_client_frame_roundtrips() {
    let frame = ClientFrame::ClipboardImageError(ClipboardImageError::from_message(
        "host path is not an image".to_owned(),
    ));
    let bytes = encode_client(frame.clone()).unwrap();
    assert_eq!(bytes[0], TAG_CLIPBOARD_IMAGE_ERROR);

    let decoded = decode_client(bytes[0], bytes[5..].to_vec()).unwrap();
    assert_eq!(decoded, frame);
}

#[test]
fn host_notice_client_frame_roundtrips() {
    let frame = ClientFrame::HostNotice("File exported: ~/Downloads/jackin/report.txt".to_owned());
    let bytes = encode_client(frame.clone()).unwrap();
    assert_eq!(bytes[0], TAG_HOST_NOTICE);

    let decoded = decode_client(bytes[0], bytes[5..].to_vec()).unwrap();
    assert_eq!(decoded, frame);
}

#[test]
fn clipboard_image_transfer_decode_rejects_malformed_payloads() {
    decode_client(TAG_CLIPBOARD_IMAGE_START, Vec::new()).unwrap_err();

    let mut empty_size = Vec::new();
    empty_size.extend_from_slice(&1u64.to_be_bytes());
    empty_size.push(ClipboardImageFormat::Png.tag());
    empty_size.extend_from_slice(&0u64.to_be_bytes());
    decode_client(TAG_CLIPBOARD_IMAGE_START, empty_size).unwrap_err();

    let mut empty_chunk = Vec::new();
    empty_chunk.extend_from_slice(&1u64.to_be_bytes());
    empty_chunk.extend_from_slice(&0u64.to_be_bytes());
    decode_client(TAG_CLIPBOARD_IMAGE_CHUNK, empty_chunk).unwrap_err();

    let mut short_end = Vec::new();
    short_end.extend_from_slice(&1u64.to_be_bytes());
    short_end.extend_from_slice(&[0; 3]);
    decode_client(TAG_CLIPBOARD_IMAGE_END, short_end).unwrap_err();

    decode_client(TAG_CLIPBOARD_IMAGE_ERROR, Vec::new()).unwrap_err();
    decode_client(TAG_HOST_NOTICE, Vec::new()).unwrap_err();
}

#[test]
fn clipboard_image_roundtrips() {
    let image = ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: b"\x89PNG\r\n\x1a\npayload".to_vec(),
    };
    let bytes = encode_client(ClientFrame::ClipboardImage(image.clone())).unwrap();
    assert_eq!(bytes[0], TAG_CLIPBOARD_IMAGE);
    let payload = bytes[5..].to_vec();
    assert_eq!(
        decode_client(TAG_CLIPBOARD_IMAGE, payload).unwrap(),
        ClientFrame::ClipboardImage(image)
    );
}
