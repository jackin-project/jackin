//! Tests for `attach`.
use super::*;

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
        }
    );
}

#[test]
fn hello_with_spawn_agent_and_env_roundtrips() {
    let bytes = encode_client(ClientFrame::Hello {
        rows: 50,
        cols: 200,
        spawn: Some(SpawnRequest::Agent("codex".to_owned())),
        env: vec![
            ("JACKIN_GIT_COAUTHOR_TRAILER".to_owned(), "1".to_owned()),
            ("JACKIN_GIT_DCO".to_owned(), "1".to_owned()),
        ],
        terminal: ClientTerminal::default(),
        focus_session: None,
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
            spawn: Some(SpawnRequest::Agent("codex".to_owned())),
            env: vec![
                ("JACKIN_GIT_COAUTHOR_TRAILER".to_owned(), "1".to_owned()),
                ("JACKIN_GIT_DCO".to_owned(), "1".to_owned()),
            ],
            terminal: ClientTerminal::default(),
            focus_session: None,
        }
    );
}

#[test]
fn hello_with_agent_and_provider_roundtrips() {
    // spawn_kind=3 carries both the slug and the provider label.
    // A regression dropping the label bytes from the encoder while
    // the decoder still reads them would only surface at a real
    // console-initiated provider launch — pin the round-trip here.
    let spawn = Some(SpawnRequest::AgentWithProvider {
        slug: "claude".to_owned(),
        provider_label: "Z.AI".to_owned(),
    });
    let bytes = encode_client(ClientFrame::Hello {
        rows: 50,
        cols: 200,
        spawn: spawn.clone(),
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
    })
    .unwrap();
    let payload = bytes[5..].to_vec();
    match decode_client(TAG_HELLO, payload).unwrap() {
        ClientFrame::Hello { spawn: out, .. } => assert_eq!(out, spawn),
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn hello_rejects_oversized_provider_label_at_encode() {
    let err = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: Some(SpawnRequest::AgentWithProvider {
            slug: "claude".to_owned(),
            provider_label: "p".repeat(MAX_HELLO_PROVIDER_LABEL + 1),
        }),
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
    })
    .expect_err("over-cap provider label must be rejected at encode");
    let msg = format!("{err:#}");
    assert!(msg.contains("provider label"), "got: {msg}");
    assert!(
        msg.contains(&MAX_HELLO_PROVIDER_LABEL.to_string()),
        "got: {msg}"
    );
}

#[test]
fn hello_rejects_empty_provider_label_at_decode() {
    // spawn_kind=3, slug="claude", provider_label_len=0. The decoder
    // must reject an AgentWithProvider frame with no label rather than
    // construct one the daemon would route as an unknown provider.
    let mut payload = vec![0, 24, 0, 80, 3, 0, 6];
    payload.extend(b"claude");
    payload.extend_from_slice(&0u16.to_be_bytes()); // provider_label_len = 0
    assert!(decode_client(TAG_HELLO, payload).is_err());
}

#[test]
fn hello_rejects_oversized_agent_len() {
    // spawn_kind=agent, agent_len=99 but payload only carries
    // 12 bytes of "only-7-bytes".
    // decode must bail rather than slice past the buffer.
    let mut payload = vec![0, 42, 0, 100, 2, 0, 99];
    payload.extend(b"only-7-bytes");
    assert!(decode_client(TAG_HELLO, payload).is_err());
}

#[test]
fn hello_rejects_non_utf8_agent_bytes() {
    let mut payload = vec![0, 42, 0, 100, 2, 0, 3];
    payload.extend(&[0xFF, 0xFE, 0xFD]);
    assert!(decode_client(TAG_HELLO, payload).is_err());
}

#[test]
fn hello_rejects_truncated_env_value() {
    let mut payload = vec![0, 42, 0, 100, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 99];
    payload.extend(b"KEY");
    payload.extend(b"short");
    assert!(decode_client(TAG_HELLO, payload).is_err());
}

#[test]
fn hello_rejects_truncated_4_byte_payload() {
    let payload = vec![0, 24, 0, 80];
    assert!(decode_client(TAG_HELLO, payload).is_err());
}

#[test]
fn hello_shell_with_non_empty_agent_slug_rejected() {
    // spawn_kind=1 (Shell), agent_len=5 ("claude"-ish bytes).
    // Shell + slug is structurally invalid; decode must bail.
    let mut payload = vec![0, 24, 0, 80, 1, 0, 5];
    payload.extend(b"claud");
    payload.extend(&[0, 0]);
    payload.push(0);
    assert!(decode_client(TAG_HELLO, payload).is_err());
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
    })
    .expect("encode_client for a valid Hello must succeed");
    bytes.push(0xFF);
    let payload = bytes[5..].to_vec();
    assert!(decode_client(TAG_HELLO, payload).is_err());
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
    assert!(decode_server(TAG_WELCOME, vec![0, 0]).is_err());
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
        ServerFrame::HostRevealPath(
            "/Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl".into(),
        ),
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
    assert!(decode_server(TAG_FILE_EXPORT_START, Vec::new()).is_err());
    assert!(decode_server(TAG_FILE_EXPORT_CHUNK, vec![0; 16]).is_err());
    assert!(decode_server(TAG_FILE_EXPORT_END, vec![0; 8]).is_err());

    let mut bad_reveal_flag = Vec::new();
    bad_reveal_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_reveal_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_reveal_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_reveal_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_reveal_flag.push(2);
    bad_reveal_flag.extend_from_slice(b"s");
    bad_reveal_flag.extend_from_slice(b"n");
    assert!(decode_server(TAG_FILE_EXPORT_START, bad_reveal_flag).is_err());

    let mut bad_open_flag = Vec::new();
    bad_open_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_open_flag.extend_from_slice(&1u64.to_be_bytes());
    bad_open_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_open_flag.extend_from_slice(&1u16.to_be_bytes());
    bad_open_flag.push(0);
    bad_open_flag.push(2);
    bad_open_flag.extend_from_slice(b"s");
    bad_open_flag.extend_from_slice(b"n");
    assert!(decode_server(TAG_FILE_EXPORT_START, bad_open_flag).is_err());
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
    let frame = ClientFrame::ClipboardImageError("host path is not an image".to_owned());
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
    assert!(decode_client(TAG_CLIPBOARD_IMAGE_START, Vec::new()).is_err());

    let mut empty_size = Vec::new();
    empty_size.extend_from_slice(&1u64.to_be_bytes());
    empty_size.push(ClipboardImageFormat::Png.tag());
    empty_size.extend_from_slice(&0u64.to_be_bytes());
    assert!(decode_client(TAG_CLIPBOARD_IMAGE_START, empty_size).is_err());

    let mut empty_chunk = Vec::new();
    empty_chunk.extend_from_slice(&1u64.to_be_bytes());
    empty_chunk.extend_from_slice(&0u64.to_be_bytes());
    assert!(decode_client(TAG_CLIPBOARD_IMAGE_CHUNK, empty_chunk).is_err());

    let mut short_end = Vec::new();
    short_end.extend_from_slice(&1u64.to_be_bytes());
    short_end.extend_from_slice(&[0; 3]);
    assert!(decode_client(TAG_CLIPBOARD_IMAGE_END, short_end).is_err());

    assert!(decode_client(TAG_CLIPBOARD_IMAGE_ERROR, Vec::new()).is_err());
    assert!(decode_client(TAG_HOST_NOTICE, Vec::new()).is_err());
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

#[test]
fn clipboard_image_rejects_empty_payload() {
    let err = encode_client(ClientFrame::ClipboardImage(ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: Vec::new(),
    }))
    .expect_err("empty image payload must be rejected");
    assert!(format!("{err:#}").contains("empty"));

    assert!(decode_client(TAG_CLIPBOARD_IMAGE, vec![1]).is_err());
}

#[test]
fn clipboard_image_rejects_unknown_format() {
    assert!(decode_client(TAG_CLIPBOARD_IMAGE, vec![99, 0x42]).is_err());
}

#[test]
fn clipboard_image_rejects_over_cap_payload_at_encode() {
    let err = encode_client(ClientFrame::ClipboardImage(ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: vec![0x42; MAX_CLIPBOARD_IMAGE_BYTES + 1],
    }))
    .expect_err("over-cap image payload must be rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("clipboard image payload"), "got: {msg}");
    assert!(
        msg.contains(&MAX_CLIPBOARD_IMAGE_BYTES.to_string()),
        "got: {msg}"
    );
}

#[test]
fn clipboard_image_rejects_over_cap_payload_at_decode() {
    let mut payload = Vec::with_capacity(MAX_CLIPBOARD_IMAGE_BYTES + 2);
    payload.push(1);
    payload.extend(std::iter::repeat_n(0x42, MAX_CLIPBOARD_IMAGE_BYTES + 1));
    let err = decode_client(TAG_CLIPBOARD_IMAGE, payload)
        .expect_err("over-cap image payload must be rejected at decode");
    let msg = format!("{err:#}");
    assert!(msg.contains("clipboard image payload"), "got: {msg}");
    assert!(
        msg.contains(&MAX_CLIPBOARD_IMAGE_BYTES.to_string()),
        "got: {msg}"
    );
}

#[test]
fn host_open_url_rejects_disallowed_schemes() {
    assert!(decode_server(TAG_HOST_OPEN_URL, b"file:///tmp/report.html".to_vec()).is_err());
    assert!(decode_server(TAG_HOST_OPEN_URL, b"javascript:alert(1)".to_vec()).is_err());
}

#[test]
fn unknown_server_tag_rejected() {
    assert!(decode_server(0xFE, Vec::new()).is_err());
}

#[test]
fn read_client_frame_rejects_oversize() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let oversize_len = (MAX_FRAME_PAYLOAD + 1) as u32;
        a.write_all(&oversize_len.to_be_bytes()).await.unwrap();
        a.shutdown().await.unwrap();
        let result = read_client_frame(&mut b, TAG_INPUT).await;
        assert!(
            result.is_err(),
            "expected oversize rejection, got {result:?}"
        );
    });
}

#[test]
fn read_client_frame_accepts_exact_max_payload() {
    // Boundary partner for `read_client_frame_rejects_oversize`: a
    // refactor that swaps the inequality from `>` to `>=` in
    // `read_framed_payload` would silently shrink the documented
    // maximum by one byte. This test fails the moment that happens.
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let exact_len = MAX_FRAME_PAYLOAD as u32;
        let write_task = tokio::spawn(async move {
            a.write_all(&exact_len.to_be_bytes()).await.unwrap();
            a.write_all(&vec![0x42u8; MAX_FRAME_PAYLOAD]).await.unwrap();
            a.shutdown().await.unwrap();
        });
        let result = read_client_frame(&mut b, TAG_INPUT).await;
        write_task.await.unwrap();
        let frame = result
            .expect("must not reject exact-max payload")
            .expect("frame present");
        match frame {
            ClientFrame::Input(bytes) => assert_eq!(bytes.len(), MAX_FRAME_PAYLOAD),
            other => panic!("expected Input, got {other:?}"),
        }
    });
}

#[test]
fn read_client_frame_accepts_large_clipboard_image_payload() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let image_len = MAX_FRAME_PAYLOAD + 128;
        let payload_len = 1 + image_len;
        let write_task = tokio::spawn(async move {
            a.write_all(&(payload_len as u32).to_be_bytes())
                .await
                .unwrap();
            a.write_all(&[ClipboardImageFormat::Png.tag()])
                .await
                .unwrap();
            a.write_all(&vec![0x42u8; image_len]).await.unwrap();
            a.shutdown().await.unwrap();
        });
        let result = read_client_frame(&mut b, TAG_CLIPBOARD_IMAGE).await;
        write_task.await.unwrap();
        let frame = result
            .expect("large clipboard image frame must be accepted")
            .expect("frame present");
        match frame {
            ClientFrame::ClipboardImage(image) => {
                assert_eq!(image.format, ClipboardImageFormat::Png);
                assert_eq!(image.bytes.len(), image_len);
            }
            other => panic!("expected ClipboardImage, got {other:?}"),
        }
    });
}

#[test]
fn hello_env_count_over_cap_is_rejected_by_encoder() {
    // Encoder gate must reject `MAX_HELLO_ENV + 1`. Without this the
    // wire could carry an env list a future decoder gladly accepts,
    // bypassing the documented cap.
    let env: Vec<(String, String)> = (0..=MAX_HELLO_ENV)
        .map(|i| (format!("K{i}"), "v".into()))
        .collect();
    let err = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env,
        terminal: ClientTerminal::default(),
        focus_session: None,
    })
    .expect_err("over-cap env must be rejected at encode");
    let msg = format!("{err:#}");
    assert!(msg.contains("env count"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV.to_string()), "got: {msg}");
}

#[test]
fn hello_env_count_over_cap_is_rejected_by_decoder() {
    // Decoder must refuse a hand-crafted payload claiming
    // `env_count = MAX_HELLO_ENV + 1`. This is the wire-level
    // counterpart of the encoder guard: a buggy or hostile peer
    // could otherwise force the daemon to pre-allocate an
    // arbitrarily large env table.
    let mut payload = Vec::new();
    payload.extend_from_slice(&24u16.to_be_bytes()); // rows
    payload.extend_from_slice(&80u16.to_be_bytes()); // cols
    payload.push(0u8); // spawn_kind = None
    payload.extend_from_slice(&0u16.to_be_bytes()); // agent_len = 0
    let bogus_count = u16::try_from(MAX_HELLO_ENV + 1).expect("fits u16");
    payload.extend_from_slice(&bogus_count.to_be_bytes());
    let err = decode_client(TAG_HELLO, payload)
        .expect_err("over-cap env_count must be rejected at decode");
    let msg = format!("{err:#}");
    assert!(msg.contains("env_count"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV.to_string()), "got: {msg}");
}

#[test]
fn hello_env_count_over_cap_is_rejected_by_decoder_with_full_payload() {
    // Partner for `hello_env_count_over_cap_is_rejected_by_decoder`:
    // that test crafts ONLY the env_count and stops, so the
    // front-of-loop guard fires before the per-entry read runs. A
    // refactor that moved the cap check below the per-entry loop
    // (computing it from accumulated reads) would still pass that
    // test. This variant supplies a fully-populated payload of
    // `MAX_HELLO_ENV + 1` real entries so the boundary is verified
    // after the per-entry read, not just at the count declaration.
    let mut payload = Vec::new();
    payload.extend_from_slice(&24u16.to_be_bytes()); // rows
    payload.extend_from_slice(&80u16.to_be_bytes()); // cols
    payload.push(0u8); // spawn_kind = None
    payload.extend_from_slice(&0u16.to_be_bytes()); // agent_len = 0
    let bogus_count = u16::try_from(MAX_HELLO_ENV + 1).expect("fits u16");
    payload.extend_from_slice(&bogus_count.to_be_bytes());
    for i in 0..=MAX_HELLO_ENV {
        let key = format!("K{i}");
        let value = "v";
        payload.extend_from_slice(&(key.len() as u16).to_be_bytes());
        payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
        payload.extend_from_slice(key.as_bytes());
        payload.extend_from_slice(value.as_bytes());
    }
    payload.push(0u8); // focus_kind = None
    let err = decode_client(TAG_HELLO, payload)
        .expect_err("fully-populated over-cap env_count must be rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("env_count"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV.to_string()), "got: {msg}");
}

#[test]
fn hello_env_count_at_cap_round_trips() {
    // Partner for `hello_env_count_over_cap_is_rejected_by_encoder`:
    // a refactor that swaps `>` to `>=` in the encoder OR decoder
    // would silently shrink the documented cap. Both sides must
    // accept exactly `MAX_HELLO_ENV` entries.
    let env: Vec<(String, String)> = (0..MAX_HELLO_ENV)
        .map(|i| (format!("K{i}"), "v".into()))
        .collect();
    let bytes = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: env.clone(),
        terminal: ClientTerminal::default(),
        focus_session: None,
    })
    .expect("at-cap env must encode");
    let payload = bytes[5..].to_vec();
    let decoded = decode_client(TAG_HELLO, payload).expect("at-cap env must decode");
    match decoded {
        ClientFrame::Hello { env: out, .. } => assert_eq!(out, env),
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn hello_with_focus_session_round_trips() {
    // The console preview-pane click path sets
    // `focus_session: Some(<session_id>)`. A refactor that drops
    // the trailing 8 bytes of session id from the encoder while
    // the decoder still expects them would only fail at a real
    // attach. Exercise the round-trip explicitly so the contract
    // is pinned in the test suite.
    let target = 0xDEAD_BEEF_CAFE_BABEu64;
    let bytes = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: Some(target),
    })
    .expect("focus_session encode");
    let payload = bytes[5..].to_vec();
    let decoded = decode_client(TAG_HELLO, payload).expect("focus_session decode");
    match decoded {
        ClientFrame::Hello { focus_session, .. } => {
            assert_eq!(focus_session, Some(target));
        }
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn hello_with_client_terminal_round_trips() {
    let terminal = ClientTerminal {
        term: Some("xterm-ghostty".to_owned()),
        term_program: Some("ghostty".to_owned()),
        colorterm: Some("truecolor".to_owned()),
        default_fg: Some((0xe6, 0xe6, 0xe6)),
        default_bg: Some((0x17, 0x17, 0x17)),
        ..ClientTerminal::default()
    };
    let bytes = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        terminal: terminal.clone(),
        focus_session: None,
    })
    .expect("terminal identity encode");
    let payload = bytes[5..].to_vec();
    let decoded = decode_client(TAG_HELLO, payload).expect("terminal identity decode");
    match decoded {
        ClientFrame::Hello { terminal: out, .. } => assert_eq!(out, terminal),
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn client_terminal_detects_known_pointer_shape_support() {
    let ghostty = ClientTerminal {
        term: Some("xterm-ghostty".to_owned()),
        ..ClientTerminal::default()
    };
    let kitty = ClientTerminal {
        term: Some("xterm-kitty".to_owned()),
        ..ClientTerminal::default()
    };
    let iterm = ClientTerminal {
        term_program: Some("iTerm.app".to_owned()),
        ..ClientTerminal::default()
    };
    let warp = ClientTerminal {
        term_program: Some("WarpTerminal".to_owned()),
        ..ClientTerminal::default()
    };
    let apple_terminal = ClientTerminal {
        term: Some("xterm-256color".to_owned()),
        term_program: Some("Apple_Terminal".to_owned()),
        ..ClientTerminal::default()
    };
    let generic_xterm = ClientTerminal {
        term: Some("xterm-256color".to_owned()),
        ..ClientTerminal::default()
    };
    let dumb = ClientTerminal {
        term: Some("dumb".to_owned()),
        ..ClientTerminal::default()
    };

    assert!(ghostty.pointer_shapes_supported());
    assert!(kitty.pointer_shapes_supported());
    assert!(iterm.pointer_shapes_supported());
    assert!(apple_terminal.pointer_shapes_supported());
    assert!(!generic_xterm.pointer_shapes_supported());
    assert!(!warp.pointer_shapes_supported());
    assert!(!dumb.pointer_shapes_supported());
}

#[test]
fn client_terminal_derives_attach_capabilities() {
    let kitty = ClientTerminal {
        term: Some("xterm-kitty".to_owned()),
        colorterm: Some("truecolor".to_owned()),
        ..ClientTerminal::default()
    };
    let caps = kitty.attach_capabilities();
    assert!(caps.pointer_shapes);
    assert!(caps.truecolor);
    assert!(caps.synchronized_output);
    assert!(caps.osc8_hyperlinks);
    assert!(caps.underline_style);
    assert_eq!(caps.image_protocol, ImageProtocolCapability::Kitty);

    let dumb = ClientTerminal {
        term: Some("dumb".to_owned()),
        ..ClientTerminal::default()
    };
    let caps = dumb.attach_capabilities();
    assert!(!caps.pointer_shapes);
    assert!(!caps.synchronized_output);
    assert!(!caps.osc8_hyperlinks);
    assert_eq!(caps.image_protocol, ImageProtocolCapability::Unsupported);
}

#[test]
fn client_terminal_records_capability_sources_and_overrides() {
    let terminal = ClientTerminal {
        term: Some("xterm-256color".to_owned()),
        colorterm: Some("truecolor".to_owned()),
        default_fg: Some((1, 2, 3)),
        capability_overrides: AttachCapabilityOverrides {
            osc8_hyperlinks: Some(false),
            image_protocol: Some(ImageProtocolCapability::Kitty),
            ..AttachCapabilityOverrides::default()
        },
        ..ClientTerminal::default()
    };

    let caps = terminal.attach_capabilities();

    assert!(caps.sources.handshake_identity);
    assert!(caps.sources.terminfo_name);
    assert!(caps.sources.safe_color_probe);
    assert!(caps.sources.user_override);
    assert!(!caps.sources.denylist);
    assert!(caps.truecolor);
    assert!(!caps.osc8_hyperlinks);
    assert_eq!(caps.image_protocol, ImageProtocolCapability::Kitty);
}

#[test]
fn hello_round_trips_capability_overrides() {
    let frame = ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        focus_session: None,
        terminal: ClientTerminal {
            term: Some("xterm-kitty".to_owned()),
            capability_overrides: AttachCapabilityOverrides {
                pointer_shapes: Some(false),
                synchronized_output: Some(true),
                underline_style: Some(false),
                image_protocol: Some(ImageProtocolCapability::Unsupported),
                ..AttachCapabilityOverrides::default()
            },
            ..ClientTerminal::default()
        },
    };

    let encoded = encode_client(frame.clone()).expect("encode hello");
    let decoded = decode_client(encoded[0], encoded[5..].to_vec()).expect("decode hello");

    assert_eq!(decoded, frame);
}

#[test]
fn hello_without_capability_override_tail_decodes_as_default() {
    let mut encoded = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        focus_session: None,
        terminal: ClientTerminal {
            term: Some("xterm-ghostty".to_owned()),
            default_fg: Some((1, 2, 3)),
            default_bg: Some((4, 5, 6)),
            ..ClientTerminal::default()
        },
    })
    .expect("encode hello");
    encoded.truncate(encoded.len() - 6);

    let decoded = decode_client(TAG_HELLO, encoded[5..].to_vec()).expect("decode old hello");

    match decoded {
        ClientFrame::Hello { terminal, .. } => {
            assert_eq!(
                terminal.capability_overrides,
                AttachCapabilityOverrides::default()
            );
        }
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn hello_env_value_over_cap_rejected_by_encoder() {
    // Encoder gate must reject a single env value larger than
    // MAX_HELLO_ENV_VALUE so a buggy producer cannot smuggle a
    // megabyte-sized env entry past MAX_HELLO_ENV.
    let big = "v".repeat(MAX_HELLO_ENV_VALUE + 1);
    let err = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: vec![("PWD".into(), big)],
        terminal: ClientTerminal::default(),
        focus_session: None,
    })
    .expect_err("over-cap env value must be rejected at encode");
    let msg = format!("{err:#}");
    assert!(msg.contains("env value"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV_VALUE.to_string()), "got: {msg}");
}

#[test]
fn hello_env_value_over_cap_rejected_by_decoder() {
    // Wire-level counterpart: a hand-crafted payload claiming
    // value_len > MAX_HELLO_ENV_VALUE must be rejected before
    // any read_string allocates the actual bytes.
    let mut payload = Vec::new();
    payload.extend_from_slice(&24u16.to_be_bytes()); // rows
    payload.extend_from_slice(&80u16.to_be_bytes()); // cols
    payload.push(0u8); // spawn_kind = None
    payload.extend_from_slice(&0u16.to_be_bytes()); // agent_len = 0
    payload.extend_from_slice(&1u16.to_be_bytes()); // env_count = 1
    payload.extend_from_slice(&3u16.to_be_bytes()); // key_len = 3
    let bogus_value_len = u32::try_from(MAX_HELLO_ENV_VALUE + 1).expect("fits u32");
    payload.extend_from_slice(&bogus_value_len.to_be_bytes());
    payload.extend_from_slice(b"PWD");
    // No need to supply the value bytes; the cap check fires before
    // read_string reaches into the buffer.
    let err = decode_client(TAG_HELLO, payload)
        .expect_err("over-cap env value length must be rejected at decode");
    let msg = format!("{err:#}");
    assert!(msg.contains("env value"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV_VALUE.to_string()), "got: {msg}");
}

#[test]
fn hello_env_key_over_cap_rejected_by_decoder() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&24u16.to_be_bytes());
    payload.extend_from_slice(&80u16.to_be_bytes());
    payload.push(0u8);
    payload.extend_from_slice(&0u16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    let bogus_key_len = u16::try_from(MAX_HELLO_ENV_KEY + 1).expect("fits u16");
    payload.extend_from_slice(&bogus_key_len.to_be_bytes());
    payload.extend_from_slice(&1u32.to_be_bytes());
    let err = decode_client(TAG_HELLO, payload)
        .expect_err("over-cap env key length must be rejected at decode");
    let msg = format!("{err:#}");
    assert!(msg.contains("env key"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV_KEY.to_string()), "got: {msg}");
}

#[test]
fn read_client_frame_eof_after_tag_returns_none() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        // Tag is treated as already-peeked; write nothing else, then
        // close. The reader should hit EOF inside the length read
        // and return Ok(None), not Err.
        a.shutdown().await.unwrap();
        drop(a);
        let result = read_client_frame(&mut b, TAG_INPUT).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn hello_rejects_unknown_color_presence_byte() {
    let bytes = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
    })
    .expect("hello encode");
    // Both colors are None and precede the six capability override bytes.
    // Corrupt the fg presence byte to an undefined discriminant.
    let mut payload = bytes[5..].to_vec();
    let fg_presence = payload.len() - 8;
    payload[fg_presence] = 2;
    let err = decode_client(TAG_HELLO, payload).expect_err("presence byte 2 must fail");
    assert!(
        err.to_string().contains("default fg presence"),
        "unexpected error: {err:#}"
    );

    // Same body, bg label: bg is immediately before override bytes.
    let mut payload = bytes[5..].to_vec();
    let bg_presence = payload.len() - 7;
    payload[bg_presence] = 7;
    let err = decode_client(TAG_HELLO, payload).expect_err("presence byte 7 must fail");
    assert!(
        err.to_string().contains("default bg presence"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn decode_client_rejects_truncated_payloads_without_panic() {
    // For every known client tag, a deliberately-too-short payload must not panic.
    for tag in 0u8..=40 {
        // 0-byte and 1-byte payloads exercise the length-prefix / field readers.
        drop(decode_client(tag, Vec::new()));
        drop(decode_client(tag, vec![0x00]));
        drop(decode_client(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]));
    }
    // The point is no panic; reaching here is the assertion.
}

#[test]
fn decode_server_rejects_truncated_payloads_without_panic() {
    for tag in 0u8..=40 {
        drop(decode_server(tag, Vec::new()));
        drop(decode_server(tag, vec![0x00]));
        drop(decode_server(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]));
    }
    // Server tags live in the 0x80+ range; cover those too.
    for tag in 0x80u8..=0x8f {
        drop(decode_server(tag, Vec::new()));
        drop(decode_server(tag, vec![0x00]));
        drop(decode_server(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]));
    }
}

#[test]
fn decode_rejects_unknown_tags() {
    // 0xFE is not a defined client or server frame tag.
    assert!(decode_client(0xFE, Vec::new()).is_err());
    assert!(decode_server(0xFE, Vec::new()).is_err());
}

#[test]
fn truncated_valid_frame_fails_closed() {
    // Welcome requires a 4-byte body; lopping a byte off a valid encoding
    // must fail closed (Err), never panic. TAG_OUTPUT would tolerate a
    // shorter body by design, so Welcome is the load-bearing case.
    let frame = encode_server(ServerFrame::Welcome { session_count: 7 });
    // frame = [tag, len(4 bytes BE), body…]
    let tag = frame[0];
    assert!(frame.len() > 5, "encoded welcome must carry a body");
    let body = &frame[5..frame.len() - 1];
    assert!(
        decode_server(tag, body.to_vec()).is_err(),
        "truncated welcome body must decode as Err"
    );
}

#[test]
fn resize_rejects_short_payload_without_panic() {
    let err = decode_client(TAG_RESIZE, vec![0x00, 0x01])
        .expect_err("resize needs 4 bytes");
    assert!(
        err.to_string().contains("resize payload too short"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn welcome_rejects_short_payload_without_panic() {
    let err = decode_server(TAG_WELCOME, vec![0x00, 0x01, 0x02])
        .expect_err("welcome needs 4 bytes");
    assert!(
        err.to_string().contains("welcome payload too short"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn clipboard_image_rejects_empty_payload_without_panic() {
    let err = decode_client(TAG_CLIPBOARD_IMAGE, Vec::new())
        .expect_err("clipboard image needs format byte");
    assert!(
        err.to_string().contains("clipboard image payload too short"),
        "unexpected error: {err:#}"
    );
}

