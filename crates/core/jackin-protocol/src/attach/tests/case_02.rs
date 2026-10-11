// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn clipboard_image_rejects_empty_payload() {
    let err = encode_client(ClientFrame::ClipboardImage(ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes: Vec::new(),
    }))
    .expect_err("empty image payload must be rejected");
    assert!(format!("{err:#}").contains("empty"));

    decode_client(TAG_CLIPBOARD_IMAGE, vec![1]).unwrap_err();
}

#[test]
fn clipboard_image_rejects_unknown_format() {
    decode_client(TAG_CLIPBOARD_IMAGE, vec![99, 0x42]).unwrap_err();
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
    decode_server(TAG_HOST_OPEN_URL, b"file:///tmp/report.html".to_vec()).unwrap_err();
    decode_server(TAG_HOST_OPEN_URL, b"javascript:alert(1)".to_vec()).unwrap_err();
}

#[test]
fn unknown_server_tag_rejected() {
    decode_server(0xFE, Vec::new()).unwrap_err();
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
        result.expect_err("expected oversize rejection, got");
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
        context: None,
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
        context: None,
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
        context: None,
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
        context: None,
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
