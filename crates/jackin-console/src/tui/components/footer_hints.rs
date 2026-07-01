//! Shared footer hint fragments for modal pickers and confirmations.

use crate::tui::keymap::{
    AUTH_EDIT_SOURCE_KEYMAP, AUTH_MANAGE_KEYMAP, EDITOR_CONTENT_KEYMAP,
    EDITOR_GENERAL_RENAME_KEYMAP, EDITOR_GENERAL_TOGGLE_KEYMAP, EDITOR_GENERAL_WORKDIR_KEYMAP,
    EDITOR_GLOBAL_KEYMAP, EDITOR_ROLE_NEW_KEYMAP, EDITOR_TAB_BAR_KEYMAP, EditorContentAction,
    EditorGlobalAction, EditorTabBarAction, PREVIEW_PANE_KEYMAP, PreviewPaneAction,
    SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TOGGLE_KEYMAP, SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP,
    SETTINGS_TRUST_TOGGLE_KEYMAP, SettingsEnvTabAction, SettingsGlobalMountsTabAction,
    WORKSPACE_LIST_KEYMAP, WorkspaceListAction,
};
use jackin_tui::HintSpan;
use jackin_tui::components::{
    ScrollAxes, error_popup_hint_spans, save_discard_hint_spans, scroll_hint_spans,
};
use ratatui::layout::Rect;

use crate::tui::components::auth_panel;
use crate::tui::components::confirm_save;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::op_picker::OpPickerRenderState;
use crate::tui::components::op_picker::OpPickerStage;
use crate::tui::model::ConsoleManagerStageRoute;
use crate::tui::screens::settings::model::AuthFormFocus;
use crate::tui::screens::workspaces::model::ManagerListRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListFooterMode {
    AgentPicker {
        scroll_axes: ScrollAxes,
    },
    RolePicker {
        scroll_axes: ScrollAxes,
    },
    PreviewPane,
    InstanceRow {
        has_snapshot: bool,
        is_live: bool,
    },
    WorkspaceRow {
        scroll_axes: ScrollAxes,
        enter_label: &'static str,
        is_saved: bool,
        show_prewarm: bool,
        show_expand: bool,
        show_collapse: bool,
        show_open_in_github: bool,
    },
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Twelve orthogonal footer-state flags (inline-agent/role-picker, \
              selected row / preview focus, snapshot+live markers, saved vs new \
              workspace, show prewarm/expand/collapse/github) — each tracks an \
              independent UI hint visibility consumed individually by the footer \
              item builder. Named-field reads match the per-hint gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterFacts {
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub selected_instance: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_instance_is_live: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
    pub show_prewarm: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Eight orthogonal footer-input flags (selected row, inline-agent/role \
              pickers, preview focus, snapshot+live markers, show_expand/collapse, \
              scroll axes, open-in-github) — each is an independent input the \
              workspace-list-footer mode resolver reads individually. Named-field \
              reads match the per-input gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterInputFacts {
    pub selected_row: ManagerListRow,
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_instance_is_live: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterRowFacts {
    pub selected_instance: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
}

#[must_use]
pub const fn workspace_list_footer_row_facts(row: ManagerListRow) -> WorkspaceListFooterRowFacts {
    match row {
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_) => {
            WorkspaceListFooterRowFacts {
                selected_instance: true,
                selected_saved_workspace: false,
                selected_new_workspace: false,
            }
        }
        ManagerListRow::SavedWorkspace(_) => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: true,
            selected_new_workspace: false,
        },
        ManagerListRow::NewWorkspace => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: true,
        },
        ManagerListRow::CurrentDirectory => WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: false,
        },
    }
}

#[must_use]
pub const fn workspace_list_open_github_visible(
    row: ManagerListRow,
    selected_workspace_has_github_mounts: bool,
) -> bool {
    matches!(row, ManagerListRow::SavedWorkspace(_)) && selected_workspace_has_github_mounts
}

#[must_use]
pub const fn workspace_list_footer_facts(
    facts: WorkspaceListFooterInputFacts,
) -> WorkspaceListFooterFacts {
    let row_facts = workspace_list_footer_row_facts(facts.selected_row);
    WorkspaceListFooterFacts {
        inline_agent_picker: facts.inline_agent_picker,
        inline_role_picker: facts.inline_role_picker,
        selected_instance: row_facts.selected_instance,
        preview_focused: facts.preview_focused,
        selected_instance_has_snapshot: facts.selected_instance_has_snapshot,
        selected_instance_is_live: facts.selected_instance_is_live,
        selected_saved_workspace: row_facts.selected_saved_workspace,
        selected_new_workspace: row_facts.selected_new_workspace,
        // Surface the `W` prewarm hint exactly when a saved workspace is
        // selected — the only row for which `W` dispatches PrewarmNamed.
        show_prewarm: row_facts.selected_saved_workspace,
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        workspace_scroll_axes: facts.workspace_scroll_axes,
        show_open_in_github: facts.show_open_in_github,
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Five orthogonal scroll-axis input flags (inline-agent/role pickers, \
              list-names focus, scroll axes per pane, show_expand) — each tracks an \
              independent scrollable-pane state consumed individually by the scroll \
              axes planner. Named-field reads match the per-pane gating idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFooterScrollFacts {
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub inline_picker_scroll_axes: ScrollAxes,
    pub focused_block_scroll_axes: Option<ScrollAxes>,
    pub list_names_focused: bool,
    pub list_names_scroll_axes: ScrollAxes,
    pub show_expand: bool,
    pub show_collapse: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceInlinePickerContentFacts {
    pub agent_picker_count: Option<usize>,
    pub role_picker_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceScreenFooterFacts {
    List {
        list_items: Vec<HintSpan<'static>>,
        modal_items: Option<Vec<HintSpan<'static>>>,
    },
    CreatePrelude {
        modal_items: Option<Vec<HintSpan<'static>>>,
    },
    DestructiveConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceScreenFooterPlan {
    List,
    CreatePrelude,
    DestructiveConfirm,
    ScreenOwned,
}

#[must_use]
pub const fn workspace_screen_footer_plan(
    route: ConsoleManagerStageRoute,
) -> WorkspaceScreenFooterPlan {
    match route {
        ConsoleManagerStageRoute::List => WorkspaceScreenFooterPlan::List,
        ConsoleManagerStageRoute::CreatePrelude => WorkspaceScreenFooterPlan::CreatePrelude,
        ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => {
            WorkspaceScreenFooterPlan::DestructiveConfirm
        }
        ConsoleManagerStageRoute::Editor | ConsoleManagerStageRoute::Settings => {
            WorkspaceScreenFooterPlan::ScreenOwned
        }
    }
}

#[must_use]
pub fn workspace_screen_footer_items(facts: WorkspaceScreenFooterFacts) -> Vec<HintSpan<'static>> {
    match facts {
        WorkspaceScreenFooterFacts::List {
            list_items,
            modal_items,
        } => modal_items.unwrap_or(list_items),
        WorkspaceScreenFooterFacts::CreatePrelude { modal_items } => {
            modal_items.unwrap_or_else(create_prelude_footer_items)
        }
        WorkspaceScreenFooterFacts::DestructiveConfirm => destructive_confirm_footer_items(),
    }
}

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
        } => tab_bar_footer_items(save_label, enter_content, dirty_change_count),
        EditorScreenFooterFacts::Content {
            save_label,
            row_items,
            dirty_change_count,
        } => content_footer_items(save_label, row_items, dirty_change_count),
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
pub fn workspace_footer_scroll_axes(facts: WorkspaceFooterScrollFacts) -> ScrollAxes {
    if facts.inline_agent_picker || facts.inline_role_picker {
        return facts.inline_picker_scroll_axes;
    }
    if let Some(axes) = facts.focused_block_scroll_axes {
        return axes;
    }
    if facts.list_names_focused && !facts.show_expand && !facts.show_collapse {
        return facts.list_names_scroll_axes;
    }
    ScrollAxes::none()
}

#[must_use]
pub fn workspace_inline_picker_content_height(facts: WorkspaceInlinePickerContentFacts) -> usize {
    facts
        .agent_picker_count
        .or(facts.role_picker_count)
        .unwrap_or(0)
}

#[must_use]
pub fn workspace_list_footer_mode_for_facts(
    facts: WorkspaceListFooterFacts,
) -> WorkspaceListFooterMode {
    if facts.inline_agent_picker {
        return WorkspaceListFooterMode::AgentPicker {
            scroll_axes: facts.workspace_scroll_axes,
        };
    }
    if facts.inline_role_picker {
        return WorkspaceListFooterMode::RolePicker {
            scroll_axes: facts.workspace_scroll_axes,
        };
    }
    if facts.selected_instance {
        if facts.preview_focused {
            return WorkspaceListFooterMode::PreviewPane;
        }
        return WorkspaceListFooterMode::InstanceRow {
            has_snapshot: facts.selected_instance_has_snapshot,
            is_live: facts.selected_instance_is_live,
        };
    }
    WorkspaceListFooterMode::WorkspaceRow {
        scroll_axes: facts.workspace_scroll_axes,
        enter_label: if facts.selected_new_workspace {
            "setup"
        } else {
            "launch"
        },
        is_saved: facts.selected_saved_workspace,
        show_prewarm: facts.show_prewarm,
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        show_open_in_github: facts.show_open_in_github,
    }
}

#[must_use]
pub fn workspace_list_footer_items(mode: WorkspaceListFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        WorkspaceListFooterMode::AgentPicker { scroll_axes } => {
            workspace_picker_footer_items(scroll_axes, true)
        }
        WorkspaceListFooterMode::RolePicker { scroll_axes } => {
            workspace_picker_footer_items(scroll_axes, true)
        }
        WorkspaceListFooterMode::PreviewPane => {
            // Glyphs derive from PREVIEW_PANE_KEYMAP — the same table that drives
            // `preview_pane_key_plan` dispatch — so advertised keys cannot drift
            // from handled keys. BackTab (HiddenAlias) and the upstream Ctrl-Q
            // are intentionally not advertised.
            let g = |a| PREVIEW_PANE_KEYMAP.glyph_for(a);
            vec![
                HintSpan::Key(g(PreviewPaneAction::NavigateUp)),
                HintSpan::Text("navigate panes"),
                HintSpan::Sep,
                HintSpan::Key(g(PreviewPaneAction::Attach)),
                HintSpan::Text("attach focused pane"),
                HintSpan::GroupSep,
                HintSpan::Key(g(PreviewPaneAction::Back)),
                HintSpan::Text("back"),
            ]
        }
        WorkspaceListFooterMode::InstanceRow {
            has_snapshot,
            is_live,
        } => {
            // Glyphs derive from WORKSPACE_LIST_KEYMAP (the dispatch table);
            // labels are instance-row-specific and supplied here.
            let g = |a| WORKSPACE_LIST_KEYMAP.glyph_for(a);
            // A failed/stopped instance has no live daemon: new-session, shell,
            // and stop are meaningless. `Enter` enters the restore ladder
            // (docker start + reconnect, or recreate from image) — that is the
            // "restart" verb — so it is labelled accordingly (D15).
            let mut items = if is_live {
                vec![
                    HintSpan::Key(g(WorkspaceListAction::NavigateUp)),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::Enter)),
                    HintSpan::Text("reconnect"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::NewSession)),
                    HintSpan::Text("new session"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceShell)),
                    HintSpan::Text("shell"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceStop)),
                    HintSpan::Text("stop"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::ConfirmPurge)),
                    HintSpan::Text("purge"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceInspect)),
                    HintSpan::Text("info"),
                ]
            } else {
                vec![
                    HintSpan::Key(g(WorkspaceListAction::NavigateUp)),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::Enter)),
                    HintSpan::Text("restart"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::ConfirmPurge)),
                    HintSpan::Text("delete"),
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::InstanceInspect)),
                    HintSpan::Text("info"),
                ]
            };
            if has_snapshot {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::EnterPreview)));
                items.push(HintSpan::Text("into preview"));
            }
            items.extend([
                HintSpan::GroupSep,
                HintSpan::Key(g(WorkspaceListAction::TreeLeft)),
                HintSpan::Text("back"),
                HintSpan::GroupSep,
                HintSpan::Key(g(WorkspaceListAction::Quit)),
                HintSpan::Text("quit"),
            ]);
            items
        }
        WorkspaceListFooterMode::WorkspaceRow {
            scroll_axes,
            enter_label,
            is_saved,
            show_prewarm,
            show_expand,
            show_collapse,
            show_open_in_github,
        } => {
            // Glyphs derive from WORKSPACE_LIST_KEYMAP (the dispatch table);
            // labels and conditional composition are workspace-row-specific.
            let g = |a| WORKSPACE_LIST_KEYMAP.glyph_for(a);
            let mut items = Vec::new();
            if scroll_axes.any() {
                items.extend(scroll_hint_spans(scroll_axes));
                items.push(HintSpan::GroupSep);
            } else {
                items.push(HintSpan::Key(g(WorkspaceListAction::NavigateUp)));
                items.push(HintSpan::Sep);
            }
            items.extend([
                HintSpan::Key(g(WorkspaceListAction::Enter)),
                HintSpan::Text(enter_label),
                HintSpan::GroupSep,
            ]);
            if is_saved {
                items.extend([
                    HintSpan::Key(g(WorkspaceListAction::Edit)),
                    HintSpan::Text("edit"),
                    HintSpan::Sep,
                ]);
            }
            if show_prewarm {
                items.extend([
                    HintSpan::Key(g(WorkspaceListAction::Prewarm)),
                    HintSpan::Text("prewarm"),
                    HintSpan::Sep,
                ]);
            }
            items.extend([
                HintSpan::Key(g(WorkspaceListAction::NewSession)),
                HintSpan::Text("new"),
            ]);
            if is_saved {
                items.extend([
                    HintSpan::Sep,
                    HintSpan::Key(g(WorkspaceListAction::Delete)),
                    HintSpan::Text("delete"),
                ]);
            }
            items.extend([
                HintSpan::Sep,
                HintSpan::Key(g(WorkspaceListAction::Settings)),
                HintSpan::Text("settings"),
            ]);
            if show_expand {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::TreeRight)));
                items.push(HintSpan::Text("expand"));
            }
            if show_collapse {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::TreeLeft)));
                items.push(HintSpan::Text("collapse"));
            }
            if show_open_in_github {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key(g(WorkspaceListAction::OpenGithub)));
                items.push(HintSpan::Text("open in GitHub"));
            }
            items.push(HintSpan::GroupSep);
            items.push(HintSpan::Key(g(WorkspaceListAction::Quit)));
            items.push(HintSpan::Text("quit"));
            items
        }
    }
}

#[must_use]
pub fn selected_instance_snapshot_available(
    selected: ManagerListRow,
    workspace_has_snapshot: impl FnOnce(usize, usize) -> bool,
    current_dir_has_snapshot: impl FnOnce(usize) -> bool,
) -> bool {
    match selected {
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => {
            workspace_has_snapshot(ws_idx, inst_idx)
        }
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => current_dir_has_snapshot(inst_idx),
        ManagerListRow::CurrentDirectory
        | ManagerListRow::SavedWorkspace(_)
        | ManagerListRow::NewWorkspace => false,
    }
}

#[must_use]
pub const fn editor_save_footer_label() -> &'static str {
    "save workspace"
}

#[must_use]
pub const fn settings_save_footer_label() -> &'static str {
    "save settings"
}

#[must_use]
pub const fn pick_list_select_footer_label() -> &'static str {
    "select"
}

#[must_use]
pub const fn pick_list_confirm_footer_label() -> &'static str {
    "confirm"
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
            save_label: editor_save_footer_label(),
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
        save_label: editor_save_footer_label(),
        row_items,
        dirty_change_count: state.is_dirty().then(|| state.change_count()),
    })
}

#[must_use]
pub fn destructive_confirm_footer_items() -> Vec<HintSpan<'static>> {
    jackin_tui::components::confirm_hint_spans()
}

#[must_use]
pub fn create_prelude_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Dyn("Create workspace — follow the prompts".to_owned()),
        HintSpan::GroupSep,
        // UNREGISTERABLE(create-prelude-no-keymap): Esc handled inline; no dedicated create-prelude keymap.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
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

#[must_use]
pub fn settings_general_row_footer_items() -> Vec<HintSpan<'static>> {
    // `content_footer_items` already prepends ↑↓ navigate; only add the tab-specific action.
    SETTINGS_GENERAL_TOGGLE_KEYMAP.hint_spans()
}

#[must_use]
pub fn settings_trust_row_footer_items(
    has_roles: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    if has_roles {
        let mut items = SETTINGS_TRUST_TOGGLE_KEYMAP.hint_spans();
        let scroll_items = scroll_hint_spans(scroll_axes);
        if !scroll_items.is_empty() {
            items.push(HintSpan::Sep);
            items.extend(scroll_items);
        }
        items
    } else {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsContextFooterMode {
    General,
    MountRow {
        has_github_url: bool,
        scroll_axes: ScrollAxes,
    },
    MountAddRow,
    EnvOpRefRow,
    EnvPlainRow,
    EnvRoleHeader,
    EnvAddRow,
    Empty,
    AuthManage,
    AuthEditMode,
    AuthEditSource,
    Trust {
        has_roles: bool,
        scroll_axes: ScrollAxes,
    },
}

#[must_use]
pub fn settings_contextual_row_footer_items(
    mode: SettingsContextFooterMode,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    match mode {
        SettingsContextFooterMode::General => settings_general_row_footer_items(),
        SettingsContextFooterMode::MountRow {
            has_github_url,
            scroll_axes,
        } => global_mount_row_footer_items(has_github_url, scroll_axes),
        SettingsContextFooterMode::MountAddRow => add_row_footer_items("add"),
        SettingsContextFooterMode::EnvOpRefRow => secret_op_ref_row_footer_items(op_available),
        SettingsContextFooterMode::EnvPlainRow => secret_plain_row_footer_items(op_available),
        SettingsContextFooterMode::EnvRoleHeader => secret_role_header_footer_items(),
        SettingsContextFooterMode::EnvAddRow => secret_add_row_footer_items(op_available),
        SettingsContextFooterMode::Empty => Vec::new(),
        SettingsContextFooterMode::AuthManage => {
            auth_row_footer_items(AuthRowFooterMode::ManageAuth)
        }
        SettingsContextFooterMode::AuthEditMode => {
            auth_row_footer_items(AuthRowFooterMode::EditMode)
        }
        SettingsContextFooterMode::AuthEditSource => {
            auth_row_footer_items(AuthRowFooterMode::EditSource)
        }
        SettingsContextFooterMode::Trust {
            has_roles,
            scroll_axes,
        } => settings_trust_row_footer_items(has_roles, scroll_axes),
    }
}

#[must_use]
pub fn add_row_footer_items(label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group): combined Enter/A display; Enter and A are separate chords.
        HintSpan::Key("↵/A"),
        HintSpan::Text(label),
    ]
}

pub fn append_generate_token_footer_item(items: &mut Vec<HintSpan<'static>>) {
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(auth-form-no-keymap): G triggers token generation inline; no AUTH_FORM_KEYMAP.
        HintSpan::Key("G"),
        HintSpan::Text("generate"),
    ]);
}

fn workspace_picker_footer_items(
    scroll_axes: ScrollAxes,
    include_quit: bool,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(multi-key-display-group): combined up/down navigation display.
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Sep,
        HintSpan::Key(WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Enter)),
        HintSpan::Text("launch"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(workspace-picker-no-keymap): Esc handled inline; no dedicated workspace-picker keymap.
        HintSpan::Key("Esc"),
        HintSpan::Text("return to workspaces"),
        HintSpan::GroupSep,
        HintSpan::Text("type to filter"),
    ];
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(scroll_items);
    }
    if include_quit {
        items.push(HintSpan::GroupSep);
        items.push(HintSpan::Key(
            WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Quit),
        ));
        items.push(HintSpan::Text("quit"));
    }
    items
}

#[must_use]
pub fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(mount-destination-no-keymap): M handled inline; no MOUNT_DESTINATION_KEYMAP.
        HintSpan::Key("M"),
        HintSpan::Text("mount"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(mount-destination-no-keymap): E handled inline.
        HintSpan::Key("E"),
        HintSpan::Text("edit"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined left/right display.
        HintSpan::Key("\u{2190}/\u{2192}"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(mount-destination-no-keymap): Enter confirms inline.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined C/Esc cancel display.
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("\u{2190}/\u{2192}"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(segmented-choice-no-keymap): Enter handled inline; no SEGMENTED_CHOICE_KEYMAP.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(segmented-choice-no-keymap): Esc handled inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(pick-list-no-keymap): Enter handled inline; no PICK_LIST_KEYMAP.
        HintSpan::Key("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        // UNREGISTERABLE(pick-list-no-keymap): Esc handled inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn filtered_picker_footer_items(
    include_refresh: bool,
    include_collapse: bool,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(descriptive-label): not a key — describes free-text filter input.
        HintSpan::Key("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            // UNREGISTERABLE(filtered-picker-no-keymap): R refresh handled inline; no FILTERED_PICKER_KEYMAP.
            HintSpan::Key("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    if include_collapse {
        items.extend([
            HintSpan::GroupSep,
            // UNREGISTERABLE(multi-key-display-group)
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("collapse/expand section"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(filtered-picker-no-keymap): Enter selects inline.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(filtered-picker-no-keymap): Esc cancels inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

#[must_use]
pub fn op_section_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(op-section-no-keymap): Enter handled inline; no OP_SECTION_KEYMAP.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(op-section-no-keymap): Esc handled inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalFooterMode {
    AuthForm {
        focus: AuthFormFocus,
        shows_source_folder: bool,
        shows_credential_block: bool,
        can_generate_token: bool,
    },
    ConfirmDismiss,
    FileBrowser,
    MountDestination,
    SegmentedChoice,
    PickList {
        commit_label: &'static str,
    },
    ConfirmSave {
        scroll_axes: ScrollAxes,
    },
    SaveDiscardCancel,
    ErrorPopup,
    ContainerInfo,
    StatusPopup,
    OpNamingTextInput,
    OpSection,
    FilteredPicker {
        include_refresh: bool,
        include_collapse: bool,
    },
    YesNo,
}

pub trait ModalFileBrowserFooterState {
    fn footer_items(&self) -> Vec<HintSpan<'static>>;
}

impl ModalFileBrowserFooterState for FileBrowserState {
    fn footer_items(&self) -> Vec<HintSpan<'static>> {
        Self::footer_items(self)
    }
}

pub trait ModalAuthFormFooterState<Focus> {
    fn footer_mode(&self, focus: Focus, can_generate_token: bool) -> ModalFooterMode;
}

impl<V: auth_panel::AuthCredential> ModalAuthFormFooterState<AuthFormFocus>
    for auth_panel::AuthForm<V>
{
    fn footer_mode(&self, focus: AuthFormFocus, can_generate_token: bool) -> ModalFooterMode {
        ModalFooterMode::AuthForm {
            focus,
            shows_source_folder: self.shows_source_folder(),
            shows_credential_block: self.shows_credential_block(),
            can_generate_token,
        }
    }
}

pub trait ModalConfirmSaveFooterState {
    fn footer_mode(&self) -> ModalFooterMode;
}

impl<M: Clone> ModalConfirmSaveFooterState for confirm_save::ConfirmSaveState<M> {
    fn footer_mode(&self) -> ModalFooterMode {
        ModalFooterMode::ConfirmSave {
            scroll_axes: self.scroll_axes(),
        }
    }
}

pub trait ModalContainerInfoFooterState {
    fn content_width(&self) -> usize;
    fn content_height(&self) -> usize;
}

impl ModalContainerInfoFooterState for jackin_tui::components::ContainerInfoState {
    fn content_width(&self) -> usize {
        Self::content_width(self)
    }

    fn content_height(&self) -> usize {
        Self::content_height(self)
    }
}

pub trait ModalOpPickerFooterState {
    fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode;
}

impl<T: OpPickerRenderState> ModalOpPickerFooterState for T {
    fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
        op_picker_modal_footer_mode(
            self.stage(),
            self.naming_stage_input().is_some(),
            include_refresh,
        )
    }
}

#[must_use]
pub const fn op_picker_modal_footer_mode(
    stage: OpPickerStage,
    has_naming_stage_input: bool,
    include_refresh: bool,
) -> ModalFooterMode {
    if has_naming_stage_input {
        return ModalFooterMode::OpNamingTextInput;
    }
    match stage {
        OpPickerStage::Section => ModalFooterMode::OpSection,
        OpPickerStage::Field => ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse: true,
        },
        _ => ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse: false,
        },
    }
}

#[must_use]
pub fn modal_footer_items(mode: ModalFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        ModalFooterMode::AuthForm {
            focus,
            shows_source_folder,
            shows_credential_block,
            can_generate_token,
        } => {
            let mut items =
                auth_form_footer_items(focus, shows_source_folder, shows_credential_block);
            if can_generate_token {
                append_generate_token_footer_item(&mut items);
            }
            items
        }
        ModalFooterMode::ConfirmDismiss | ModalFooterMode::OpNamingTextInput => {
            jackin_tui::components::text_input_hint_spans()
        }
        ModalFooterMode::FileBrowser => Vec::new(),
        ModalFooterMode::MountDestination => mount_destination_footer_items(),
        ModalFooterMode::SegmentedChoice => segmented_choice_footer_items(),
        ModalFooterMode::PickList { commit_label } => pick_list_footer_items(commit_label),
        ModalFooterMode::ConfirmSave { scroll_axes } => confirm_save_footer_items(scroll_axes),
        ModalFooterMode::SaveDiscardCancel => save_discard_cancel_footer_items(),
        ModalFooterMode::ErrorPopup => error_popup_footer_items(),
        // Generic default: no scroll segment. The actual render path (the host
        // console's frame builder) has the dialog rect and re-derives the real
        // axes, so this `none()` only guards a path that never reaches the
        // screen — and even then it never claims an axis the body cannot move.
        ModalFooterMode::ContainerInfo => container_info_footer_items(ScrollAxes::none()),
        ModalFooterMode::StatusPopup => status_popup_footer_items(),
        ModalFooterMode::OpSection => op_section_footer_items(),
        ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse,
        } => filtered_picker_footer_items(include_refresh, include_collapse),
        ModalFooterMode::YesNo => jackin_tui::components::confirm_hint_spans(),
    }
}

#[must_use]
pub fn confirm_save_footer_items(scroll_axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key(EDITOR_GLOBAL_KEYMAP.glyph_for(EditorGlobalAction::Save)),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined C/Esc cancel display.
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ];
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(scroll_items);
    }
    items
}

/// Hint spans for inline yes/no confirm modals (`Modal::Confirm`,
/// `GlobalMountModal::Confirm`, `SettingsEnvModal::Confirm`).
///
/// Delegates to [`jackin_tui::components::confirm_hint_spans`] so this matches
#[must_use]
pub fn save_discard_cancel_footer_items() -> Vec<HintSpan<'static>> {
    save_discard_hint_spans()
}

#[must_use]
pub fn error_popup_footer_items() -> Vec<HintSpan<'static>> {
    error_popup_hint_spans()
}

/// Debug-info modal footer: the *available* scroll axes (per `axes`), dismiss,
/// and click-to-copy. The scroll segment is omitted when the body fits and
/// shows only the axis/axes that actually overflow, so the footer never
/// advertises a scroll direction the operator cannot move.
#[must_use]
pub fn container_info_footer_items(axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    // Delegate to the shared Debug-info hint builder so the console list modal,
    // the launch cockpit, and any future surface render byte-identical hint bars
    // for the same dialog. The UNREGISTERABLE annotations live at the shared
    // definition in `jackin_tui::components::debug_info_hint_spans`.
    jackin_tui::components::debug_info_hint_spans(axes)
}

#[must_use]
pub fn container_info_footer_items_for_dialog(
    content_width: usize,
    content_height: usize,
    dialog_rect: Rect,
) -> Vec<HintSpan<'static>> {
    let axes =
        jackin_tui::components::dialog_scroll_axes(content_width, content_height, dialog_rect);
    container_info_footer_items(axes)
}

#[must_use]
pub fn status_popup_footer_items() -> Vec<HintSpan<'static>> {
    vec![HintSpan::Text("working")]
}

#[must_use]
pub fn tab_bar_footer_items(
    save_label: &'static str,
    enter_content: bool,
    dirty_change_count: Option<usize>,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(multi-key-display-group): combined prev/next tab display; EDITOR_TAB_BAR_KEYMAP splits these into separate PrevTab (←/⇤) and NextTab (→) entries.
        HintSpan::Key("\u{2190}\u{2192}"),
        HintSpan::Text("switch tab"),
    ];
    if enter_content {
        items.extend([
            HintSpan::GroupSep,
            // Both EDITOR_TAB_BAR_KEYMAP and SETTINGS_TAB_BAR_KEYMAP use the same glyph.
            HintSpan::Key(EDITOR_TAB_BAR_KEYMAP.glyph_for(EditorTabBarAction::FocusContent)),
            HintSpan::Text("enter content"),
        ]);
    }
    append_save_and_escape(&mut items, save_label, dirty_change_count);
    items
}

#[must_use]
pub fn content_footer_items(
    save_label: &'static str,
    row_items: Vec<HintSpan<'static>>,
    dirty_change_count: Option<usize>,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // Both EDITOR_CONTENT_KEYMAP and SETTINGS_*_TAB_KEYMAP use the same ↑↓ glyph.
        HintSpan::Key(EDITOR_CONTENT_KEYMAP.glyph_for(EditorContentAction::MoveUp)),
        HintSpan::Text("navigate"),
    ];

    if !row_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(row_items);
    }

    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key(EDITOR_CONTENT_KEYMAP.glyph_for(EditorContentAction::FocusTabBar)),
        HintSpan::Text("tab bar"),
        HintSpan::GroupSep,
    ]);
    append_save_and_escape(&mut items, save_label, dirty_change_count);
    items
}

#[must_use]
pub fn workspace_mount_row_footer_items(
    has_github_url: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(workspace-mount-row-no-keymap): D removes mount inline; no WORKSPACE_MOUNT_ROW_KEYMAP.
        HintSpan::Key("D"),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): A adds mount inline.
        HintSpan::Key("A"),
        HintSpan::Text("add"),
    ];
    append_open_in_github(&mut items, has_github_url);
    items.extend([
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): R toggles read-only inline.
        HintSpan::Key("R"),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): I cycles isolation inline.
        HintSpan::Key("I"),
        HintSpan::Text("cycle isolation"),
    ]);
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::Sep);
        items.extend(scroll_items);
    }
    items
}

#[must_use]
pub fn global_mount_row_footer_items(
    has_github_url: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        HintSpan::Key(g(SettingsGlobalMountsTabAction::Delete)),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::Add)),
        HintSpan::Text("add"),
    ];
    if has_github_url {
        items.extend([
            HintSpan::Sep,
            HintSpan::Key(g(SettingsGlobalMountsTabAction::OpenGithub)),
            HintSpan::Text("open in GitHub"),
        ]);
    }
    items.extend([
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::ToggleReadonly)),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditRename)),
        HintSpan::Text("rename"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditSource)),
        HintSpan::Text("edit source"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditDest)),
        HintSpan::Text("edit dst"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditScope)),
        HintSpan::Text("edit scope"),
    ]);
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::Sep);
        items.extend(scroll_items);
    }
    items
}

#[must_use]
pub fn secret_op_ref_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = if op_available {
        vec![
            HintSpan::Key(g(SettingsEnvTabAction::Enter)),
            HintSpan::Sep,
            HintSpan::Key(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("re-pick from 1Password"),
            HintSpan::Sep,
        ]
    } else {
        Vec::new()
    };
    items.extend([
        HintSpan::Key(g(SettingsEnvTabAction::Delete)),
        HintSpan::Text("delete"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
    ]);
    items
}

#[must_use]
pub fn secret_plain_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        HintSpan::Key(g(SettingsEnvTabAction::Enter)),
        HintSpan::Text("edit"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::Delete)),
        HintSpan::Text("delete"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::ToggleMask)),
        HintSpan::Text("mask/unmask"),
    ];
    if op_available {
        items.extend([
            HintSpan::Sep,
            HintSpan::Key(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("1Password"),
        ]);
    }
    items
}

#[must_use]
pub fn secret_add_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        HintSpan::Key(g(SettingsEnvTabAction::Enter)),
        HintSpan::Text("add"),
    ];
    if op_available {
        items.extend([
            HintSpan::Sep,
            HintSpan::Key(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("1Password"),
        ]);
    }
    items
}

#[must_use]
pub fn secret_role_header_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key(SETTINGS_ENV_TAB_KEYMAP.glyph_for(SettingsEnvTabAction::Enter)),
        HintSpan::Text("expand"),
        HintSpan::Sep,
        // UNREGISTERABLE(multi-key-display-group): combined collapse/expand left/right display.
        HintSpan::Key("←/→"),
        HintSpan::Text("collapse/expand"),
        HintSpan::Sep,
        HintSpan::Key(SETTINGS_ENV_TAB_KEYMAP.glyph_for(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
    ]
}

#[must_use]
pub fn auth_form_footer_items(
    focus: AuthFormFocus,
    shows_source_folder: bool,
    shows_credential_block: bool,
) -> Vec<HintSpan<'static>> {
    let mut items: Vec<HintSpan<'static>> = match focus {
        AuthFormFocus::Mode => {
            let mut v = vec![
                // UNREGISTERABLE(auth-form-no-keymap): Space cycles mode inline.
                HintSpan::Key("\u{2423}"),
                HintSpan::Text("cycle"),
            ];
            if shows_source_folder || shows_credential_block {
                v.extend([
                    HintSpan::Sep,
                    // UNREGISTERABLE(auth-form-no-keymap): Down navigates fields inline.
                    HintSpan::Key("\u{2193}"),
                    HintSpan::Text("navigate"),
                ]);
            }
            v.extend([
                HintSpan::GroupSep,
                // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
                HintSpan::Key("\u{21e5}"),
                HintSpan::Text("button row"),
            ]);
            v
        }
        AuthFormFocus::SourceFolder => vec![
            // UNREGISTERABLE(auth-form-no-keymap): Enter handled inline.
            HintSpan::Key("↵"),
            HintSpan::Text("browse"),
            HintSpan::Sep,
            // UNREGISTERABLE(multi-key-display-group): combined navigate display.
            HintSpan::Key("\u{2191}/\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
            HintSpan::Key("\u{21e5}"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::CredentialSource => vec![
            // UNREGISTERABLE(auth-form-no-keymap): Enter confirms the field inline.
            HintSpan::Key("↵"),
            HintSpan::Text("set"),
            HintSpan::Sep,
            // UNREGISTERABLE(auth-form-no-keymap): ↑↓ navigates credential source list inline.
            HintSpan::Key("\u{2191}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
            HintSpan::Key("\u{21e5}"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::Save | AuthFormFocus::Cancel | AuthFormFocus::Reset => vec![
            // UNREGISTERABLE(multi-key-display-group): combined left/right display.
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
            HintSpan::Key("\u{21e5}"),
            HintSpan::Text("fields"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Enter handled inline.
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
        ],
    };
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(auth-form-no-keymap): Esc cancels inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

fn append_open_in_github(items: &mut Vec<HintSpan<'static>>, has_github_url: bool) {
    if has_github_url {
        items.extend([
            HintSpan::Sep,
            // UNREGISTERABLE(workspace-mount-no-keymap): used by workspace-mount rows which have no backing keymap; global-mount callers use SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP directly.
            HintSpan::Key("O"),
            HintSpan::Text("open in GitHub"),
        ]);
    }
}

fn append_save_and_escape(
    items: &mut Vec<HintSpan<'static>>,
    save_label: &'static str,
    dirty_change_count: Option<usize>,
) {
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key(EDITOR_GLOBAL_KEYMAP.glyph_for(EditorGlobalAction::Save)),
        HintSpan::Text(save_label),
    ]);
    if let Some(count) = dirty_change_count {
        items.push(HintSpan::Dyn(format!("({count} changes)")));
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key(EDITOR_GLOBAL_KEYMAP.glyph_for(EditorGlobalAction::Escape)),
        HintSpan::Text(if dirty_change_count.is_some() {
            "discard"
        } else {
            "back"
        }),
        HintSpan::Sep,
        HintSpan::Key(WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Quit)),
        HintSpan::Text("quit"),
    ]);
}
