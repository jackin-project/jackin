//! Footer hint items for the workspace editor screen.

use std::cmp::Ordering;

use jackin_tui::HintSpan;

use crate::config::AppConfig;
use crate::console::manager::modal_footer::modal_footer_items;
use crate::console::manager::state::auth_flat_rows;
use crate::console::manager::state::{
    AuthRow, EditorState, EditorTab, FieldFocus, Modal, SecretsRow,
};
use crate::operator_env::EnvValue;

pub(crate) fn editor_footer_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    if let Some(modal) = &state.modal {
        let mut items = modal_footer_items(modal);
        if matches!(modal, Modal::AuthForm { .. })
            && crate::console::manager::input::auth::auth_form_can_generate_token(state)
        {
            items.extend([
                HintSpan::GroupSep,
                HintSpan::Key("G"),
                HintSpan::Text("generate"),
            ]);
        }
        return items;
    }
    if state.tab_bar_focused {
        let enter_content_hint = if state.active_tab == EditorTab::General {
            &[][..]
        } else {
            &[
                HintSpan::GroupSep,
                HintSpan::Key("⇥/↓"),
                HintSpan::Text("enter content"),
            ][..]
        };
        let mut items = vec![
            HintSpan::Key("\u{2190}\u{2192}"),
            HintSpan::Text("switch tab"),
        ];
        items.extend_from_slice(enter_content_hint);
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("S"),
            HintSpan::Text("save workspace"),
        ]);
        if state.is_dirty() {
            items.push(HintSpan::Dyn(format!("({} changes)", state.change_count())));
        }
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            if state.is_dirty() {
                HintSpan::Text("discard")
            } else {
                HintSpan::Text("back")
            },
        ]);
        return items;
    }
    let mut items: Vec<HintSpan<'static>> = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
    ];
    let row_items = contextual_row_items(state, config, op_available);
    if !row_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(row_items);
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("⇧Tab"),
        HintSpan::Text("tab bar"),
        HintSpan::GroupSep,
        HintSpan::Key("S"),
        HintSpan::Text("save workspace"),
    ]);
    if state.is_dirty() {
        items.push(HintSpan::Dyn(format!("({} changes)", state.change_count())));
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        if state.is_dirty() {
            HintSpan::Text("discard")
        } else {
            HintSpan::Text("back")
        },
    ]);
    items
}

#[allow(clippy::too_many_lines)]
pub(crate) fn contextual_row_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => match cursor {
            0 => vec![HintSpan::Key("↵"), HintSpan::Text("rename")],
            1 if !state.pending.mounts.is_empty() => {
                vec![HintSpan::Key("↵"), HintSpan::Text("pick working directory")]
            }
            2 | 3 => vec![HintSpan::Key("␣"), HintSpan::Text("toggle")],
            _ => Vec::new(),
        },
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            match cursor.cmp(&mount_count) {
                Ordering::Less => {
                    let mut items = vec![
                        HintSpan::Key("D"),
                        HintSpan::Text("remove"),
                        HintSpan::Sep,
                        HintSpan::Key("A"),
                        HintSpan::Text("add"),
                    ];
                    if state
                        .pending
                        .mounts
                        .get(cursor)
                        .and_then(|m| state.mount_info_cache.github_web_url(&m.src))
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
                        HintSpan::Key("I"),
                        HintSpan::Text("cycle isolation"),
                        HintSpan::Sep,
                        HintSpan::Key("H/L"),
                        HintSpan::Text("scroll"),
                    ]);
                    items
                }
                Ordering::Equal => vec![HintSpan::Key("↵/A"), HintSpan::Text("add")],
                Ordering::Greater => Vec::new(),
            }
        }
        EditorTab::Roles => {
            if cursor < config.roles.len() {
                vec![
                    HintSpan::Key("␣"),
                    HintSpan::Text("allow/disallow"),
                    HintSpan::Sep,
                    HintSpan::Key("*"),
                    HintSpan::Text("set/unset default"),
                    HintSpan::Sep,
                    HintSpan::Key("A"),
                    HintSpan::Text("load role"),
                ]
            } else {
                vec![HintSpan::Key("↵/A"), HintSpan::Text("load role")]
            }
        }
        EditorTab::Secrets => {
            let rows = secrets_flat_rows(state);
            let focused_value_is_op_ref = match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(key)) => state
                    .pending
                    .env
                    .get(key)
                    .is_some_and(|v| matches!(v, EnvValue::OpRef(_))),
                Some(SecretsRow::RoleKeyRow { role, key }) => state
                    .pending
                    .roles
                    .get(role)
                    .and_then(|ov| ov.env.get(key))
                    .is_some_and(|v| matches!(v, EnvValue::OpRef(_))),
                _ => false,
            };
            match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. })
                    if focused_value_is_op_ref =>
                {
                    let mut items = if op_available {
                        vec![
                            HintSpan::Key("↵"),
                            HintSpan::Sep,
                            HintSpan::Key("P"),
                            HintSpan::Text("re-pick from 1Password"),
                            HintSpan::Sep,
                        ]
                    } else {
                        Vec::new()
                    };
                    items.extend([
                        HintSpan::Key("D"),
                        HintSpan::Text("delete"),
                        HintSpan::Sep,
                        HintSpan::Key("A"),
                        HintSpan::Text("add"),
                    ]);
                    items
                }
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. }) => {
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
                        items.extend([
                            HintSpan::Sep,
                            HintSpan::Key("P"),
                            HintSpan::Text("1Password"),
                        ]);
                    }
                    items
                }
                Some(SecretsRow::RoleHeader { .. }) => vec![
                    HintSpan::Key("↵"),
                    HintSpan::Text("expand"),
                    HintSpan::Sep,
                    HintSpan::Key("←/→"),
                    HintSpan::Text("collapse/expand"),
                    HintSpan::Sep,
                    HintSpan::Key("A"),
                    HintSpan::Text("add"),
                ],
                Some(SecretsRow::WorkspaceAddSentinel | SecretsRow::RoleAddSentinel(_)) => {
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
                Some(SecretsRow::SectionSpacer) | None => vec![],
            }
        }
        EditorTab::Auth => {
            let flat = auth_flat_rows(state, config);
            match flat.get(cursor) {
                Some(AuthRow::AuthKindRow { .. }) => {
                    vec![HintSpan::Key("↵"), HintSpan::Text("manage auth")]
                }
                Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                    vec![HintSpan::Key("↵"), HintSpan::Text("edit mode")]
                }
                Some(AuthRow::RoleHeader { .. }) => vec![
                    HintSpan::Key("↵"),
                    HintSpan::Text("expand"),
                    HintSpan::Sep,
                    HintSpan::Key("←/→"),
                    HintSpan::Text("collapse/expand"),
                    HintSpan::Sep,
                    HintSpan::Key("D"),
                    HintSpan::Text("reset"),
                ],
                Some(AuthRow::AddSentinel { .. }) => {
                    vec![HintSpan::Key("↵/A"), HintSpan::Text("add override")]
                }
                Some(AuthRow::WorkspaceSource { .. } | AuthRow::RoleSource { .. }) => {
                    vec![HintSpan::Key("↵"), HintSpan::Text("edit source")]
                }
                Some(AuthRow::Spacer) | None => Vec::new(),
            }
        }
    }
}

fn secrets_flat_rows(editor: &EditorState<'_>) -> Vec<SecretsRow> {
    jackin_console::editor::update::secrets_flat_rows(
        &editor.pending.env,
        &editor.pending.roles,
        &editor.secrets_expanded,
        |role| &role.env,
    )
}
