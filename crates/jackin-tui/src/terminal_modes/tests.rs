// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn mouse_capture_uses_sgr_without_urxvt_mode() {
    let mut out = Vec::new();

    enable_mouse_capture(&mut out).unwrap();

    let seq = String::from_utf8(out).unwrap();
    assert!(seq.contains("?1006h"));
    assert!(!seq.contains("?1015h"));
}

#[test]
fn disable_mouse_capture_clears_legacy_urxvt_defensively() {
    let mut out = Vec::new();

    disable_mouse_capture(&mut out).unwrap();

    let seq = String::from_utf8(out).unwrap();
    assert!(seq.contains("?1015l"));
}
