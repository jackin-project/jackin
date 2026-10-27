// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_brand::{
    ACTION_ACCENT as ACTION_ACCENT_RGB, BRAND_BLOCK as BRAND_BLOCK_RGB, CYAN as CYAN_RGB,
    CYAN_DIM as CYAN_DIM_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DISCLOSURE_ACCENT as DISCLOSURE_ACCENT_RGB, LINK_BLUE as LINK_BLUE_RGB,
    MENU_AWAITING_BG as MENU_AWAITING_BG_RGB, MENU_AWAITING_HOVER_BG as MENU_AWAITING_HOVER_BG_RGB,
    MENU_IDLE_BG as MENU_IDLE_BG_RGB, MENU_IDLE_HOVER_BG as MENU_IDLE_HOVER_BG_RGB,
    PHOSPHOR_DARK as PHOSPHOR_DARK_RGB, PHOSPHOR_DIM as PHOSPHOR_DIM_RGB, Rgb,
    STATUS_BLOCKED_RED as STATUS_BLOCKED_RED_RGB, WHITE as WHITE_RGB,
};
use ratatui::style::Color;

#[must_use]
pub const fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

pub const BRAND_BLOCK: Color = color(BRAND_BLOCK_RGB);
pub const DEBUG_AMBER: Color = color(DEBUG_AMBER_RGB);
pub const STATUS_BLOCKED_RED: Color = color(STATUS_BLOCKED_RED_RGB);
pub const MENU_IDLE_BG: Color = color(MENU_IDLE_BG_RGB);
pub const MENU_IDLE_HOVER_BG: Color = color(MENU_IDLE_HOVER_BG_RGB);
pub const MENU_AWAITING_BG: Color = color(MENU_AWAITING_BG_RGB);
pub const MENU_AWAITING_HOVER_BG: Color = color(MENU_AWAITING_HOVER_BG_RGB);
pub const LINK_BLUE: Color = color(LINK_BLUE_RGB);
pub const ACTION_ACCENT: Color = color(ACTION_ACCENT_RGB);
pub const DISCLOSURE_ACCENT: Color = color(DISCLOSURE_ACCENT_RGB);
pub const CYAN: Color = color(CYAN_RGB);
pub const CYAN_DIM: Color = color(CYAN_DIM_RGB);
pub const INK: Color = Color::Black;
pub const BRAND_CHEVRON: Color = color(WHITE_RGB);
pub const BRAND_SEPARATOR: Color = color(PHOSPHOR_DARK_RGB);
pub const BRAND_LABEL: Color = color(PHOSPHOR_DIM_RGB);
