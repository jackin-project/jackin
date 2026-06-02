//! Ratatui adapters for shared jackin' design tokens.
//!
//! Also exposes named `Style` constants for the most-repeated combinations
//! (`BOLD_WHITE`, `BOLD_GREEN`, `DIM`, `DANGER`) so callers avoid writing
//! `Style::default().fg(WHITE).add_modifier(Modifier::BOLD)` inline.

use ratatui::style::{Color, Modifier, Style};

use crate::{
    ACTION_ACCENT as ACTION_ACCENT_RGB, BORDER_GRAY as BORDER_GRAY_RGB, CYAN as CYAN_RGB,
    CYAN_DIM as CYAN_DIM_RGB, DANGER_RED as DANGER_RED_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DIALOG_BACKDROP as DIALOG_BACKDROP_RGB, DIALOG_SCROLL_THUMB as DIALOG_SCROLL_THUMB_RGB,
    DIALOG_SCROLL_TRACK as DIALOG_SCROLL_TRACK_RGB, DIALOG_SURFACE as DIALOG_SURFACE_RGB,
    DISCLOSURE_ACCENT as DISCLOSURE_ACCENT_RGB, INPUT_BG_DIM as INPUT_BG_DIM_RGB,
    LINK_BLUE as LINK_BLUE_RGB, PHOSPHOR_DARK as PHOSPHOR_DARK_RGB,
    PHOSPHOR_DIM as PHOSPHOR_DIM_RGB, PHOSPHOR_GREEN as PHOSPHOR_GREEN_RGB, Rgb,
    TAB_BG_ACTIVE as TAB_BG_ACTIVE_RGB, TAB_BG_ACTIVE_HOVER as TAB_BG_ACTIVE_HOVER_RGB,
    TAB_BG_INACTIVE as TAB_BG_INACTIVE_RGB, TAB_BG_INACTIVE_HOVER as TAB_BG_INACTIVE_HOVER_RGB,
    WHITE as WHITE_RGB,
};

#[must_use]
pub const fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

pub const PHOSPHOR_GREEN: Color = color(PHOSPHOR_GREEN_RGB);
pub const PHOSPHOR_DIM: Color = color(PHOSPHOR_DIM_RGB);
pub const PHOSPHOR_DARK: Color = color(PHOSPHOR_DARK_RGB);
pub const INPUT_BG_DIM: Color = color(INPUT_BG_DIM_RGB);
pub const DIALOG_BACKDROP: Color = color(DIALOG_BACKDROP_RGB);
pub const DIALOG_SURFACE: Color = color(DIALOG_SURFACE_RGB);
pub const DIALOG_SCROLL_THUMB: Color = color(DIALOG_SCROLL_THUMB_RGB);
pub const DIALOG_SCROLL_TRACK: Color = color(DIALOG_SCROLL_TRACK_RGB);
pub const WHITE: Color = color(WHITE_RGB);
pub const TAB_BG_INACTIVE: Color = color(TAB_BG_INACTIVE_RGB);
pub const TAB_BG_INACTIVE_HOVER: Color = color(TAB_BG_INACTIVE_HOVER_RGB);
pub const TAB_BG_ACTIVE: Color = color(TAB_BG_ACTIVE_RGB);
pub const TAB_BG_ACTIVE_HOVER: Color = color(TAB_BG_ACTIVE_HOVER_RGB);
pub const LINK_BLUE: Color = color(LINK_BLUE_RGB);
pub const DEBUG_AMBER: Color = color(DEBUG_AMBER_RGB);
pub const BORDER_GRAY: Color = color(BORDER_GRAY_RGB);
pub const DANGER_RED: Color = color(DANGER_RED_RGB);
pub const CYAN: Color = color(CYAN_RGB);
pub const CYAN_DIM: Color = color(CYAN_DIM_RGB);
pub const ACTION_ACCENT: Color = color(ACTION_ACCENT_RGB);
pub const DISCLOSURE_ACCENT: Color = color(DISCLOSURE_ACCENT_RGB);

/// Named style constants — the most-repeated `Style::default().fg(…).add_modifier(…)` chains.
pub const BOLD_WHITE: Style = Style::new().fg(WHITE).add_modifier(Modifier::BOLD);
pub const BOLD_GREEN: Style = Style::new().fg(PHOSPHOR_GREEN).add_modifier(Modifier::BOLD);
pub const DIM: Style = Style::new().fg(PHOSPHOR_DIM);
pub const GREEN: Style = Style::new().fg(PHOSPHOR_GREEN);
pub const DANGER: Style = Style::new().fg(DANGER_RED).add_modifier(Modifier::BOLD);

#[must_use]
pub fn faded(color: Color, alpha: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let scale = |component: u8| (f32::from(component) * alpha.clamp(0.0, 1.0)) as u8;
            Color::Rgb(scale(r), scale(g), scale(b))
        }
        other => other,
    }
}
