//! Footer hint items for the settings screen.

use jackin_tui::HintSpan;

use crate::console::manager::modal_footer::{
    settings_auth_modal_footer_items, settings_env_modal_footer_items,
    settings_mounts_modal_footer_items,
};
use crate::console::manager::state::{
    SettingsEnvRow, SettingsEnvScope, SettingsState, SettingsTab,
};
use crate::operator_env::EnvValue;

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
        let mut items = vec![
            HintSpan::Key("\u{2190}\u{2192}"),
            HintSpan::Text("switch tab"),
            HintSpan::GroupSep,
            HintSpan::Key("⇥/↓"),
            HintSpan::Text("enter content"),
        ];
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("S"),
            HintSpan::Text("save settings"),
        ]);
        if state.is_dirty() {
            items.push(HintSpan::Dyn(format!("({} changes)", state.change_count())));
        }
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text(if state.is_dirty() { "discard" } else { "back" }),
        ]);
        return items;
    }

    let mut items = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
    ];

    let row_items = contextual_row_items(state, op_available);
    if !row_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(row_items);
    }

    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("⇧Tab"),
        HintSpan::Text("tab bar"),
        HintSpan::GroupSep,
    ]);
    items.extend([HintSpan::Key("S"), HintSpan::Text("save settings")]);
    if state.is_dirty() {
        items.push(HintSpan::Dyn(format!("({} changes)", state.change_count())));
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text(if state.is_dirty() { "discard" } else { "back" }),
    ]);
    items
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
                let mut items = vec![
                    HintSpan::Key("D"),
                    HintSpan::Text("remove"),
                    HintSpan::Sep,
                    HintSpan::Key("A"),
                    HintSpan::Text("add"),
                ];
                if state
                    .mounts
                    .pending
                    .get(cursor)
                    .and_then(|row| state.mounts.mount_info_cache.github_web_url(&row.mount.src))
                    .is_some()
                {
                    items.push(HintSpan::Sep);
                    items.push(HintSpan::Key("O"));
                    items.push(HintSpan::Text("open in GitHub"));
                }
                items.extend([
                    HintSpan::Sep,
                    HintSpan::Key("R"),
                    HintSpan::Text("toggle ro/rw"),
                    HintSpan::Sep,
                    HintSpan::Key("N"),
                    HintSpan::Text("rename"),
                    HintSpan::Sep,
                    HintSpan::Key("1"),
                    HintSpan::Text("edit source"),
                    HintSpan::Sep,
                    HintSpan::Key("2"),
                    HintSpan::Text("edit dst"),
                    HintSpan::Sep,
                    HintSpan::Key("3"),
                    HintSpan::Text("edit scope"),
                    HintSpan::Sep,
                    HintSpan::Key("H/L"),
                    HintSpan::Text("scroll"),
                ]);
                items
            }
        }
        SettingsTab::Environments => {
            let rows = settings_env_flat_rows(state);
            match rows.get(state.env.selected) {
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_env_value_is_op_ref(state, scope, key) =>
                {
                    let mut items = vec![
                        HintSpan::Key("↵"),
                        HintSpan::Sep,
                        HintSpan::Key("P"),
                        HintSpan::Text("re-pick from 1Password"),
                        HintSpan::Sep,
                        HintSpan::Key("D"),
                        HintSpan::Text("delete"),
                        HintSpan::Sep,
                        HintSpan::Key("A"),
                        HintSpan::Text("add"),
                    ];
                    if !op_available {
                        items.drain(..4);
                    }
                    items
                }
                Some(SettingsEnvRow::Key { .. }) => {
                    let mut items = vec![
                        HintSpan::Key("↵"),
                        HintSpan::Text("edit"),
                        HintSpan::Sep,
                        HintSpan::Key("D"),
                        HintSpan::Text("delete"),
                        HintSpan::Sep,
                        HintSpan::Key("A"),
                        HintSpan::Text("add"),
                        HintSpan::Sep,
                        HintSpan::Key("M"),
                        HintSpan::Text("mask/unmask"),
                    ];
                    if op_available {
                        items.push(HintSpan::Sep);
                        items.push(HintSpan::Key("P"));
                        items.push(HintSpan::Text("1Password"));
                    }
                    items
                }
                Some(SettingsEnvRow::RoleHeader { .. }) => vec![
                    HintSpan::Key("↵"),
                    HintSpan::Text("expand"),
                    HintSpan::Sep,
                    HintSpan::Key("←/→"),
                    HintSpan::Text("collapse/expand"),
                    HintSpan::Sep,
                    HintSpan::Key("A"),
                    HintSpan::Text("add"),
                ],
                Some(SettingsEnvRow::GlobalAddSentinel | SettingsEnvRow::RoleAddSentinel(_)) => {
                    let mut items = vec![HintSpan::Key("↵"), HintSpan::Text("add")];
                    if op_available {
                        items.extend([
                            HintSpan::Sep,
                            HintSpan::Key("P"),
                            HintSpan::Text("1Password"),
                        ]);
                    }
                    items
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
    jackin_console::settings::update::settings_env_flat_rows(
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
