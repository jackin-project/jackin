//! Footer hint items for the settings screen.

use crate::console::tui::components::footer::modal::{
    settings_auth_modal_footer_items, settings_env_modal_footer_items,
    settings_mounts_modal_footer_items,
};
use crate::console::tui::state::{
    SettingsEnvRow, SettingsEnvScope, SettingsState, SettingsTab, settings_env_flat_rows,
};
use crate::operator_env::EnvValue;
use jackin_console::tui::components::footer_hints::{
    SettingsContextFooterMode, content_footer_items, settings_contextual_row_footer_items,
    settings_save_footer_label, tab_bar_footer_items,
};
use jackin_tui::{HintSpan, components::ScrollAxes};
use ratatui::layout::Rect;

pub(crate) fn settings_footer_items(
    state: &SettingsState<'_>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if state.auth.modal.is_some() {
        settings_auth_modal_footer_items(&state.auth)
    } else if let Some(modal) = &state.env.modal {
        settings_env_modal_footer_items(modal)
    } else if let Some(modal) = &state.mounts.modal {
        settings_mounts_modal_footer_items(modal)
    } else {
        footer_items(state, op_available, body_area)
    }
}

fn footer_items(
    state: &SettingsState<'_>,
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

    let row_items = contextual_row_items(state, op_available, body_area);
    content_footer_items(
        settings_save_footer_label(),
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}

fn contextual_row_items(
    state: &SettingsState<'_>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    settings_contextual_row_footer_items(
        settings_context_footer_mode(state, body_area),
        op_available,
    )
}

fn settings_context_footer_mode(
    state: &SettingsState<'_>,
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
            let rows = settings_env_flat_rows(state);
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
        SettingsTab::Auth => {
            if state.auth.selected_kind.is_none() {
                SettingsContextFooterMode::AuthManage
            } else if selected_settings_auth_row_is_focusable(state) {
                SettingsContextFooterMode::AuthEditMode
            } else {
                SettingsContextFooterMode::Empty
            }
        }
        SettingsTab::Trust => SettingsContextFooterMode::Trust {
            has_roles: !state.trust.pending.is_empty(),
            scroll_axes: trust_scroll_axes(state, body_area),
        },
    }
}

fn selected_settings_auth_row_is_focusable(state: &SettingsState<'_>) -> bool {
    let Some(kind) = state.auth.selected_kind else {
        return true;
    };
    let Some(row) = state.auth.pending.iter().find(|row| row.kind == kind) else {
        return false;
    };
    jackin_console::tui::screens::settings::update::settings_auth_detail_rows(kind, row.mode)
        .get(state.auth.selected)
        .copied()
        .is_some_and(jackin_console::tui::screens::settings::update::settings_auth_row_is_focusable)
}

fn trust_scroll_axes(state: &SettingsState<'_>, body_area: Rect) -> ScrollAxes {
    if state.trust.pending.is_empty() {
        return ScrollAxes::none();
    }
    let viewport = jackin_tui::components::scrollable_panel::viewport_width(body_area);
    let content = jackin_console::tui::screens::settings::update::trust_content_width(&state.trust);
    ScrollAxes {
        horizontal: jackin_tui::components::scrollable_panel::is_scrollable(content, viewport),
        vertical: false,
    }
}

fn global_mount_scroll_axes(state: &SettingsState<'_>, body_area: Rect) -> ScrollAxes {
    if state.mounts.pending.is_empty() {
        return ScrollAxes::none();
    }
    let content_width =
        jackin_console::tui::mount_display::settings_global_config_mounts_content_width_with_cache(
            &state.mounts.pending,
            &state.mounts.mount_info_cache,
        );
    let viewport = jackin_tui::components::scrollable_panel::viewport_width(body_area);
    ScrollAxes {
        horizontal: jackin_tui::components::scrollable_panel::is_scrollable(
            content_width,
            viewport,
        ),
        vertical: false,
    }
}

fn settings_env_value<'a>(
    state: &'a SettingsState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a EnvValue> {
    match scope {
        SettingsEnvScope::Global => state.env.pending.env.get(key),
        SettingsEnvScope::Role(role) => state
            .env
            .pending
            .roles
            .get(role)
            .and_then(|env| env.get(key)),
    }
}

fn settings_env_value_is_op_ref(
    state: &SettingsState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> bool {
    settings_env_value(state, scope, key).is_some_and(|value| matches!(value, EnvValue::OpRef(_)))
}
