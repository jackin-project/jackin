// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_brand::Rgb;
use ratatui::style::Color;

use crate::tokens::{INK, color};

#[test]
fn color_maps_rgb_channels_in_order() {
    assert_eq!(color(Rgb::new(1, 2, 3)), Color::Rgb(1, 2, 3));
}

#[test]
fn ink_is_plain_black() {
    assert_eq!(INK, Color::Black);
}
