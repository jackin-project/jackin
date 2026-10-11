// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings per-tab render dispatch.

use super::{
    ConsoleSettingsState, auth_state_lines, env_state_lines, general_state_lines,
    global_mount_state_lines, settings_trust_focused, settings_trust_lines_for_state,
};

use super::super::model::SettingsEnvRow;
use super::super::model::SettingsEnvScope;

use super::super::model::SettingsTab;

use ratatui::{Frame, layout::Rect};

use termrock::widgets::HintSpan;

use crate::tui::components::footer_hints::{
    SettingsContextFooterMode, content_footer_items, settings_contextual_row_footer_items,
    settings_save_footer_label, tab_bar_footer_items,
};

pub fn render_general_tab<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    area: Rect,
) {
    let focused = !state.tab_bar_focused() && state.error_popup.is_none();
    let lines = general_state_lines(&state.general, focused);
    crate::tui::scroll_block::render_scrollable_block_at(frame, area, lines, 0, 0, focused, None);
}

pub fn render_mounts_tab<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    area: Rect,
) {
    let focused = state.content_focused(SettingsTab::Mounts) && !state.mounts.modals.is_open();
    let selected = if focused {
        Some(state.mounts.selected)
    } else {
        None
    };
    let lines = global_mount_state_lines(&state.mounts, selected, true);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.mounts.scroll.offset_x(),
        state.mounts.scroll.offset_y(),
        focused,
        None,
    );
}

pub fn render_env_tab<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    area: Rect,
) {
    let focused = state.content_focused(SettingsTab::Environments) && !state.env.modals.is_open();
    let lines = env_state_lines(&state.env, focused, area.width);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.env.scroll.offset_y(),
        focused,
        None,
    );
}

pub fn render_auth_tab<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    area: Rect,
) {
    let title = Some("Accounts");
    let focused = state.content_focused(SettingsTab::Auth) && !state.auth.modals.is_open();
    let lines = auth_state_lines(&state.auth, &state.env, focused);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        0,
        state.auth.scroll.offset_y(),
        focused,
        title,
    );
}

pub fn render_trust_tab<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    frame: &mut Frame<'_>,
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    area: Rect,
) {
    let lines = settings_trust_lines_for_state(state);
    let focused = settings_trust_focused(state);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.trust.scroll.offset_x(),
        state.trust.scroll.offset_y(),
        focused,
        None,
    );
}

pub fn settings_footer_items<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if state.tab_bar_focused() {
        return tab_bar_footer_items(
            settings_save_footer_label(),
            true,
            state.is_dirty().then(|| state.change_count()),
        );
    }

    let row_items = settings_contextual_row_footer_items(
        settings_context_footer_mode(state, body_area),
        op_available,
    );
    content_footer_items(
        settings_save_footer_label(),
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}

pub(crate) fn settings_context_footer_mode<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    body_area: Rect,
) -> SettingsContextFooterMode {
    match state.active_tab {
        SettingsTab::General => SettingsContextFooterMode::General,
        SettingsTab::Mounts => {
            let cursor = state.mounts.selected;
            let mount_count = state.mounts.pending.len();
            if cursor == mount_count {
                SettingsContextFooterMode::MountAddRow
            } else {
                SettingsContextFooterMode::MountRow {
                    has_github_url: state
                        .mounts
                        .pending
                        .get(cursor)
                        .and_then(|row| {
                            state.mounts.mount_info_cache.github_web_url(&row.mount.src)
                        })
                        .is_some(),
                    scroll_axes: global_mount_scroll_axes(state, body_area),
                }
            }
        }
        SettingsTab::Environments => {
            let rows = state.env_flat_rows();
            match rows.get(state.env.selected) {
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_env_value_is_op_ref(state, scope, key) =>
                {
                    SettingsContextFooterMode::EnvOpRefRow
                }
                Some(SettingsEnvRow::Key { .. }) => SettingsContextFooterMode::EnvPlainRow,
                Some(SettingsEnvRow::RoleHeader { .. }) => SettingsContextFooterMode::EnvRoleHeader,
                Some(SettingsEnvRow::GlobalAddSentinel | SettingsEnvRow::RoleAddSentinel(_)) => {
                    SettingsContextFooterMode::EnvAddRow
                }
                Some(SettingsEnvRow::SectionSpacer) | None => SettingsContextFooterMode::Empty,
            }
        }
        SettingsTab::Auth => SettingsContextFooterMode::AuthManage,
        SettingsTab::Trust => SettingsContextFooterMode::Trust {
            has_roles: !state.trust.pending.is_empty(),
            scroll_axes: trust_scroll_axes(state, body_area),
        },
    }
}

pub(crate) fn trust_scroll_axes<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    body_area: Rect,
) -> termrock::scroll::ScrollAxes {
    let content = crate::tui::screens::settings::update::trust_content_width(&state.trust);
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.trust.pending.is_empty(),
        content,
        body_area,
    )
}

pub(crate) fn global_mount_scroll_axes<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    body_area: Rect,
) -> termrock::scroll::ScrollAxes {
    let content_width =
        crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache(
            &state.mounts.pending,
            &state.mounts.mount_info_cache,
        );
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.mounts.pending.is_empty(),
        content_width,
        body_area,
    )
}

pub(crate) fn settings_env_value_is_op_ref<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    scope: &SettingsEnvScope,
    key: &str,
) -> bool {
    state
        .env
        .pending_value(scope, key)
        .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
}
