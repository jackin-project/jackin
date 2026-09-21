// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn codex_over_cap_keeps_raw_label_with_clamped_bar() {
    let window: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"used_percent": 142}))
            .expect("over-cap window decodes");
    assert_eq!(window.used_percent_raw(), Some(142.0));
    assert_eq!(window.used_percent_clamped(), Some(100));
    let mut buckets = Vec::new();
    push_codex_window(
        &mut buckets,
        "Session",
        Some(StatusSlot::Session),
        Some(&window),
        1_781_185_560,
    );
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].used_label.as_deref(), Some("142% used"));
    assert_eq!(buckets[0].remaining_percent, Some(0));

    // In-range labels are unchanged; negatives floor at 0, never "-3%".
    let normal: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"used_percent": 63.7}))
            .expect("fractional window decodes");
    let mut buckets = Vec::new();
    push_codex_window(&mut buckets, "Session", None, Some(&normal), 1_781_185_560);
    assert_eq!(buckets[0].used_label.as_deref(), Some("64% used"));
    let negative: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"used_percent": -3}))
            .expect("negative window decodes");
    let mut buckets = Vec::new();
    push_codex_window(
        &mut buckets,
        "Session",
        None,
        Some(&negative),
        1_781_185_560,
    );
    assert_eq!(buckets[0].used_label.as_deref(), Some("0% used"));
    assert_eq!(buckets[0].remaining_percent, Some(100));
}
