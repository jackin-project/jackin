// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Top-level console frame composition helpers.
mod areas;
mod brand_header_crop;
mod chrome;
mod geometry;
mod helpers;
mod modal;
mod plans;
mod png_baselines;
mod render;
#[cfg(test)]
mod tests;
pub use areas::{
    ConsoleMainFramePlan, ConsoleModalRenderPlan, ConsolePrepareFramePlan,
    ConsoleReservedFooterHeightPlan, ModalContentAreas, ModalOverlayState,
    ReservedFooterHeightFacts, StageFooterHeightFacts, StageModalArea, VisibleModalPrepareAreas,
    WorkspaceFrameAreas,
};
pub use chrome::{
    delete_confirm_area, effective_footer_height, footer_height, measured_footer_height,
    purge_confirm_area, render_footer, render_header, render_modal_backdrop, settings_error_area,
    status_overlay_area, workspace_header_title,
};
pub use geometry::{
    modal_backdrop_area, modal_content_area, modal_content_areas, stage_modal_area_for_route,
    visible_modal_prepare_areas, visible_modal_prepare_areas_for_stage_facts,
    workspace_frame_areas,
};
pub(crate) use helpers::{has_modal_overlay, render_usage_surface, reserved_footer_height};
pub use modal::{prepare_for_render, render_modal};
pub use plans::{
    console_main_frame_plan, console_modal_render_plan, console_prepare_frame_plan,
    console_reserved_footer_height_plan, modal_overlay_state_for_route,
    modal_overlay_state_from_stage_facts, modal_overlay_visible, reserved_footer_height_for_facts,
};
pub use render::render;

#[cfg(test)]
pub(crate) use ratatui::Frame;
