// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Binary attach channel: roundtrip contract for every frame variant.
///
/// The hot path forwards raw PTY bytes with five bytes of framing
/// overhead (1-byte tag + 4-byte BE length). These tests pin that
/// shape so a future refactor can't sneak base64 or JSON back into
/// the hot path.
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
    // Control channel writes a 4-byte BE length prefix where the high
    // byte is `0x00` for the message sizes the host CLI sends. Attach
    // tags must avoid `0x00` so the daemon can dispatch by first byte.
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
    // Use the named constant so renumbering `TAG_WELCOME` doesn't
    // produce a magic-byte assertion failure with no breadcrumb.
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
    // 3-byte payload is less than the 4 bytes the encoder writes —
    // rejecting it loudly prevents a PTY from being silently resized
    // to whatever the next-frame bytes happen to spell.
    let err = decode_client(TAG_RESIZE, vec![0u8, 24, 0]).expect_err("expected resize bail");
    assert!(
        format!("{err:#}").contains("resize payload too short"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn unknown_client_tag_is_rejected() {
    // First-byte dispatch in `handle_attach_client` is the daemon's
    // trust boundary; unknown tags must bail so a refactor never maps
    // one into a confused ClientFrame variant.
    let err = decode_client(0x7F, vec![1, 2, 3, 4]).expect_err("expected unknown-tag bail");
    assert!(
        format!("{err:#}").contains("unknown client attach tag"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn spawn_request_agent_rejects_empty_slug() {
    SpawnRequest::agent("").expect_err("empty slug must be rejected at construction");
    let ok = SpawnRequest::agent("claude").expect("non-empty slug");
    assert!(matches!(ok, SpawnRequest::Agent(s) if s == "claude"));
}

#[test]
fn hello_env_count_over_cap_is_rejected_by_encoder() {
    // Encoder symmetry: the decoder bails on env_count > MAX_HELLO_ENV;
    // the encoder must too, otherwise a refactor that drops one side
    // would silently produce frames the peer rejects.
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
