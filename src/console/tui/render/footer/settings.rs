//! Footer hint items for the settings screen.

use jackin_tui::HintSpan;

use crate::console::tui::render::footer::modal::{
    settings_auth_modal_footer_items, settings_env_modal_footer_items,
    settings_mounts_modal_footer_items,
};
use crate::console::tui::state::{
    SettingsEnvRow, SettingsEnvScope, SettingsState, SettingsTab,
};
use crate::operator_env::EnvValue;
use jackin_console::tui::components::footer_hints::{
    content_footer_items, global_mount_row_footer_items, secret_add_row_footer_items,
    secret_op_ref_row_footer_items, secret_plain_row_footer_items, secret_role_header_footer_items,
    tab_bar_footer_items,
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
            "save settings",
            true,
            state.is_dirty().then(|| state.change_count()),
        );
    }

    let row_items = contextual_row_items(state, op_available);
    content_footer_items(
        "save settings",
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}

#[allow(clippy::too_many_lines)]
fn contextual_row_items(state: &SettingsState<'_>, op_available: bool) -> Vec<HintSpan<'static>> {
    match state.active_tab {
        SettingsTab::General => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::Sep,
            HintSpan::Key("␣"),
            HintSpan::Text("toggle"),
        ],
        SettingsTab::Mounts => {
            let cursor = state.mounts.selected;
            let mount_count = state.mounts.pending.len();
            if cursor == mount_count {
                vec![HintSpan::Key("↵/A"), HintSpan::Text("add")]
            } else {
                global_mount_row_footer_items(
                    state
                        .mounts
                        .pending
                        .get(cursor)
                        .and_then(|row| {
                            state.mounts.mount_info_cache.github_web_url(&row.mount.src)
                        })
                        .is_some(),
                )
            }
        }
        SettingsTab::Environments => {
            let rows = settings_env_flat_rows(state);
            match rows.get(state.env.selected) {
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_env_value_is_op_ref(state, scope, key) =>
                {
                    secret_op_ref_row_footer_items(op_available)
                }
                Some(SettingsEnvRow::Key { .. }) => {
                    secret_plain_row_footer_items(op_available)
                }
                Some(SettingsEnvRow::RoleHeader { .. }) => secret_role_header_footer_items(),
                Some(SettingsEnvRow::GlobalAddSentinel | SettingsEnvRow::RoleAddSentinel(_)) => {
                    secret_add_row_footer_items(op_available)
                }
                Some(SettingsEnvRow::SectionSpacer) | None => Vec::new(),
            }
        }
        SettingsTab::Auth => {
            if state.auth.selected_kind.is_none() {
                vec![HintSpan::Key("↵"), HintSpan::Text("manage auth")]
            } else if state.auth.selected == 0 {
                vec![HintSpan::Key("↵"), HintSpan::Text("edit mode")]
            } else {
                vec![HintSpan::Key("↵"), HintSpan::Text("edit source")]
            }
        }
        SettingsTab::Trust => {
            if state.trust.pending.is_empty() {
                Vec::new()
            } else {
                vec![
                    HintSpan::Key("␣"),
                    HintSpan::Text("trust/untrust"),
                    HintSpan::Sep,
                    HintSpan::Key("H/L"),
                    HintSpan::Text("scroll"),
                ]
            }
        }
    }
}

fn settings_env_flat_rows(state: &SettingsState<'_>) -> Vec<SettingsEnvRow> {
    jackin_console::tui::screens::settings::update::settings_env_flat_rows(
        &state.env.pending,
        &state.env.expanded,
    )
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
