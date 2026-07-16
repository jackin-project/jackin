// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! jackin❯ product visual tokens and `TermRock` theme access.
//!
//! Reusable widget semantics come from `TermRock`'s [`termrock::Theme`] and
//! [`termrock::style::Role`]. This module keeps only product-owned Ratatui
//! tokens (brand chrome, agent menu/status accents, action rows) and helpers
//! that resolve shared presentation through `Theme::default()`.

use ratatui::style::{Color, Style};
use termrock::Theme;
use termrock::style::Role;

use crate::{
    ACTION_ACCENT as ACTION_ACCENT_RGB, BRAND_BLOCK as BRAND_BLOCK_RGB, CYAN as CYAN_RGB,
    CYAN_DIM as CYAN_DIM_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DISCLOSURE_ACCENT as DISCLOSURE_ACCENT_RGB, LINK_BLUE as LINK_BLUE_RGB,
    MENU_AWAITING_BG as MENU_AWAITING_BG_RGB, MENU_AWAITING_HOVER_BG as MENU_AWAITING_HOVER_BG_RGB,
    MENU_IDLE_BG as MENU_IDLE_BG_RGB, MENU_IDLE_HOVER_BG as MENU_IDLE_HOVER_BG_RGB, Rgb,
    STATUS_BLOCKED_RED as STATUS_BLOCKED_RED_RGB,
};

/// Convert a product-owned RGB token into a Ratatui color.
#[must_use]
pub const fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Canonical `TermRock` theme used by every jackin❯ surface.
#[must_use]
pub fn theme() -> Theme {
    Theme::default()
}

/// Resolve a `TermRock` semantic role from the default theme.
#[must_use]
pub fn role(role: Role) -> Style {
    Theme::default().style(role)
}

fn style_fg(role: Role, fallback: Color) -> Color {
    Theme::default().style(role).fg.unwrap_or(fallback)
}

fn style_bg(role: Role, fallback: Color) -> Color {
    Theme::default().style(role).bg.unwrap_or(fallback)
}

/// Foreground RGB for a `TermRock` role, for raw-ANSI surfaces that cannot paint
/// Ratatui `Style`s.
#[must_use]
pub fn role_rgb(role: Role) -> Rgb {
    match style_fg(role, Color::White) {
        Color::Rgb(r, g, b) => Rgb::new(r, g, b),
        _ => Rgb::new(255, 255, 255),
    }
}

/// Scrollbar thumb RGB from `Role::ScrollThumb`.
#[must_use]
pub fn scroll_thumb_rgb() -> Rgb {
    role_rgb(Role::ScrollThumb)
}

/// Inactive scrollbar / border RGB from `Role::Border`.
#[must_use]
pub fn border_rgb() -> Rgb {
    role_rgb(Role::Border)
}

// --- Shared presentation via TermRock roles ---

/// Ordinary body text style.
#[must_use]
pub fn text() -> Style {
    role(Role::Text)
}
/// Strong / heading text style.
#[must_use]
pub fn text_strong() -> Style {
    role(Role::TextStrong)
}
/// Muted body text style.
#[must_use]
pub fn text_muted() -> Style {
    role(Role::TextMuted)
}
/// Focus / accent style.
#[must_use]
pub fn accent() -> Style {
    role(Role::Accent)
}
/// Danger style.
#[must_use]
pub fn danger() -> Style {
    role(Role::Danger)
}
/// Warning style.
#[must_use]
pub fn warning() -> Style {
    role(Role::Warning)
}
/// Inactive border style.
#[must_use]
pub fn border() -> Style {
    role(Role::Border)
}
/// Focused border style.
#[must_use]
pub fn border_focused() -> Style {
    role(Role::BorderFocused)
}

/// Ordinary text foreground.
#[must_use]
pub fn text_fg() -> Color {
    style_fg(Role::Text, Color::White)
}
/// Accent / phosphor focus foreground.
#[must_use]
pub fn accent_fg() -> Color {
    style_fg(Role::Accent, Color::Green)
}
/// Muted text foreground.
#[must_use]
pub fn muted_fg() -> Color {
    style_fg(Role::TextMuted, Color::DarkGray)
}
/// Inactive border foreground.
#[must_use]
pub fn border_fg() -> Color {
    style_fg(Role::Border, Color::DarkGray)
}
/// Danger foreground.
#[must_use]
pub fn danger_fg() -> Color {
    style_fg(Role::Danger, Color::Red)
}
/// Warning foreground.
#[must_use]
pub fn warning_fg() -> Color {
    style_fg(Role::Warning, Color::Yellow)
}
/// Info foreground.
#[must_use]
pub fn info_fg() -> Color {
    style_fg(Role::Info, Color::Cyan)
}
/// Link foreground.
#[must_use]
pub fn link_fg() -> Color {
    style_fg(Role::Link, Color::Cyan)
}
/// Link hover foreground.
#[must_use]
pub fn link_fg_hover() -> Color {
    style_fg(Role::LinkHover, Color::Cyan)
}
/// Scroll-track / dark phosphor separator.
#[must_use]
pub fn scroll_track_fg() -> Color {
    style_fg(Role::ScrollTrack, Color::DarkGray)
}
/// Input background.
#[must_use]
pub fn input_bg() -> Color {
    style_bg(Role::Input, Color::Black)
}
/// Active tab background.
#[must_use]
pub fn tab_active_bg() -> Color {
    style_bg(Role::TabActive, Color::DarkGray)
}
/// Inactive tab background.
#[must_use]
pub fn tab_inactive_bg() -> Color {
    style_bg(Role::TabInactive, Color::Black)
}
/// Active tab hover background.
#[must_use]
pub fn tab_active_hover_bg() -> Color {
    style_bg(Role::TabActiveHovered, Color::DarkGray)
}
/// Inactive tab hover background.
#[must_use]
pub fn tab_inactive_hover_bg() -> Color {
    style_bg(Role::TabInactiveHovered, Color::DarkGray)
}

/// Dialog backdrop on the terminal default background.
pub const DIALOG_BACKDROP: Color = Color::Reset;
/// Dialog surface on the terminal default background.
pub const DIALOG_SURFACE: Color = Color::Reset;

// --- Product-owned tokens ---

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
mod tests {
    use super::*;

    /// Product brand/domain tokens must stay distinct from neutral theme roles
    /// except brand green, which intentionally shares the accent phosphor.
    #[test]
    fn product_tokens_are_domain_owned() {
        assert_ne!(DEBUG_AMBER, accent_fg());
        assert_ne!(STATUS_BLOCKED_RED, danger_fg());
        assert_ne!(MENU_IDLE_BG, tab_inactive_bg());
        assert_eq!(BRAND_BLOCK, accent_fg());
    }
}
