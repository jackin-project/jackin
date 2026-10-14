// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_capsule::protocol::attach::{
    ClientFrame, ClientTerminal, MAX_HELLO_ENV, ServerFrame, SpawnRequest, TAG_HELLO, TAG_OUTPUT,
    TAG_RESIZE, TAG_SHUTDOWN, TAG_WELCOME, decode_client, encode_client, encode_server,
};

#[test]
fn output_frame_has_five_byte_overhead_only() {
    let payload = vec![0xCDu8; 8192];
    let bytes = encode_server(ServerFrame::Output(payload.clone()));
    assert_eq!(bytes.len(), 5 + payload.len());
    assert_eq!(bytes[0], TAG_OUTPUT);
    assert_eq!(
        u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize,
        payload.len()
    );
    assert_eq!(&bytes[5..], &payload[..]);
}

#[test]
fn shutdown_frame_is_empty_payload() {
    let bytes = encode_server(ServerFrame::Shutdown { reason: None });
    assert_eq!(bytes, vec![TAG_SHUTDOWN, 0, 0, 0, 0]);
}

#[test]
fn shutdown_frame_can_carry_reason() {
    let bytes = encode_server(ServerFrame::Shutdown {
        reason: Some("session process exited with code 1".to_owned()),
    });
    assert_eq!(bytes[0], TAG_SHUTDOWN);
    assert_eq!(
        &bytes[5..],
        b"session process exited with code 1".as_slice()
    );
}

#[test]
fn hello_first_byte_never_collides_with_control_channel() {
    let bytes = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .expect("encode Hello");
    assert_ne!(bytes[0], 0x00);
    assert_eq!(bytes[0], TAG_HELLO);
}

#[test]
fn welcome_carries_session_count_be() {
    let bytes = encode_server(ServerFrame::Welcome { session_count: 3 });

    assert_eq!(bytes[0], TAG_WELCOME);
    assert_eq!(bytes[1..5], [0, 0, 0, 4]);
    assert_eq!(bytes[5..], [0, 0, 0, 3]);
}

#[test]
fn input_frame_carries_raw_bytes_unchanged() {
    let bytes =
        encode_client(ClientFrame::Input(b"hello\x02world".to_vec())).expect("encode Input");
    assert_eq!(&bytes[5..], b"hello\x02world");
}

#[test]
fn resize_frame_roundtrips() {
    let bytes = encode_client(ClientFrame::Resize {
        rows: 50,
        cols: 200,
    })
    .expect("encode Resize");
    assert_eq!(bytes[0], TAG_RESIZE);
    let payload = bytes[5..].to_vec();
    let decoded = decode_client(TAG_RESIZE, payload).expect("decode Resize");
    assert!(matches!(
        decoded,
        ClientFrame::Resize {
            rows: 50,
            cols: 200
        }
    ));
}

#[test]
fn resize_frame_truncated_rejected() {
    let err = decode_client(TAG_RESIZE, vec![0u8, 24, 0]).expect_err("expected resize bail");
    assert!(
        format!("{err:#}").contains("resize payload too short"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn unknown_client_tag_is_rejected() {
    let err = decode_client(0x7F, vec![1, 2, 3, 4]).expect_err("expected unknown-tag bail");
    assert!(
        format!("{err:#}").contains("unknown client attach tag"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn spawn_request_instance_rejects_empty_target() {
    SpawnRequest::instance("").expect_err("empty target must be rejected at construction");
    let ok = SpawnRequest::instance("claude-work").expect("non-empty target");
    assert!(matches!(ok, SpawnRequest::Instance(s) if s == "claude-work"));
}

#[test]
fn hello_env_count_over_cap_is_rejected_by_encoder() {
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
    .expect_err("over-cap env must bail");
    assert!(
        format!("{err:#}").contains("exceeds wire cap"),
        "unexpected error: {err:#}"
    );
}
