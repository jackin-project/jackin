// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings lines and content metrics.

use super::{ConsoleSettingsState, env_state_lines, trust_state_lines};

use super::super::model::SettingsTab;

use ratatui::text::Line;

pub fn settings_env_lines_for_state<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
    area_width: u16,
) -> Vec<Line<'static>> {
    let show_cursor =
        state.content_focused(SettingsTab::Environments) && !state.env.modals.is_open();
    env_state_lines(&state.env, show_cursor, area_width)
}

pub fn settings_trust_lines_for_state<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
) -> Vec<Line<'static>> {
    trust_state_lines(
        &state.trust,
        state.hovered_trust_row(),
        settings_trust_focused(state),
    )
}

pub(crate) fn settings_trust_focused<
    MountModal,
    EnvModal,
    AuthModal,
    ErrorPopup,
    PendingOpCommit,
>(
    state: &ConsoleSettingsState<MountModal, EnvModal, AuthModal, ErrorPopup, PendingOpCommit>,
) -> bool {
    state.content_focused(SettingsTab::Trust)
        && !state.auth.modals.is_open()
        && !state.env.modals.is_open()
        && !state.mounts.modals.is_open()
}

#[must_use]
pub fn content_height_with_error_rows(height: usize, has_error: bool) -> usize {
    if has_error {
        height.saturating_add(2)
    } else {
        height
    }
}

#[must_use]
pub fn mounts_content_height(row_height: usize, has_error: bool) -> usize {
    content_height_with_error_rows(row_height, has_error)
}

#[must_use]
pub fn env_content_height(row_count: usize, has_error: bool) -> usize {
    content_height_with_error_rows(row_count, has_error)
}

#[must_use]
pub fn trust_content_height(row_count: usize, has_error: bool) -> usize {
    content_height_with_error_rows(1 + row_count.max(1), has_error)
}
