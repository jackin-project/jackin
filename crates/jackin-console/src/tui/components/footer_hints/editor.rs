//! Editor-screen footer facts + hint-span builders for the editor's
//! per-row contextual footer (general / mount / role / secret / auth).

use jackin_tui::HintSpan;
use ratatui::layout::Rect;

use crate::tui::keymap::{
    AUTH_EDIT_SOURCE_KEYMAP, AUTH_MANAGE_KEYMAP, EDITOR_GENERAL_RENAME_KEYMAP,
    EDITOR_GENERAL_TOGGLE_KEYMAP, EDITOR_GENERAL_WORKDIR_KEYMAP, EDITOR_ROLE_NEW_KEYMAP,
};
use jackin_tui::components::ScrollAxes;

use super::settings::{
    add_row_footer_items, secret_add_row_footer_items, secret_op_ref_row_footer_items,
    secret_plain_row_footer_items, secret_role_header_footer_items,
    workspace_mount_row_footer_items,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorScreenFooterFacts {
    Modal {
        items: Vec<HintSpan<'static>>,
    },
    TabBar {
        save_label: &'static str,
        enter_content: bool,
        dirty_change_count: Option<usize>,
    },
    Content {
        save_label: &'static str,
        row_items: Vec<HintSpan<'static>>,
        dirty_change_count: Option<usize>,
    },
}

#[must_use]
pub fn editor_screen_footer_items(facts: EditorScreenFooterFacts) -> Vec<HintSpan<'static>> {
    match facts {
        EditorScreenFooterFacts::Modal { items } => items,
        EditorScreenFooterFacts::TabBar {
            save_label,
            enter_content,
            dirty_change_count,
        } => super::common::tab_bar_footer_items(save_label, enter_content, dirty_change_count),
        EditorScreenFooterFacts::Content {
            save_label,
            row_items,
            dirty_change_count,
        } => super::common::content_footer_items(save_label, row_items, dirty_change_count),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsScreenFooterFacts {
    pub auth_modal_items: Option<Vec<HintSpan<'static>>>,
    pub env_modal_items: Option<Vec<HintSpan<'static>>>,
    pub mounts_modal_items: Option<Vec<HintSpan<'static>>>,
    pub screen_items: Vec<HintSpan<'static>>,
}

#[must_use]
pub fn settings_screen_footer_items(facts: SettingsScreenFooterFacts) -> Vec<HintSpan<'static>> {
    if let Some(items) = facts.auth_modal_items {
        return items;
    }
    if let Some(items) = facts.env_modal_items {
        return items;
    }
    if let Some(items) = facts.mounts_modal_items {
        return items;
    }
    facts.screen_items
}

#[must_use]
pub fn editor_footer_items(
    state: &crate::tui::state::EditorState<'_>,
    config: &jackin_config::AppConfig,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    if let Some(modal) = &state.modal {
        return editor_screen_footer_items(EditorScreenFooterFacts::Modal {
            items: modal.footer_items(state.auth_form_can_generate_token()),
        });
    }
    if state.tab_bar_focused() {
        return editor_screen_footer_items(EditorScreenFooterFacts::TabBar {
            save_label: super::workspace::editor_save_footer_label(),
            enter_content: state.active_tab != crate::tui::state::EditorTab::General,
            dirty_change_count: state.is_dirty().then(|| state.change_count()),
        });
    }
    let row_items = crate::tui::screens::editor::view::editor_contextual_footer_items(
        state,
        config,
        op_available,
        body_area,
    );
    editor_screen_footer_items(EditorScreenFooterFacts::Content {
        save_label: super::workspace::editor_save_footer_label(),
        row_items,
        dirty_change_count: state.is_dirty().then(|| state.change_count()),
    })
}

#[must_use]
pub fn editor_general_row_footer_items(row: usize, has_mounts: bool) -> Vec<HintSpan<'static>> {
    match row {
        0 => EDITOR_GENERAL_RENAME_KEYMAP.hint_spans(),
        1 if has_mounts => EDITOR_GENERAL_WORKDIR_KEYMAP.hint_spans(),
        2 | 3 => EDITOR_GENERAL_TOGGLE_KEYMAP.hint_spans(),
        _ => Vec::new(),
    }
}

#[must_use]
pub fn editor_role_row_footer_items(is_existing_role: bool) -> Vec<HintSpan<'static>> {
    if is_existing_role {
        vec![
            // UNREGISTERABLE(editor-role-existing-no-keymap): Space toggles allow/disallow inline; no EDITOR_ROLE_EXISTING_KEYMAP.
            HintSpan::Key("␣"),
            HintSpan::Text("allow/disallow"),
            HintSpan::Sep,
            // UNREGISTERABLE(editor-role-existing-no-keymap): asterisk sets default role inline; no EDITOR_ROLE_EXISTING_KEYMAP.
            HintSpan::Key("*"),
            HintSpan::Text("set/unset default"),
            HintSpan::Sep,
            // UNREGISTERABLE(editor-role-existing-no-keymap): A loads role inline; no EDITOR_ROLE_EXISTING_KEYMAP.
            HintSpan::Key("A"),
            HintSpan::Text("load role"),
        ]
    } else {
        EDITOR_ROLE_NEW_KEYMAP.hint_spans()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorContextFooterMode {
    General {
        row: usize,
        has_mounts: bool,
    },
    MountRow {
        has_github_url: bool,
        scroll_axes: ScrollAxes,
    },
    MountAddRow,
    RoleRow {
        is_existing_role: bool,
    },
    SecretOpRefRow,
    SecretPlainRow,
    SecretRoleHeader,
    SecretAddRow,
    AuthManage,
    AuthEditMode,
    AuthRoleHeader,
    AuthAddOverride,
    AuthEditSource,
    Empty,
}

#[must_use]
pub fn editor_contextual_row_footer_items(
    mode: EditorContextFooterMode,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    match mode {
        EditorContextFooterMode::General { row, has_mounts } => {
            editor_general_row_footer_items(row, has_mounts)
        }
        EditorContextFooterMode::MountRow {
            has_github_url,
            scroll_axes,
        } => workspace_mount_row_footer_items(has_github_url, scroll_axes),
        EditorContextFooterMode::MountAddRow => add_row_footer_items("add"),
        EditorContextFooterMode::RoleRow { is_existing_role } => {
            editor_role_row_footer_items(is_existing_role)
        }
        EditorContextFooterMode::SecretOpRefRow => secret_op_ref_row_footer_items(op_available),
        EditorContextFooterMode::SecretPlainRow => secret_plain_row_footer_items(op_available),
        EditorContextFooterMode::SecretRoleHeader => secret_role_header_footer_items(),
        EditorContextFooterMode::SecretAddRow => secret_add_row_footer_items(op_available),
        EditorContextFooterMode::AuthManage => auth_row_footer_items(AuthRowFooterMode::ManageAuth),
        EditorContextFooterMode::AuthEditMode => auth_row_footer_items(AuthRowFooterMode::EditMode),
        EditorContextFooterMode::AuthRoleHeader => {
            auth_row_footer_items(AuthRowFooterMode::RoleHeader)
        }
        EditorContextFooterMode::AuthAddOverride => add_row_footer_items("add override"),
        EditorContextFooterMode::AuthEditSource => {
            auth_row_footer_items(AuthRowFooterMode::EditSource)
        }
        EditorContextFooterMode::Empty => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRowFooterMode {
    ManageAuth,
    EditMode,
    RoleHeader,
    EditSource,
    Empty,
}

#[must_use]
pub fn auth_row_footer_items(mode: AuthRowFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        AuthRowFooterMode::ManageAuth => AUTH_MANAGE_KEYMAP.hint_spans(),
        AuthRowFooterMode::EditMode => vec![
            // UNREGISTERABLE(auth-edit-mode-no-keymap): handled inline; no dedicated auth-edit-mode or role-header keymap.
            HintSpan::Key("↵"),
            HintSpan::Text("edit mode"),
            HintSpan::Sep,
            // UNREGISTERABLE(auth-edit-mode-no-keymap): handled inline; no dedicated auth-edit-mode or role-header keymap.
            HintSpan::Key("D"),
            HintSpan::Text("reset"),
        ],
        AuthRowFooterMode::RoleHeader => vec![
            // UNREGISTERABLE(auth-edit-mode-no-keymap): handled inline; no dedicated auth-edit-mode or role-header keymap.
            HintSpan::Key("↵"),
            HintSpan::Text("expand"),
            HintSpan::Sep,
            // UNREGISTERABLE(auth-edit-mode-no-keymap): handled inline; no dedicated auth-edit-mode or role-header keymap.
            HintSpan::Key("←/→"),
            HintSpan::Text("collapse/expand"),
            HintSpan::Sep,
            // UNREGISTERABLE(auth-edit-mode-no-keymap): handled inline; no dedicated auth-edit-mode or role-header keymap.
            HintSpan::Key("D"),
            HintSpan::Text("reset"),
        ],
        AuthRowFooterMode::EditSource => AUTH_EDIT_SOURCE_KEYMAP.hint_spans(),
        AuthRowFooterMode::Empty => Vec::new(),
    }
}
