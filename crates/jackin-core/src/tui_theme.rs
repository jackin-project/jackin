// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Ratatui adapters for jackin❯-specific visual tokens.
//!
//! Reusable widget semantics come from TermRock's [`termrock::Theme`]. These
//! constants are only the product-owned colors TermRock deliberately does not
//! own: brand chrome, launch rain, debug/status accents, and compatibility
//! adapters for raw jackin❯ rendering paths.

use ratatui::style::{Color, Modifier, Style};

use crate::{
    ACTION_ACCENT as ACTION_ACCENT_RGB, BORDER_GRAY as BORDER_GRAY_RGB,
    BORDER_GRAY_LIGHT as BORDER_GRAY_LIGHT_RGB, BRAND_BLOCK as BRAND_BLOCK_RGB, CYAN as CYAN_RGB,
    CYAN_DIM as CYAN_DIM_RGB, DANGER_RED as DANGER_RED_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DISCLOSURE_ACCENT as DISCLOSURE_ACCENT_RGB, INPUT_BG_DIM as INPUT_BG_DIM_RGB,
    LINK_BLUE as LINK_BLUE_RGB, LINK_FG as LINK_FG_RGB, LINK_FG_HOVER as LINK_FG_HOVER_RGB,
    MENU_AWAITING_BG as MENU_AWAITING_BG_RGB, MENU_AWAITING_HOVER_BG as MENU_AWAITING_HOVER_BG_RGB,
    MENU_IDLE_BG as MENU_IDLE_BG_RGB, MENU_IDLE_HOVER_BG as MENU_IDLE_HOVER_BG_RGB,
    PHOSPHOR_DARK as PHOSPHOR_DARK_RGB, PHOSPHOR_DIM as PHOSPHOR_DIM_RGB,
    PHOSPHOR_GREEN as PHOSPHOR_GREEN_RGB, Rgb, STATUS_BLOCKED_RED as STATUS_BLOCKED_RED_RGB,
    TAB_BG_ACTIVE as TAB_BG_ACTIVE_RGB, TAB_BG_ACTIVE_HOVER as TAB_BG_ACTIVE_HOVER_RGB,
    TAB_BG_INACTIVE as TAB_BG_INACTIVE_RGB, TAB_BG_INACTIVE_HOVER as TAB_BG_INACTIVE_HOVER_RGB,
    WARNING_YELLOW as WARNING_YELLOW_RGB, WHITE as WHITE_RGB,
};

/// Convert a product-owned RGB token into a Ratatui color.
#[must_use]
pub const fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Active/focused phosphor green.
pub const PHOSPHOR_GREEN: Color = color(PHOSPHOR_GREEN_RGB);
/// Dim phosphor text.
pub const PHOSPHOR_DIM: Color = color(PHOSPHOR_DIM_RGB);
/// Dark phosphor separator color.
pub const PHOSPHOR_DARK: Color = color(PHOSPHOR_DARK_RGB);
/// Brand pill background.
pub const BRAND_BLOCK: Color = color(BRAND_BLOCK_RGB);
/// Input background.
pub const INPUT_BG_DIM: Color = color(INPUT_BG_DIM_RGB);
/// Dialog backdrop on the terminal's default background.
pub const DIALOG_BACKDROP: Color = Color::Reset;
/// Dialog surface on the terminal's default background.
pub const DIALOG_SURFACE: Color = Color::Reset;
/// Dialog scrollbar thumb.
pub const DIALOG_SCROLL_THUMB: Color = PHOSPHOR_GREEN;
/// Dialog scrollbar track.
pub const DIALOG_SCROLL_TRACK: Color = PHOSPHOR_DARK;
/// High-contrast white.
pub const WHITE: Color = color(WHITE_RGB);
/// Text on bright chips.
pub const INK: Color = Color::Black;
/// Inactive tab background.
pub const TAB_BG_INACTIVE: Color = color(TAB_BG_INACTIVE_RGB);
/// Hovered inactive tab background.
pub const TAB_BG_INACTIVE_HOVER: Color = color(TAB_BG_INACTIVE_HOVER_RGB);
/// Active tab background.
pub const TAB_BG_ACTIVE: Color = color(TAB_BG_ACTIVE_RGB);
/// Hovered active tab background.
pub const TAB_BG_ACTIVE_HOVER: Color = color(TAB_BG_ACTIVE_HOVER_RGB);
/// Link color on light bars.
pub const LINK_BLUE: Color = color(LINK_BLUE_RGB);
/// Link color on dark surfaces.
pub const LINK_FG: Color = color(LINK_FG_RGB);
/// Hovered link color.
pub const LINK_FG_HOVER: Color = color(LINK_FG_HOVER_RGB);
/// Debug accent.
pub const DEBUG_AMBER: Color = color(DEBUG_AMBER_RGB);
/// Inactive border gray.
pub const BORDER_GRAY: Color = color(BORDER_GRAY_RGB);
/// Inactive scrollbar gray.
pub const BORDER_GRAY_LIGHT: Color = color(BORDER_GRAY_LIGHT_RGB);
/// Danger accent.
pub const DANGER_RED: Color = color(DANGER_RED_RGB);
/// Blocked status accent.
pub const STATUS_BLOCKED_RED: Color = color(STATUS_BLOCKED_RED_RGB);
/// Live-state cyan.
pub const CYAN: Color = color(CYAN_RGB);
/// Dim live-state cyan.
pub const CYAN_DIM: Color = color(CYAN_DIM_RGB);
/// Action accent.
pub const ACTION_ACCENT: Color = color(ACTION_ACCENT_RGB);
/// Disclosure accent.
pub const DISCLOSURE_ACCENT: Color = color(DISCLOSURE_ACCENT_RGB);
/// Warning accent.
pub const WARNING_YELLOW: Color = color(WARNING_YELLOW_RGB);
/// Idle menu background.
pub const MENU_IDLE_BG: Color = color(MENU_IDLE_BG_RGB);
/// Hovered idle menu background.
pub const MENU_IDLE_HOVER_BG: Color = color(MENU_IDLE_HOVER_BG_RGB);
/// Awaiting-command menu background.
pub const MENU_AWAITING_BG: Color = color(MENU_AWAITING_BG_RGB);
/// Hovered awaiting-command menu background.
pub const MENU_AWAITING_HOVER_BG: Color = color(MENU_AWAITING_HOVER_BG_RGB);

/// Bold white text.
pub const BOLD_WHITE: Style = Style::new().fg(WHITE).add_modifier(Modifier::BOLD);
/// Dim phosphor text.
pub const DIM: Style = Style::new().fg(PHOSPHOR_DIM);
/// Bright phosphor text.
pub const GREEN: Style = Style::new().fg(PHOSPHOR_GREEN);
/// Inactive border style.
pub const BORDER: Style = Style::new().fg(BORDER_GRAY);
/// Danger label style.
pub const DANGER: Style = Style::new().fg(DANGER_RED).add_modifier(Modifier::BOLD);
