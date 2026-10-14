// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        context: None,
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
fn hello_without_capability_override_tail_is_rejected() {
    let mut encoded = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        focus_session: None,
        context: None,
        terminal: ClientTerminal {
            term: Some("xterm-ghostty".to_owned()),
            default_fg: Some((1, 2, 3)),
            default_bg: Some((4, 5, 6)),
            ..ClientTerminal::default()
        },
    })
    .expect("encode hello");
    let context_wire_len = 2 + serde_json::to_vec(&None::<TelemetryContext>).unwrap().len();
    let end = encoded.len() - context_wire_len;
    encoded.drain(end - 6..end);

    decode_client(TAG_HELLO, encoded[5..].to_vec())
        .expect_err("old Hello without the required tail must be rejected");
}

#[test]
fn hello_env_value_over_cap_rejected_by_encoder() {
    let big = "v".repeat(MAX_HELLO_ENV_VALUE + 1);
    let err = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: vec![("PWD".into(), big)],
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .expect_err("over-cap env value must be rejected at encode");
    let msg = format!("{err:#}");
    assert!(msg.contains("env value"), "got: {msg}");
    assert!(msg.contains(&MAX_HELLO_ENV_VALUE.to_string()), "got: {msg}");
}

#[test]
fn hello_env_value_over_cap_rejected_by_decoder() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&24u16.to_be_bytes());
    payload.extend_from_slice(&80u16.to_be_bytes());
    payload.push(0u8);
    payload.extend_from_slice(&0u16.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&3u16.to_be_bytes());
    let bogus_value_len = u32::try_from(MAX_HELLO_ENV_VALUE + 1).expect("fits u32");
    payload.extend_from_slice(&bogus_value_len.to_be_bytes());
    payload.extend_from_slice(b"PWD");
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
        context: None,
    })
    .expect("hello encode");
    let mut payload = bytes[5..].to_vec();
    let context_wire_len = 2 + serde_json::to_vec(&None::<TelemetryContext>).unwrap().len();
    let fg_presence = payload.len() - 8 - context_wire_len;
    payload[fg_presence] = 2;
    let err = decode_client(TAG_HELLO, payload).expect_err("presence byte 2 must fail");
    assert!(
        err.to_string().contains("default fg presence"),
        "unexpected error: {err:#}"
    );

    let mut payload = bytes[5..].to_vec();
    let bg_presence = payload.len() - 7 - context_wire_len;
    payload[bg_presence] = 7;
    let err = decode_client(TAG_HELLO, payload).expect_err("presence byte 7 must fail");
    assert!(
        err.to_string().contains("default bg presence"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn decode_client_rejects_truncated_payloads_without_panic() {
    for tag in 0u8..=40 {
        drop(decode_client(tag, Vec::new()));
        drop(decode_client(tag, vec![0x00]));
        drop(decode_client(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]));
    }
}

#[test]
fn decode_server_rejects_truncated_payloads_without_panic() {
    for tag in 0u8..=40 {
        drop(decode_server(tag, Vec::new()));
        drop(decode_server(tag, vec![0x00]));
        drop(decode_server(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]));
    }
    for tag in 0x80u8..=0x8f {
        drop(decode_server(tag, Vec::new()));
        drop(decode_server(tag, vec![0x00]));
        drop(decode_server(tag, vec![0xFF, 0xFF, 0xFF, 0xFF]));
    }
}

#[test]
fn decode_rejects_unknown_tags() {
    decode_client(0xFE, Vec::new()).unwrap_err();
    decode_server(0xFE, Vec::new()).unwrap_err();
}

#[test]
fn truncated_valid_frame_fails_closed() {
    let frame = encode_server(ServerFrame::Welcome { session_count: 7 });
    let tag = frame[0];
    assert!(frame.len() > 5, "encoded welcome must carry a body");
    let body = &frame[5..frame.len() - 1];
    decode_server(tag, body.to_vec()).expect_err("truncated welcome body must decode as Err");
}

#[test]
fn resize_rejects_short_payload_without_panic() {
    let err = decode_client(TAG_RESIZE, vec![0x00, 0x01]).expect_err("resize needs 4 bytes");
    assert!(
        err.to_string().contains("resize payload too short"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn welcome_rejects_short_payload_without_panic() {
    let err =
        decode_server(TAG_WELCOME, vec![0x00, 0x01, 0x02]).expect_err("welcome needs 4 bytes");
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
        err.to_string()
            .contains("clipboard image payload too short"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn clipboard_image_error_variants_roundtrip() {
    let variants = [
        ClipboardImageError::Empty,
        ClipboardImageError::TooLarge,
        ClipboardImageError::UnsupportedFormat,
        ClipboardImageError::DigestMismatch,
        ClipboardImageError::ChunkSequence,
        ClipboardImageError::MissingTransfer,
        ClipboardImageError::DuplicateTransfer,
        ClipboardImageError::BackendUnavailable,
        ClipboardImageError::Io,
        ClipboardImageError::Other("host clipboard image probe failed: boom".to_owned()),
    ];
    for original in variants {
        let bytes = encode_client(ClientFrame::ClipboardImageError(original.clone())).unwrap();
        let tag = bytes[0];
        let payload = bytes[5..].to_vec();
        let decoded = decode_client(tag, payload).unwrap();
        match decoded {
            ClientFrame::ClipboardImageError(got) => {
                assert_eq!(got.reason_code(), original.reason_code());
                if matches!(original, ClipboardImageError::Other(_)) {
                    assert_eq!(got, original);
                } else {
                    assert_eq!(got.reason_code(), original.reason_code());
                    assert!(!got.message().is_empty());
                }
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

#[test]
fn clipboard_image_error_from_message_classifies_known_shapes() {
    assert_eq!(
        ClipboardImageError::from_message("clipboard image transfer is empty".into()).reason_code(),
        "empty"
    );
    assert_eq!(
        ClipboardImageError::from_message("transfer 9 exceeds cap 8".into()).reason_code(),
        "oversize"
    );
    assert_eq!(
        ClipboardImageError::from_message("host path is not an image".into()).reason_code(),
        "signature-mismatch"
    );
    assert_eq!(
        ClipboardImageError::from_message("SHA-256 mismatch".into()).reason_code(),
        "digest-mismatch"
    );
    assert_eq!(
        ClipboardImageError::from_message("offset 4 did not match expected 8".into()).reason_code(),
        "offset-mismatch"
    );
}
