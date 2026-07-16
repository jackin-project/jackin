// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `rain`.
use super::*;

#[test]
fn age_to_color_uses_shared_rain_tokens() {
    assert_eq!(age_to_color(0), Some(RAIN_HEAD));
    assert_eq!(age_to_color(1), Some(RAIN_FRESH));
    assert_eq!(age_to_color(3), Some(RAIN_BODY));
    assert_eq!(age_to_color(6), Some(RAIN_MID));
    assert_eq!(age_to_color(11), Some(RAIN_DIM));
    assert_eq!(age_to_color(17), Some(RAIN_DARK));
    assert_eq!(age_to_color(25), None);
}
