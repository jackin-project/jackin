// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn pty_osc8_hyperlink_emits_from_frame_metadata() {
    let mut grid = termpane::DamageGrid::new(24, 80, 100);
    grid.process(b"\x1b]8;;https://example.com/run\x1b\\link text\x1b]8;;\x1b\\");
    let target = grid.hyperlink_target_at_content_row(0, 1);
    assert_eq!(target, Some("https://example.com/run"));
    assert!(crate::session::osc8_uri_is_safe(target.unwrap()));
}

#[test]
fn unsafe_pty_osc8_hyperlink_is_not_emitted_from_frame_metadata() {
    assert!(!crate::session::osc8_uri_is_safe("javascript:alert(1)"));
    assert!(!crate::session::osc8_uri_is_safe(
        "data:text/html,<p>hi</p>"
    ));
    let mut grid = termpane::DamageGrid::new(24, 80, 100);
    grid.process(b"\x1b]8;;javascript:alert(1)\x1b\\click me\x1b]8;;\x1b\\");
    if let Some(uri) = grid.hyperlink_target_at_content_row(0, 1) {
        assert!(
            !crate::session::osc8_uri_is_safe(uri),
            "untrusted metadata must not pass the emission gate"
        );
    }
}
