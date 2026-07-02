//! Host-terminal default-color capture for attach clients.
//!
//! Before an attach `Hello`, the client can ask the terminal it runs on for
//! its default foreground/background (OSC 10/11). The daemon feeds the answer
//! into every pane grid, which answers agent OSC 10/11 queries from that stored
//! state. Agents gate theming on this answer; for example Codex paints no
//! backgrounds at all when OSC 11 goes silent.

use std::time::Duration;

use tokio::io::AsyncReadExt;

/// Result of the pre-Hello color query. `leftover_input` is the bytes from
/// the query window that were not part of a complete OSC reply — operator
/// keystrokes typed before attach completed — which the caller forwards as
/// ordinary input.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HostColors {
    pub fg: Option<(u8, u8, u8)>,
    pub bg: Option<(u8, u8, u8)>,
    pub leftover_input: Vec<u8>,
}

/// Wall-clock cap for the whole query. Local terminals answer in
/// single-digit milliseconds; the cap bounds terminals that never answer or
/// answer too late.
const QUERY_TIMEOUT: Duration = Duration::from_millis(250);

/// Byte cap on the query-window buffer. Both replies fit in well under a
/// hundred bytes; the cap bounds the per-chunk re-scan and the buffer when
/// stdin floods inside the window.
const QUERY_BUFFER_CAP: usize = 4096;

/// Query the controlling terminal for its default colors. Writes the OSC
/// 10/11 queries to `writer` (the raw-mode terminal), reads `reader` until
/// both replies arrived or the timeout passed. Terminals that cannot answer
/// (`TERM=dumb`/`linux`) are skipped without writing anything.
pub async fn query_host_terminal_colors<R, W>(
    term: Option<&str>,
    reader: &mut R,
    writer: &mut W,
) -> HostColors
where
    R: tokio::io::AsyncRead + Unpin,
    W: std::io::Write,
{
    if matches!(term.unwrap_or(""), "dumb" | "linux") {
        return HostColors::default();
    }

    if writer
        .write_all(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\")
        .and_then(|()| writer.flush())
        .is_err()
    {
        return HostColors::default();
    }

    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    let deadline = tokio::time::Instant::now() + QUERY_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() || buf.len() >= QUERY_BUFFER_CAP {
            break;
        }
        match tokio::time::timeout(remaining, reader.read(&mut chunk)).await {
            Ok(Ok(0) | Err(_)) | Err(_) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&chunk[..n]);
                let parsed = extract_color_replies(&buf);
                if parsed.fg.is_some() && parsed.bg.is_some() {
                    break;
                }
            }
        }
    }

    extract_color_replies(&buf)
}

/// Scan `buf` for OSC 10/11 color replies; return the parsed colors and, as
/// `leftover_input`, the bytes that were not part of a reply in their
/// original order.
fn extract_color_replies(buf: &[u8]) -> HostColors {
    let mut fg = None;
    let mut bg = None;
    let mut leftover = Vec::new();
    let mut rest = buf;
    loop {
        let Some(start) = find(rest, b"\x1b]1") else {
            leftover.extend_from_slice(rest);
            break;
        };
        let candidate = &rest[start..];
        let code = match candidate.get(3) {
            Some(b'0') => 10u8,
            Some(b'1') => 11u8,
            _ => {
                leftover.extend_from_slice(&rest[..=start]);
                rest = &rest[start + 1..];
                continue;
            }
        };
        if candidate.get(4) != Some(&b';') {
            leftover.extend_from_slice(&rest[..=start]);
            rest = &rest[start + 1..];
            continue;
        }
        let payload_start = 5;
        let Some((payload_end, term_len)) = find_terminator(&candidate[payload_start..]) else {
            leftover.extend_from_slice(&rest[..start]);
            break;
        };
        let payload = &candidate[payload_start..payload_start + payload_end];
        let parsed = parse_color_payload(payload);
        match code {
            10 => fg = parsed.or(fg),
            _ => bg = parsed.or(bg),
        }
        leftover.extend_from_slice(&rest[..start]);
        rest = &candidate[payload_start + payload_end + term_len..];
    }
    HostColors {
        fg,
        bg,
        leftover_input: leftover,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_terminator(bytes: &[u8]) -> Option<(usize, usize)> {
    for (i, &b) in bytes.iter().enumerate() {
        if b == 0x07 {
            return Some((i, 1));
        }
        if b == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
            return Some((i, 2));
        }
    }
    None
}

fn parse_color_payload(payload: &[u8]) -> Option<(u8, u8, u8)> {
    let payload = std::str::from_utf8(payload).ok()?;
    if let Some(rgb) = payload.strip_prefix("rgb:") {
        let mut channels = rgb.split('/');
        let r = parse_channel(channels.next()?)?;
        let g = parse_channel(channels.next()?)?;
        let b = parse_channel(channels.next()?)?;
        if channels.next().is_some() {
            return None;
        }
        return Some((r, g, b));
    }
    if let Some(hex) = payload.strip_prefix('#')
        && hex.len() == 6
    {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some((r, g, b));
    }
    None
}

fn parse_channel(channel: &str) -> Option<u8> {
    let digits = channel.len();
    if digits == 0 || digits > 4 {
        return None;
    }
    let value = u32::from_str_radix(channel, 16).ok()?;
    let max = (1u32 << (4 * digits)) - 1;
    u8::try_from(value * 255 / max).ok()
}
