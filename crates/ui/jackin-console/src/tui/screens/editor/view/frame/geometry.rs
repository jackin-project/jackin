// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor render preparation and frame geometry.

use super::{editor_frame_areas, render_editor_screen};
use ratatui::{Frame, layout::Rect};

use crate::tui::screens::editor::model::EditorTab;

use super::super::{EditorScrollGeometry, EditorTabContentGeometry, WorkspaceEditorState};

pub(crate) fn prepare_editor_for_render<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    area: Rect,
    state: &mut WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let body = editor_body_area(area, state.cached_footer_h);
    prepare_editor_tab_for_area(body, state, config);
}

pub(crate) fn prepare_editor_tab_for_area<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    body: Rect,
    state: &mut WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let geometry = editor_tab_geometry(body, state, config);
    state.tab_content_width = geometry.content_width;
    state.tab_content_height = geometry.content_height;
    clamp_editor_scroll_for_frame(
        body,
        EditorScrollGeometry {
            active_mounts: state.active_tab == EditorTab::Mounts,
            content_width: geometry.content_width,
            content_height: geometry.content_height,
            mounts_content_width:
                crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
                    &state.pending.mounts,
                    &state.mount_info_cache,
                ),
        },
        &mut state.tab_scroll,
        &mut state.workspace_mounts_scroll,
    );
}

#[must_use]
pub(crate) fn editor_tab_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> EditorTabContentGeometry {
    match state.active_tab {
        EditorTab::General => super::super::general_tab::general_state_geometry(state),
        EditorTab::Mounts => super::super::mounts_tab::mount_state_geometry(state),
        EditorTab::Roles => {
            super::super::roles_tab::role_state_geometry(state, config.roles.keys())
        }
        EditorTab::Secrets => {
            super::super::secrets_tab::secret_state_geometry(state, area.width, |role| {
                config.roles.contains_key(role)
            })
        }
        EditorTab::Auth => super::super::auth_tab::auth_state_geometry(state, config),
    }
}

pub(crate) fn clamp_editor_scroll_for_frame(
    body: Rect,
    geometry: EditorScrollGeometry,
    tab_scroll: &mut termrock::widgets::ScrollAreaState,
    mounts_scroll: &mut termrock::widgets::ScrollAreaState,
) {
    let viewport_w = termrock::scroll::viewport_width(body);
    let viewport_h = termrock::scroll::viewport_height(body);
    if geometry.active_mounts {
        mounts_scroll.set_content_size(
            u16::try_from(geometry.mounts_content_width).unwrap_or(u16::MAX),
            1,
        );
        mounts_scroll.set_viewport(u16::try_from(viewport_w).unwrap_or(u16::MAX), 1);
        mounts_scroll.clamp();
    }
    // State owns both axes: configuring one axis must preserve the other's
    // measured bounds for subsequent scroll input.
    tab_scroll.set_content_size(
        if geometry.active_mounts {
            1
        } else {
            u16::try_from(geometry.content_width).unwrap_or(u16::MAX)
        },
        u16::try_from(geometry.content_height).unwrap_or(u16::MAX),
    );
    tab_scroll.set_viewport(
        u16::try_from(viewport_w).unwrap_or(u16::MAX),
        u16::try_from(viewport_h).unwrap_or(u16::MAX),
    );
    tab_scroll.clamp();
}

pub(crate) fn editor_body_area(area: Rect, footer_h: u16) -> Rect {
    editor_frame_areas(area, footer_h).body
}

/// Concrete adapter: render the editor screen with the standard footer.
///
/// Equivalent to the generic `render_editor_screen` but binds the concrete
/// `EditorState<'_>` and `editor_footer_items` so callers do not need to
/// construct the footer closure themselves.
pub(crate) fn render_editor_with_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::state::EditorState<'_>,
    config: &jackin_config::AppConfig,
    op_available: bool,
) {
    render_editor_screen(frame, area, state, config, |state, config, body| {
        crate::tui::components::footer_hints::editor_footer_items(state, config, op_available, body)
    });
}
