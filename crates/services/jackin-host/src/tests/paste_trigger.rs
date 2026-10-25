// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::host_clipboard::is_image_paste_trigger;

#[test]
fn single_ctrl_v_triggers_image_paste() {
    assert!(is_image_paste_trigger(&[0x16]));
}

#[test]
fn other_inputs_do_not_trigger_image_paste() {
    assert!(!is_image_paste_trigger(b""));
    assert!(!is_image_paste_trigger(&[0x16, 0x16]));
    assert!(!is_image_paste_trigger(b"v"));
    assert!(!is_image_paste_trigger(&[0x03]));
}
