// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[must_use]
pub fn owo_rgb(rgb: Rgb) -> owo_colors::Rgb {
    owo_colors::Rgb(rgb.r, rgb.g, rgb.b)
}

pub const PHOSPHOR_GREEN: Rgb = Rgb::new(0, 255, 65);
pub const PHOSPHOR_DIM: Rgb = Rgb::new(0, 140, 30);
pub const PHOSPHOR_DARK: Rgb = Rgb::new(0, 80, 18);
pub const BLACK: Rgb = Rgb::new(0, 0, 0);
pub const WHITE: Rgb = Rgb::new(255, 255, 255);
pub const RAIN_HEAD: Rgb = WHITE;
pub const RAIN_FRESH: Rgb = Rgb::new(180, 255, 180);
pub const RAIN_BODY: Rgb = PHOSPHOR_GREEN;
pub const RAIN_MID: Rgb = Rgb::new(0, 200, 50);
pub const RAIN_DIM: Rgb = PHOSPHOR_DIM;
pub const RAIN_DARK: Rgb = PHOSPHOR_DARK;
pub const BRAND_BLOCK: Rgb = PHOSPHOR_GREEN;
pub const LINK_BLUE: Rgb = Rgb::new(0, 80, 180);
pub const DEBUG_AMBER: Rgb = Rgb::new(204, 92, 0);
pub const STATUS_BLOCKED_RED: Rgb = Rgb::new(255, 60, 60);
pub const MENU_IDLE_BG: Rgb = Rgb::new(18, 70, 130);
pub const MENU_IDLE_HOVER_BG: Rgb = Rgb::new(32, 92, 158);
pub const MENU_AWAITING_BG: Rgb = Rgb::new(96, 180, 255);
pub const MENU_AWAITING_HOVER_BG: Rgb = Rgb::new(132, 202, 255);
pub const CYAN: Rgb = Rgb::new(0, 180, 180);
pub const CYAN_DIM: Rgb = Rgb::new(0, 120, 120);
pub const ACTION_ACCENT: Rgb = Rgb::new(180, 255, 180);
pub const DISCLOSURE_ACCENT: Rgb = Rgb::new(255, 208, 102);
