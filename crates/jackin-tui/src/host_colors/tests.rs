use super::*;

#[test]
fn parses_xterm_four_digit_replies() {
    let buf = b"\x1b]10;rgb:e6e6/e6e6/e6e6\x1b\\\x1b]11;rgb:1717/1717/1717\x07";
    let parsed = extract_color_replies(buf);
    assert_eq!(parsed.fg, Some((0xe6, 0xe6, 0xe6)));
    assert_eq!(parsed.bg, Some((0x17, 0x17, 0x17)));
    assert!(
        parsed.leftover_input.is_empty(),
        "leftover: {:?}",
        parsed.leftover_input
    );
}

#[test]
fn parses_short_channels_and_hash_form() {
    let parsed = extract_color_replies(b"\x1b]11;rgb:f/0/8\x07");
    assert_eq!(parsed.bg, Some((0xff, 0x00, 0x88)));
    let parsed = extract_color_replies(b"\x1b]11;#336699\x07");
    assert_eq!(parsed.bg, Some((0x33, 0x66, 0x99)));
}

#[test]
fn keystrokes_around_replies_survive_in_order() {
    let parsed = extract_color_replies(b"ab\x1b]11;rgb:0000/0000/0000\x07cd");
    assert_eq!(parsed.bg, Some((0, 0, 0)));
    assert_eq!(parsed.leftover_input, b"abcd");
}

#[test]
fn partial_reply_tail_is_withheld_from_leftover() {
    let parsed = extract_color_replies(b"x\x1b]11;rgb:12");
    assert_eq!(parsed.bg, None);
    assert_eq!(parsed.leftover_input, b"x");
}

#[test]
fn unrelated_osc_one_passes_through() {
    let buf = b"\x1b]1;icon\x07";
    let parsed = extract_color_replies(buf);
    assert_eq!((parsed.fg, parsed.bg), (None, None));
    assert_eq!(parsed.leftover_input, buf.as_slice());
}

#[test]
fn malformed_payload_yields_none() {
    let parsed = extract_color_replies(b"\x1b]11;rgb:zz/00/00\x07");
    assert_eq!(parsed.bg, None);
    let parsed = extract_color_replies(b"\x1b]11;rgb:0/0\x07");
    assert_eq!(parsed.bg, None);
    let parsed = extract_color_replies(b"\x1b]11;rgb:0/0/0/0\x07");
    assert_eq!(parsed.bg, None);
}

struct SilentReader;

impl tokio::io::AsyncRead for SilentReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

#[tokio::test(start_paused = true)]
async fn silent_terminal_times_out_to_defaults() {
    let mut writer = Vec::new();
    let parsed =
        query_host_terminal_colors(Some("xterm-256color"), &mut SilentReader, &mut writer).await;
    assert_eq!(parsed, HostColors::default());
    assert_eq!(
        writer, b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\",
        "both queries must have been written before the timeout"
    );
}

#[tokio::test(start_paused = true)]
async fn dumb_terminal_skips_the_query_entirely() {
    let mut writer = Vec::new();
    let parsed = query_host_terminal_colors(Some("dumb"), &mut SilentReader, &mut writer).await;
    assert_eq!(parsed, HostColors::default());
    assert!(writer.is_empty(), "no bytes may reach a dumb terminal");
}

#[tokio::test(start_paused = true)]
async fn replies_with_typed_bytes_resolve_colors_and_keep_input() {
    let mut reader = std::io::Cursor::new(
        b"hi\x1b]10;rgb:e6e6/e6e6/e6e6\x07\x1b]11;rgb:0000/0000/0000\x07".to_vec(),
    );
    let mut writer = Vec::new();
    let parsed = query_host_terminal_colors(Some("xterm-ghostty"), &mut reader, &mut writer).await;
    assert_eq!(parsed.fg, Some((0xe6, 0xe6, 0xe6)));
    assert_eq!(parsed.bg, Some((0, 0, 0)));
    assert_eq!(parsed.leftover_input, b"hi");
}

#[tokio::test(start_paused = true)]
async fn both_replies_short_circuit_before_the_timeout() {
    use tokio::io::AsyncReadExt as _;

    let replies = std::io::Cursor::new(
        b"\x1b]10;rgb:ffff/ffff/ffff\x07\x1b]11;rgb:0000/0000/0000\x07".to_vec(),
    );
    let mut reader = replies.chain(SilentReader);
    let mut writer = Vec::new();
    let start = tokio::time::Instant::now();
    let parsed = query_host_terminal_colors(Some("xterm-ghostty"), &mut reader, &mut writer).await;
    assert!(parsed.fg.is_some() && parsed.bg.is_some());
    assert!(
        start.elapsed() < QUERY_TIMEOUT,
        "complete replies must end the query without waiting out the deadline"
    );
}
