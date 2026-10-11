// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings frame layout and screen render.

use super::{
    render_auth_tab, render_env_tab, render_general_tab, render_mounts_tab, render_trust_tab,
    settings_header_title, tab_labels,
};

use super::super::model::GlobalMountsState;
use super::super::model::SettingsAuthState;

use super::super::model::SettingsEnvState;

use super::super::model::SettingsState;
use super::super::model::SettingsTab;

use super::super::model::SettingsTrustState;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::tui::components::editor_rows::render_tab_strip;
use termrock::widgets::HintSpan;

use crate::tui::view::{
    effective_footer_height, measured_footer_height, render_footer, render_header,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsFrameAreas {
    pub header: Rect,
    pub tabs: Rect,
    pub body: Rect,
    pub footer: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsModalRenderPlan {
    ErrorPopup,
    Mounts,
    Environments,
    Auth,
    None,
}

pub type ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit> =
    SettingsState<
        GlobalMountsState<jackin_config::GlobalMountRow, MountModal>,
        SettingsEnvState<jackin_core::EnvValue, EnvModal>,
        SettingsAuthState<jackin_core::EnvValue, AuthModal, PendingOpCommit>,
        SettingsTrustState,
        ErrorPopup,
    >;

pub fn settings_frame_areas(area: Rect, footer_h: u16) -> SettingsFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    SettingsFrameAreas {
        header: chunks[0],
        tabs: chunks[1],
        body: chunks[2],
        footer: chunks[3],
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four orthogonal settings-modal visibility flags (error_popup, \
              mounts_modal, env_modal, auth_modal) — each is an independent \
              picker-open signal the render-plan resolver inspects to pick the \
              correct modal render target. Named-arg reads match the per-picker \
              visibility-routing idiom."
)]
#[must_use]
pub const fn settings_modal_render_plan(
    error_popup_open: bool,
    mounts_modal_open: bool,
    env_modal_open: bool,
    auth_modal_open: bool,
) -> SettingsModalRenderPlan {
    if error_popup_open {
        return SettingsModalRenderPlan::ErrorPopup;
    }
    if mounts_modal_open {
        return SettingsModalRenderPlan::Mounts;
    }
    if env_modal_open {
        return SettingsModalRenderPlan::Environments;
    }
    if auth_modal_open {
        return SettingsModalRenderPlan::Auth;
    }
    SettingsModalRenderPlan::None
}

pub fn render_settings_screen<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
    FooterItems,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    mut footer_items: FooterItems,
) where
    FooterItems: FnMut(
        &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
        Rect,
    ) -> Vec<HintSpan<'static>>,
{
    let provisional_body =
        settings_frame_areas(area, effective_footer_height(state.cached_footer_h)).body;
    let footer = footer_items(state, provisional_body);
    let mut footer_h = measured_footer_height(&footer, area.width);
    let mut areas = settings_frame_areas(area, footer_h);
    let mut footer = footer_items(state, areas.body);
    let exact_footer_h = measured_footer_height(&footer, area.width);
    if exact_footer_h != footer_h {
        footer_h = exact_footer_h;
        areas = settings_frame_areas(area, footer_h);
        footer = footer_items(state, areas.body);
    }
    render_header(frame, areas.header, settings_header_title());
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
        state.tab_bar_focused(),
        state.hovered_tab(),
    );

    match state.active_tab {
        SettingsTab::General => render_general_tab(frame, state, areas.body),
        SettingsTab::Mounts => render_mounts_tab(frame, state, areas.body),
        SettingsTab::Environments => render_env_tab(frame, state, areas.body),
        SettingsTab::Auth => render_auth_tab(frame, state, areas.body),
        SettingsTab::Trust => render_trust_tab(frame, state, areas.body),
    }

    render_footer(frame, areas.footer, &footer);
}
