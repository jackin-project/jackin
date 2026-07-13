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
