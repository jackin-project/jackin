/// Binary attach channel: roundtrip contract for every frame variant.
///
/// The hot path forwards raw PTY bytes with five bytes of framing
/// overhead (1-byte tag + 4-byte BE length). These tests pin that
/// shape so a future refactor can't sneak base64 or JSON back into
/// the hot path.
use jackin_container::protocol::attach::{
    ClientFrame, ServerFrame, TAG_HELLO, TAG_OUTPUT, TAG_SHUTDOWN, encode_client, encode_server,
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
    let bytes = encode_server(ServerFrame::Shutdown);
    assert_eq!(bytes, vec![TAG_SHUTDOWN, 0, 0, 0, 0]);
}

#[test]
fn hello_first_byte_never_collides_with_control_channel() {
    // Control channel writes a 4-byte BE length prefix where the high
    // byte is `0x00` for the message sizes the host CLI sends. Attach
    // tags must avoid `0x00` so the daemon can dispatch by first byte.
    let bytes = encode_client(ClientFrame::Hello { rows: 24, cols: 80 });
    assert_ne!(bytes[0], 0x00);
    assert_eq!(bytes[0], TAG_HELLO);
}

#[test]
fn welcome_carries_session_count_be() {
    let bytes = encode_server(ServerFrame::Welcome { session_count: 3 });
    assert_eq!(bytes[0..5], [0x81, 0, 0, 0, 4]);
    assert_eq!(bytes[5..], [0, 0, 0, 3]);
}

#[test]
fn input_frame_carries_raw_bytes_unchanged() {
    let bytes = encode_client(ClientFrame::Input(b"hello\x02world".to_vec()));
    assert_eq!(&bytes[5..], b"hello\x02world");
}
