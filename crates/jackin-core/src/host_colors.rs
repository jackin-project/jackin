// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cross-crate colour types and brand palette tokens.
//!
//! Three-byte RGB triple and the canonical phosphor palette, plus the
//! `owo_rgb` adapter. Originally defined in `jackin-tui`; lifted to
//! `jackin-core` as part of the A5 port-trait unblock work so
//! `jackin-runtime` can use the brand palette + colour adapter
//! without depending on the L3 presentation crate.
//!
//! Architecture Invariant: depends only on `std` and the `owo-colors`
//! crate. No `jackin-*` deps.

/// Three-byte RGB triple. Constructors below are the canonical
/// phosphor palette used everywhere a jackin TUI surface needs to
/// pick a colour.
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

/// `--jk-brand` — the bright phosphor green used for selection
/// highlights, the row-0 brand pill, and live indicators.
pub const PHOSPHOR_GREEN: Rgb = Rgb::new(0, 255, 65);

/// Mid-green used for inactive tab labels, dim labels, and "Dyn"
/// footer text in the console.
pub const PHOSPHOR_DIM: Rgb = Rgb::new(0, 140, 30);

/// Dark green used for panel borders and dot separators.
pub const PHOSPHOR_DARK: Rgb = Rgb::new(0, 80, 18);

/// Pure black used for raw terminal brand text.
pub const BLACK: Rgb = Rgb::new(0, 0, 0);

/// White used for titles, keys, and high-contrast text.
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

/// Subtle text-input background.
pub const INPUT_BG_DIM: Rgb = Rgb::new(20, 24, 22);

/// Inactive tab background.
pub const TAB_BG_INACTIVE: Rgb = Rgb::new(30, 30, 30);

/// Hovered inactive tab background.
pub const TAB_BG_INACTIVE_HOVER: Rgb = Rgb::new(48, 48, 48);

/// Active tab background.
pub const TAB_BG_ACTIVE: Rgb = Rgb::new(42, 42, 42);

/// Hovered active tab background.
pub const TAB_BG_ACTIVE_HOVER: Rgb = Rgb::new(58, 58, 58);

/// Link color on light status bars.
pub const LINK_BLUE: Rgb = Rgb::new(0, 80, 180);

/// Link color on dark surfaces.
pub const LINK_FG: Rgb = Rgb::new(0, 200, 200);

/// Hovered link color on dark surfaces.
pub const LINK_FG_HOVER: Rgb = Rgb::new(130, 240, 240);

/// Debug-mode chrome accent.
pub const DEBUG_AMBER: Rgb = Rgb::new(204, 92, 0);

/// Neutral inactive-border gray.
pub const BORDER_GRAY: Rgb = Rgb::new(80, 80, 80);

/// Lighter inactive scrollbar gray.
pub const BORDER_GRAY_LIGHT: Rgb = Rgb::new(160, 160, 160);

/// Error and danger accent.
pub const DANGER_RED: Rgb = Rgb::new(255, 94, 122);

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

/// Live-state cyan.
pub const CYAN: Rgb = Rgb::new(0, 180, 180);

/// Dim live-state cyan.
pub const CYAN_DIM: Rgb = Rgb::new(0, 120, 120);

/// Permitted-action accent.
pub const ACTION_ACCENT: Rgb = Rgb::new(180, 255, 180);

/// Disclosure-control accent.
pub const DISCLOSURE_ACCENT: Rgb = Rgb::new(255, 208, 102);

/// Warning accent.
pub const WARNING_YELLOW: Rgb = Rgb::new(255, 216, 94);
