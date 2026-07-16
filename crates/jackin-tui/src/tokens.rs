// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Ratatui adapters for jackin❯-owned brand and domain color tokens.
//!
//! Neutral presentation is resolved directly through TermRock semantic roles
//! by each consumer. This module deliberately contains no theme facade.

use jackin_brand::{
    ACTION_ACCENT as ACTION_ACCENT_RGB, BRAND_BLOCK as BRAND_BLOCK_RGB, CYAN as CYAN_RGB,
    CYAN_DIM as CYAN_DIM_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DISCLOSURE_ACCENT as DISCLOSURE_ACCENT_RGB, LINK_BLUE as LINK_BLUE_RGB,
    MENU_AWAITING_BG as MENU_AWAITING_BG_RGB, MENU_AWAITING_HOVER_BG as MENU_AWAITING_HOVER_BG_RGB,
    MENU_IDLE_BG as MENU_IDLE_BG_RGB, MENU_IDLE_HOVER_BG as MENU_IDLE_HOVER_BG_RGB, Rgb,
    STATUS_BLOCKED_RED as STATUS_BLOCKED_RED_RGB,
};
use ratatui::style::Color;

/// Convert a product-owned RGB token into a Ratatui color.
#[must_use]
pub const fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Brand pill background.
pub const BRAND_BLOCK: Color = color(BRAND_BLOCK_RGB);
/// Debug-mode chrome accent.
pub const DEBUG_AMBER: Color = color(DEBUG_AMBER_RGB);
/// Blocked-agent status accent.
pub const STATUS_BLOCKED_RED: Color = color(STATUS_BLOCKED_RED_RGB);
/// Idle menu-button background.
pub const MENU_IDLE_BG: Color = color(MENU_IDLE_BG_RGB);
/// Hovered idle menu-button background.
pub const MENU_IDLE_HOVER_BG: Color = color(MENU_IDLE_HOVER_BG_RGB);
/// Awaiting-command menu background.
pub const MENU_AWAITING_BG: Color = color(MENU_AWAITING_BG_RGB);
/// Hovered awaiting-command menu background.
pub const MENU_AWAITING_HOVER_BG: Color = color(MENU_AWAITING_HOVER_BG_RGB);
/// Link color on light status bars.
pub const LINK_BLUE: Color = color(LINK_BLUE_RGB);
/// Permitted-action accent.
pub const ACTION_ACCENT: Color = color(ACTION_ACCENT_RGB);
/// Disclosure-control accent.
pub const DISCLOSURE_ACCENT: Color = color(DISCLOSURE_ACCENT_RGB);
/// Live-state cyan for domain status chips.
pub const CYAN: Color = color(CYAN_RGB);
/// Dim live-state cyan.
pub const CYAN_DIM: Color = color(CYAN_DIM_RGB);
/// Text on bright product chips.
pub const INK: Color = Color::Black;

#[cfg(test)]
mod tests;
