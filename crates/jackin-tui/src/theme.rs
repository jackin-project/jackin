//! Ratatui adapters for shared jackin' design tokens.

use ratatui::style::Color;

use crate::{
    BORDER_GRAY as BORDER_GRAY_RGB, DANGER_RED as DANGER_RED_RGB, DEBUG_AMBER as DEBUG_AMBER_RGB,
    DIALOG_BACKDROP as DIALOG_BACKDROP_RGB, DIALOG_SCROLL_THUMB as DIALOG_SCROLL_THUMB_RGB,
    DIALOG_SCROLL_TRACK as DIALOG_SCROLL_TRACK_RGB, DIALOG_SURFACE as DIALOG_SURFACE_RGB,
    INPUT_BG_DIM as INPUT_BG_DIM_RGB, LINK_BLUE as LINK_BLUE_RGB,
    PHOSPHOR_DARK as PHOSPHOR_DARK_RGB, PHOSPHOR_DIM as PHOSPHOR_DIM_RGB,
    PHOSPHOR_GREEN as PHOSPHOR_GREEN_RGB, Rgb, TAB_BG_ACTIVE as TAB_BG_ACTIVE_RGB,
    TAB_BG_ACTIVE_HOVER as TAB_BG_ACTIVE_HOVER_RGB, TAB_BG_INACTIVE as TAB_BG_INACTIVE_RGB,
    TAB_BG_INACTIVE_HOVER as TAB_BG_INACTIVE_HOVER_RGB, WHITE as WHITE_RGB,
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
