// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn adapters_preserve_product_rgb_tokens() {
    assert_eq!(BRAND_BLOCK, color(jackin_brand::BRAND_BLOCK));
    assert_eq!(DEBUG_AMBER, color(jackin_brand::DEBUG_AMBER));
    assert_eq!(STATUS_BLOCKED_RED, color(jackin_brand::STATUS_BLOCKED_RED));
    assert_eq!(ACTION_ACCENT, color(jackin_brand::ACTION_ACCENT));
}
