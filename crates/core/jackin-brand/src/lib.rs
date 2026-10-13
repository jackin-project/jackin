//! jackin-brand: renderer-neutral jackin❯ identity and product-domain color tokens.
//!
//! **Architecture Invariant:** T0. This crate has no workspace dependencies
//! and never exposes Ratatui, terminal-protocol, widget, or run-loop types.
//! Entry point: [`Rgb`] — renderer-neutral value adapted by output owners.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Three-byte RGB triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Construct an RGB triple.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Adapt a product token to the `owo-colors` raw-ANSI color type.
#[must_use]
pub fn owo_rgb(rgb: Rgb) -> owo_colors::Rgb {
    owo_colors::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Brand phosphor green.
pub const PHOSPHOR_GREEN: Rgb = Rgb::new(0, 255, 65);
/// Dim brand phosphor.
pub const PHOSPHOR_DIM: Rgb = Rgb::new(0, 140, 30);
/// Dark brand phosphor.
pub const PHOSPHOR_DARK: Rgb = Rgb::new(0, 80, 18);
/// Black used for brand text on the phosphor block.
pub const BLACK: Rgb = Rgb::new(0, 0, 0);
/// White used for launch heads and high-contrast brand text.
pub const WHITE: Rgb = Rgb::new(255, 255, 255);
/// Bright launch-animation head.
pub const RAIN_HEAD: Rgb = WHITE;
/// Fresh launch-animation trail.
pub const RAIN_FRESH: Rgb = Rgb::new(180, 255, 180);
/// Normal launch-animation trail.
pub const RAIN_BODY: Rgb = PHOSPHOR_GREEN;
/// Mid-bright launch-animation trail.
pub const RAIN_MID: Rgb = Rgb::new(0, 200, 50);
/// Dim launch-animation trail.
pub const RAIN_DIM: Rgb = PHOSPHOR_DIM;
/// Dark launch-animation tail.
pub const RAIN_DARK: Rgb = PHOSPHOR_DARK;
/// Brand pill background.
pub const BRAND_BLOCK: Rgb = PHOSPHOR_GREEN;
/// Product link color on light status bars.
pub const LINK_BLUE: Rgb = Rgb::new(0, 80, 180);
/// Debug-mode chrome accent.
pub const DEBUG_AMBER: Rgb = Rgb::new(204, 92, 0);
/// Blocked-agent status accent.
pub const STATUS_BLOCKED_RED: Rgb = Rgb::new(255, 60, 60);
/// Idle menu-button background.
pub const MENU_IDLE_BG: Rgb = Rgb::new(18, 70, 130);
/// Hovered idle menu-button background.
pub const MENU_IDLE_HOVER_BG: Rgb = Rgb::new(32, 92, 158);
/// Awaiting-command menu background.
pub const MENU_AWAITING_BG: Rgb = Rgb::new(96, 180, 255);
/// Hovered awaiting-command menu background.
pub const MENU_AWAITING_HOVER_BG: Rgb = Rgb::new(132, 202, 255);
/// Live-state cyan for product status chips.
pub const CYAN: Rgb = Rgb::new(0, 180, 180);
/// Dim live-state cyan.
pub const CYAN_DIM: Rgb = Rgb::new(0, 120, 120);
/// Permitted-action accent.
pub const ACTION_ACCENT: Rgb = Rgb::new(180, 255, 180);
/// Disclosure-control accent.
pub const DISCLOSURE_ACCENT: Rgb = Rgb::new(255, 208, 102);
