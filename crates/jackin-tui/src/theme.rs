// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Ratatui adapters for shared jackin❯ design tokens.
//!
//! Also exposes named `Style` constants for the most-repeated combinations
//! (`BOLD_WHITE`, `BOLD_GREEN`, `DIM`, `DANGER`) so callers avoid writing
//! `crate::theme::BOLD_WHITE` inline.

use ratatui::style::{Color, Modifier, Style};

use crate::{
    ACTION_ACCENT as ACTION_ACCENT_RGB, BORDER_GRAY as BORDER_GRAY_RGB,
    BORDER_GRAY_LIGHT as BORDER_GRAY_LIGHT_RGB,
    CAPSULE_MENU_AWAITING_BG as CAPSULE_MENU_AWAITING_BG_RGB,
    CAPSULE_MENU_AWAITING_HOVER_BG as CAPSULE_MENU_AWAITING_HOVER_BG_RGB,
    CAPSULE_MENU_IDLE_BG as CAPSULE_MENU_IDLE_BG_RGB,
    CAPSULE_MENU_IDLE_HOVER_BG as CAPSULE_MENU_IDLE_HOVER_BG_RGB, CYAN as CYAN_RGB,
    CYAN_DIM as CYAN_DIM_RGB, DANGER_RED as DANGER_RED_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DIALOG_SCROLL_THUMB as DIALOG_SCROLL_THUMB_RGB, DIALOG_SCROLL_TRACK as DIALOG_SCROLL_TRACK_RGB,
    DISCLOSURE_ACCENT as DISCLOSURE_ACCENT_RGB, INPUT_BG_DIM as INPUT_BG_DIM_RGB,
    LINK_BLUE as LINK_BLUE_RGB, LINK_FG as LINK_FG_RGB, LINK_FG_HOVER as LINK_FG_HOVER_RGB,
    PHOSPHOR_DARK as PHOSPHOR_DARK_RGB, PHOSPHOR_DIM as PHOSPHOR_DIM_RGB,
    PHOSPHOR_GREEN as PHOSPHOR_GREEN_RGB, PREVIEW_CARD as PREVIEW_CARD_RGB, Rgb,
    STATUS_BLOCKED_RED as STATUS_BLOCKED_RED_RGB, TAB_BG_ACTIVE as TAB_BG_ACTIVE_RGB,
    TAB_BG_ACTIVE_HOVER as TAB_BG_ACTIVE_HOVER_RGB, TAB_BG_INACTIVE as TAB_BG_INACTIVE_RGB,
    TAB_BG_INACTIVE_HOVER as TAB_BG_INACTIVE_HOVER_RGB, WARNING_YELLOW as WARNING_YELLOW_RGB,
    WHITE as WHITE_RGB,
};

#[must_use]
pub const fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

pub const PHOSPHOR_GREEN: Color = color(PHOSPHOR_GREEN_RGB);
/// Logo block green: the canonical phosphor green (`#00FF41`), the same green
/// the rest of the CLI and the digital rain use. The brand pill sits on this
/// block with a black word and a white chevron — the logo uses the real jackin
/// green, not the muted `#5CF07A` chevron-accent.
pub const BRAND_BLOCK: Color = PHOSPHOR_GREEN;
pub const PHOSPHOR_DIM: Color = color(PHOSPHOR_DIM_RGB);
pub const PHOSPHOR_DARK: Color = color(PHOSPHOR_DARK_RGB);
pub const INPUT_BG_DIM: Color = color(INPUT_BG_DIM_RGB);
// Dialog backdrop and surface paint the terminal's DEFAULT background, not a
// fixed colour: `Color::Reset` emits `\x1b[49m`, so modal overlays match the
// operator's terminal theme instead of forcing pure black that stands out
// against a themed (non-black) default. Occlusion still holds — Reset cells
// overwrite the chrome behind them with a space on the default background.
pub const DIALOG_BACKDROP: Color = Color::Reset;
pub const DIALOG_SURFACE: Color = Color::Reset;
pub const DIALOG_SCROLL_THUMB: Color = color(DIALOG_SCROLL_THUMB_RGB);
pub const DIALOG_SCROLL_TRACK: Color = color(DIALOG_SCROLL_TRACK_RGB);
pub const WHITE: Color = color(WHITE_RGB);
/// Foreground for text on bright chips/buttons.
///
/// ANSI black by design so terminals map it consistently with their palette.
pub const INK: Color = Color::Black;
pub const TAB_BG_INACTIVE: Color = color(TAB_BG_INACTIVE_RGB);
pub const TAB_BG_INACTIVE_HOVER: Color = color(TAB_BG_INACTIVE_HOVER_RGB);
pub const TAB_BG_ACTIVE: Color = color(TAB_BG_ACTIVE_RGB);
pub const TAB_BG_ACTIVE_HOVER: Color = color(TAB_BG_ACTIVE_HOVER_RGB);
pub const LINK_BLUE: Color = color(LINK_BLUE_RGB);
pub const LINK_FG: Color = color(LINK_FG_RGB);
pub const LINK_FG_HOVER: Color = color(LINK_FG_HOVER_RGB);
pub const DEBUG_AMBER: Color = color(DEBUG_AMBER_RGB);
pub const BORDER_GRAY: Color = color(BORDER_GRAY_RGB);
pub const BORDER_GRAY_LIGHT: Color = color(BORDER_GRAY_LIGHT_RGB);
pub const DANGER_RED: Color = color(DANGER_RED_RGB);
pub const STATUS_BLOCKED_RED: Color = color(STATUS_BLOCKED_RED_RGB);
pub const CYAN: Color = color(CYAN_RGB);
pub const CYAN_DIM: Color = color(CYAN_DIM_RGB);
pub const ACTION_ACCENT: Color = color(ACTION_ACCENT_RGB);
pub const DISCLOSURE_ACCENT: Color = color(DISCLOSURE_ACCENT_RGB);
pub const WARNING_YELLOW: Color = color(WARNING_YELLOW_RGB);
pub const PREVIEW_CARD: Color = color(PREVIEW_CARD_RGB);
pub const DIFF_REMOVED_BG: Color = Color::Rgb(60, 20, 20);
pub const DIFF_ADDED_BG: Color = Color::Rgb(20, 50, 20);
pub const DIFF_REMOVED_FG: Color = DANGER_RED;
pub const DIFF_ADDED_FG: Color = PHOSPHOR_GREEN;
pub const CAPSULE_MENU_IDLE_BG: Color = color(CAPSULE_MENU_IDLE_BG_RGB);
pub const CAPSULE_MENU_IDLE_HOVER_BG: Color = color(CAPSULE_MENU_IDLE_HOVER_BG_RGB);
pub const CAPSULE_MENU_AWAITING_BG: Color = color(CAPSULE_MENU_AWAITING_BG_RGB);
pub const CAPSULE_MENU_AWAITING_HOVER_BG: Color = color(CAPSULE_MENU_AWAITING_HOVER_BG_RGB);

/// Named style constants — the most-repeated `Style::default().fg(…).add_modifier(…)` chains.
pub const BOLD_WHITE: Style = Style::new().fg(WHITE).add_modifier(Modifier::BOLD);
pub const BOLD_GREEN: Style = Style::new().fg(PHOSPHOR_GREEN).add_modifier(Modifier::BOLD);
pub const DIM: Style = Style::new().fg(PHOSPHOR_DIM);
pub const GREEN: Style = Style::new().fg(PHOSPHOR_GREEN);
pub const BORDER: Style = Style::new().fg(BORDER_GRAY);
pub const DANGER: Style = Style::new().fg(DANGER_RED).add_modifier(Modifier::BOLD);

#[must_use]
pub fn faded(color: Color, alpha: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            #[expect(
                clippy::cast_sign_loss,
                reason = "alpha clamped to 0.0..=1.0; product stays in u8 range"
            )]
            let scale = |component: u8| (f32::from(component) * alpha.clamp(0.0, 1.0)) as u8;
            Color::Rgb(scale(r), scale(g), scale(b))
        }
        other => other,
    }
}
