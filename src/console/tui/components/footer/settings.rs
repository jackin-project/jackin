//! Footer hint items for the settings screen.

use jackin_tui::HintSpan;

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

pub(crate) fn settings_footer_items(
    state: &SettingsState<'_>,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    if state.auth.modal.is_some() {
        settings_auth_modal_footer_items(&state.auth)
    } else if let Some(modal) = &state.env.modal {
        settings_env_modal_footer_items(modal)
    } else if let Some(modal) = &state.mounts.modal {
        settings_mounts_modal_footer_items(modal)
    } else {
        footer_items(state, op_available)
    }
}

fn footer_items(state: &SettingsState<'_>, op_available: bool) -> Vec<HintSpan<'static>> {
    if state.tab_bar_focused {
        return tab_bar_footer_items(
            settings_save_footer_label(),
            true,
            state.is_dirty().then(|| state.change_count()),
        );
    }

    let row_items = contextual_row_items(state, op_available);
    content_footer_items(
        settings_save_footer_label(),
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}

#[allow(clippy::too_many_lines)]
fn contextual_row_items(state: &SettingsState<'_>, op_available: bool) -> Vec<HintSpan<'static>> {
    settings_contextual_row_footer_items(settings_context_footer_mode(state), op_available)
}

fn settings_context_footer_mode(state: &SettingsState<'_>) -> SettingsContextFooterMode {
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
                        .and_then(|row| state.mounts.mount_info_cache.github_web_url(&row.mount.src))
                        .is_some(),
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
            } else if state.auth.selected == 0 {
                SettingsContextFooterMode::AuthEditMode
            } else {
                SettingsContextFooterMode::AuthEditSource
            }
        }
        SettingsTab::Trust => SettingsContextFooterMode::Trust {
            has_roles: !state.trust.pending.is_empty(),
        },
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
