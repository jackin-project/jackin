// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cross-crate RGB type and **product-owned** palette tokens.
//!
//! Shared neutral presentation (ordinary/strong/muted text, borders, tabs,
//! inputs, links on dark surfaces, danger/warning, scroll tracks) lives in
//! `TermRock`'s `Theme` / `Role` API. This module keeps only:
//!
//! - brand chrome and launch rain animation colors;
//! - domain agent/menu/status accents;
//! - non-TUI brand greens used by `owo_colors` CLI/spinner paths.
//!
//! Architecture Invariant: depends only on `std` and the `owo-colors`
//! crate. No `jackin-*` deps.

/// Three-byte RGB triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
}

impl Rgb {
    /// Construct an RGB triple from channel values.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Adapt an [`Rgb`] token to the `owo_colors` raw-ANSI colour type used by the
/// stderr output, spinner, and animation helpers across every surface crate.
#[must_use]
pub fn owo_rgb(rgb: Rgb) -> owo_colors::Rgb {
    owo_colors::Rgb(rgb.r, rgb.g, rgb.b)
}

// --- Brand phosphor (non-TUI brand paint + rain trail anchors) ---

/// Brand phosphor green — brand pill, rain body, CLI brand paint.
pub const PHOSPHOR_GREEN: Rgb = Rgb::new(0, 255, 65);

/// Dim brand phosphor — rain dim trail and CLI dim brand paint.
pub const PHOSPHOR_DIM: Rgb = Rgb::new(0, 140, 30);

/// Dark brand phosphor — rain tail.
pub const PHOSPHOR_DARK: Rgb = Rgb::new(0, 80, 18);

/// Pure black used for raw terminal brand text.
pub const BLACK: Rgb = Rgb::new(0, 0, 0);

/// White used for rain head and high-contrast brand text on dark surfaces.
pub const WHITE: Rgb = Rgb::new(255, 255, 255);

// --- Launch rain (product animation) ---

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

// --- Brand chrome ---

/// Brand pill background.
pub const BRAND_BLOCK: Rgb = PHOSPHOR_GREEN;

/// Link color on light status bars (product chrome on white chips).
pub const LINK_BLUE: Rgb = Rgb::new(0, 80, 180);

// --- Domain / agent status ---

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

/// Live-state cyan for domain status chips.
pub const CYAN: Rgb = Rgb::new(0, 180, 180);

/// Dim live-state cyan.
pub const CYAN_DIM: Rgb = Rgb::new(0, 120, 120);

/// Permitted-action accent (`+ Add …` rows).
pub const ACTION_ACCENT: Rgb = Rgb::new(180, 255, 180);

/// Disclosure-control accent.
pub const DISCLOSURE_ACCENT: Rgb = Rgb::new(255, 208, 102);
