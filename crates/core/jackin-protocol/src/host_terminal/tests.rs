// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{HostColors, extract_color_replies, query_host_terminal_colors};

#[test]
fn extracts_both_color_forms_and_preserves_operator_input() {
    let parsed = extract_color_replies(b"x\x1b]10;rgb:ffff/8000/0000\x1b\\y\x1b]11;#102030\x07z");

    assert_eq!(
        parsed,
        HostColors {
            fg: Some((255, 127, 0)),
            bg: Some((16, 32, 48)),
            leftover_input: b"xyz".to_vec(),
        }
    );
}

#[test]
fn incomplete_reply_is_not_consumed_as_terminal_protocol() {
    let bytes = b"typed\x1b]10;rgb:ffff/0000";
    let parsed = extract_color_replies(bytes);

    assert_eq!(parsed.fg, None);
    assert_eq!(parsed.bg, None);
    assert_eq!(parsed.leftover_input, bytes);
}

#[test]
fn unsupported_terminal_skips_query_without_consuming_input() {
    let mut reader = std::io::Cursor::new(b"typed".to_vec());
    let mut writer = Vec::new();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("current-thread test runtime");
    let parsed = runtime.block_on(query_host_terminal_colors(
        Some("dumb"),
        &mut reader,
        &mut writer,
    ));

    assert_eq!(parsed, HostColors::default());
    assert!(writer.is_empty());
    assert_eq!(reader.position(), 0);
}
