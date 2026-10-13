// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor frame areas and top-level screen render.

use super::{
    render_auth_tab, render_general_tab, render_mounts_tab, render_roles_tab, render_secrets_tab,
};
use crate::tui::components::editor_rows::render_tab_strip;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use termrock::widgets::HintSpan;

use super::super::{EditorFrameAreas, WorkspaceEditorState, tab_labels};
use crate::tui::screens::editor::model::EditorTab;
use crate::tui::view::{
    effective_footer_height, measured_footer_height, render_footer, render_header,
};

pub(crate) fn editor_frame_areas(area: Rect, footer_h: u16) -> EditorFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    EditorFrameAreas {
        header: chunks[0],
        tabs: chunks[1],
        body: chunks[2],
        footer: chunks[3],
    }
}

pub(crate) fn render_editor_screen<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
    FooterItems,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    mut footer_items: FooterItems,
) where
    FooterItems: FnMut(
        &WorkspaceEditorState<
            Modal,
            SaveFlow,
            jackin_core::EnvValue,
            PendingRoleLoad,
            PendingDriftCheck,
            PendingIsolationCleanup,
            PendingOpCommit,
        >,
        &jackin_config::AppConfig,
        Rect,
    ) -> Vec<HintSpan<'static>>,
{
    let provisional_body =
        editor_frame_areas(area, effective_footer_height(state.cached_footer_h)).body;
    let items = footer_items(state, config, provisional_body);
    let mut footer_h = measured_footer_height(&items, area.width);
    let mut areas = editor_frame_areas(area, footer_h);
    let mut items = footer_items(state, config, areas.body);
    let exact_footer_h = measured_footer_height(&items, area.width);
    if exact_footer_h != footer_h {
        footer_h = exact_footer_h;
        areas = editor_frame_areas(area, footer_h);
        items = footer_items(state, config, areas.body);
    }

    let title = super::super::editor_header_title(&state.mode);
    render_header(frame, areas.header, &title);
    render_tab_strip(
        frame,
        areas.tabs,
        &tab_labels(state.active_tab),
        state.tab_bar_focused(),
        state.hovered_tab(),
    );

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, areas.body, state),
        EditorTab::Mounts => render_mounts_tab(frame, areas.body, state),
        EditorTab::Roles => render_roles_tab(frame, areas.body, state, config),
        EditorTab::Secrets => render_secrets_tab(frame, areas.body, state, config),
        EditorTab::Auth => render_auth_tab(frame, areas.body, state, config),
    }

    render_footer(frame, areas.footer, &items);
}
