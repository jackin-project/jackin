//! Shared footer hint fragments for modal pickers and confirmations.

use jackin_tui::HintSpan;
use jackin_tui::components::{ScrollAxes, scroll_hint_spans};
use ratatui::layout::Rect;

use crate::tui::app::ConsoleManagerStageRoute;
use crate::tui::components::auth_panel;
use crate::tui::components::confirm_save;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::op_picker::OpPickerRenderState;
use crate::tui::components::op_picker::OpPickerStage;
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
    },
    WorkspaceRow {
        scroll_axes: ScrollAxes,
        enter_label: &'static str,
        is_saved: bool,
        show_expand: bool,
        show_collapse: bool,
        show_open_in_github: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterFacts {
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub selected_instance: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
    pub selected_saved_workspace: bool,
    pub selected_new_workspace: bool,
    pub show_expand: bool,
    pub show_collapse: bool,
    pub workspace_scroll_axes: ScrollAxes,
    pub show_open_in_github: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceListFooterInputFacts {
    pub selected_row: ManagerListRow,
    pub inline_agent_picker: bool,
    pub inline_role_picker: bool,
    pub preview_focused: bool,
    pub selected_instance_has_snapshot: bool,
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
        selected_saved_workspace: row_facts.selected_saved_workspace,
        selected_new_workspace: row_facts.selected_new_workspace,
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        workspace_scroll_axes: facts.workspace_scroll_axes,
        show_open_in_github: facts.show_open_in_github,
    }
}

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
        show_expand: facts.show_expand,
        show_collapse: facts.show_collapse,
        show_open_in_github: facts.show_open_in_github,
    }
}

#[must_use]
pub fn workspace_list_footer_items(mode: WorkspaceListFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        WorkspaceListFooterMode::AgentPicker { scroll_axes } => {
            workspace_picker_footer_items(scroll_axes, false)
        }
        WorkspaceListFooterMode::RolePicker { scroll_axes } => {
            workspace_picker_footer_items(scroll_axes, true)
        }
        WorkspaceListFooterMode::PreviewPane => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate panes"),
            HintSpan::Sep,
            HintSpan::Key("↵"),
            HintSpan::Text("attach focused pane"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("back"),
            HintSpan::GroupSep,
            HintSpan::Key("Q"),
            HintSpan::Text("quit"),
        ],
        WorkspaceListFooterMode::InstanceRow { has_snapshot } => {
            let mut items = vec![
                HintSpan::Key("\u{2191}\u{2193}"),
                HintSpan::Sep,
                HintSpan::Key("↵"),
                HintSpan::Text("reconnect"),
                HintSpan::Sep,
                HintSpan::Key("N"),
                HintSpan::Text("new session"),
                HintSpan::Sep,
                HintSpan::Key("X"),
                HintSpan::Text("shell"),
                HintSpan::Sep,
                HintSpan::Key("T"),
                HintSpan::Text("stop"),
                HintSpan::Sep,
                HintSpan::Key("P"),
                HintSpan::Text("purge"),
            ];
            if has_snapshot {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key("⇥"));
                items.push(HintSpan::Text("into preview"));
            }
            items.extend([
                HintSpan::GroupSep,
                HintSpan::Key("\u{2190}"),
                HintSpan::Text("back"),
                HintSpan::GroupSep,
                HintSpan::Key("Q"),
                HintSpan::Text("quit"),
            ]);
            items
        }
        WorkspaceListFooterMode::WorkspaceRow {
            scroll_axes,
            enter_label,
            is_saved,
            show_expand,
            show_collapse,
            show_open_in_github,
        } => {
            let mut items = Vec::new();
            if scroll_axes.any() {
                items.extend(scroll_hint_spans(scroll_axes));
                items.push(HintSpan::GroupSep);
            } else {
                items.push(HintSpan::Key("\u{2191}\u{2193}"));
                items.push(HintSpan::Sep);
            }
            items.extend([
                HintSpan::Key("↵"),
                HintSpan::Text(enter_label),
                HintSpan::GroupSep,
            ]);
            if is_saved {
                items.extend([HintSpan::Key("E"), HintSpan::Text("edit"), HintSpan::Sep]);
            }
            items.extend([HintSpan::Key("N"), HintSpan::Text("new")]);
            if is_saved {
                items.extend([HintSpan::Sep, HintSpan::Key("D"), HintSpan::Text("delete")]);
            }
            items.extend([
                HintSpan::Sep,
                HintSpan::Key("S"),
                HintSpan::Text("settings"),
            ]);
            if show_expand {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key("\u{2192}"));
                items.push(HintSpan::Text("expand"));
            }
            if show_collapse {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key("\u{2190}"));
                items.push(HintSpan::Text("collapse"));
            }
            if show_open_in_github {
                items.push(HintSpan::Sep);
                items.push(HintSpan::Key("O"));
                items.push(HintSpan::Text("open in GitHub"));
            }
            items.push(HintSpan::GroupSep);
            items.push(HintSpan::Key("Q"));
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
pub fn create_prelude_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Dyn("Create workspace — follow the prompts".to_owned()),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn destructive_confirm_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("Y"),
        HintSpan::Text("yes"),
        HintSpan::Sep,
        HintSpan::Key("N"),
        HintSpan::Text("no"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn editor_general_row_footer_items(row: usize, has_mounts: bool) -> Vec<HintSpan<'static>> {
    match row {
        0 => vec![HintSpan::Key("↵"), HintSpan::Text("rename")],
        1 if has_mounts => vec![HintSpan::Key("↵"), HintSpan::Text("pick working directory")],
        2 | 3 => vec![HintSpan::Key("␣"), HintSpan::Text("toggle")],
        _ => Vec::new(),
    }
}

#[must_use]
pub fn editor_role_row_footer_items(is_existing_role: bool) -> Vec<HintSpan<'static>> {
    if is_existing_role {
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
        AuthRowFooterMode::ManageAuth => vec![HintSpan::Key("↵"), HintSpan::Text("manage auth")],
        AuthRowFooterMode::EditMode => vec![HintSpan::Key("↵"), HintSpan::Text("edit mode")],
        AuthRowFooterMode::RoleHeader => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("expand"),
            HintSpan::Sep,
            HintSpan::Key("←/→"),
            HintSpan::Text("collapse/expand"),
            HintSpan::Sep,
            HintSpan::Key("D"),
            HintSpan::Text("reset"),
        ],
        AuthRowFooterMode::EditSource => vec![HintSpan::Key("↵"), HintSpan::Text("edit source")],
        AuthRowFooterMode::Empty => Vec::new(),
    }
}

#[must_use]
pub fn settings_general_row_footer_items() -> Vec<HintSpan<'static>> {
    // `content_footer_items` already prepends ↑↓ navigate; only add the tab-specific action.
    vec![HintSpan::Key("␣"), HintSpan::Text("toggle")]
}

#[must_use]
pub fn settings_trust_row_footer_items(
    has_roles: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    if has_roles {
        let mut items = vec![HintSpan::Key("␣"), HintSpan::Text("trust/untrust")];
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
    vec![HintSpan::Key("↵/A"), HintSpan::Text(label)]
}

pub fn append_generate_token_footer_item(items: &mut Vec<HintSpan<'static>>) {
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("G"),
        HintSpan::Text("generate"),
    ]);
}

fn workspace_picker_footer_items(
    scroll_axes: ScrollAxes,
    include_quit: bool,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Sep,
        HintSpan::Key("↵"),
        HintSpan::Text("launch"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("return to workspaces"),
    ];
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(scroll_items);
    }
    if include_quit {
        items.push(HintSpan::GroupSep);
        items.push(HintSpan::Key("Q"));
        items.push(HintSpan::Text("quit"));
    }
    items
}

#[must_use]
pub fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("M"),
        HintSpan::Text("mount"),
        HintSpan::GroupSep,
        HintSpan::Key("E"),
        HintSpan::Text("edit"),
        HintSpan::GroupSep,
        HintSpan::Key("\u{2190}/\u{2192}"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("\u{2190}/\u{2192}"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn filtered_picker_footer_items(include_refresh: bool) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

#[must_use]
pub fn op_section_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
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
        _ => ModalFooterMode::FilteredPicker { include_refresh },
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
            jackin_tui::components::hint_bar::CONFIRM_DISMISS_HINT.to_vec()
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
        ModalFooterMode::FilteredPicker { include_refresh } => {
            filtered_picker_footer_items(include_refresh)
        }
        ModalFooterMode::YesNo => yes_no_footer_items(),
    }
}

#[must_use]
pub fn confirm_save_footer_items(scroll_axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
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

#[must_use]
pub fn yes_no_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("Y"),
        HintSpan::Text("yes"),
        HintSpan::GroupSep,
        HintSpan::Key("N/Esc"),
        HintSpan::Text("no"),
    ]
}

#[must_use]
pub fn save_discard_cancel_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("D"),
        HintSpan::Text("discard"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn error_popup_footer_items() -> Vec<HintSpan<'static>> {
    vec![HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")]
}

/// Debug-info modal footer: the *available* scroll axes (per `axes`), dismiss,
/// and click-to-copy. The scroll segment is omitted when the body fits and
/// shows only the axis/axes that actually overflow, so the footer never
/// advertises a scroll direction the operator cannot move.
#[must_use]
pub fn container_info_footer_items(axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    let mut items = scroll_hint_spans(axes);
    if !items.is_empty() {
        items.push(HintSpan::GroupSep);
    }
    items.extend([
        HintSpan::Key("↵"),
        HintSpan::Text("copy value"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("dismiss"),
        HintSpan::GroupSep,
        HintSpan::Key("click"),
        HintSpan::Text("copy value"),
    ]);
    items
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
        HintSpan::Key("\u{2190}\u{2192}"),
        HintSpan::Text("switch tab"),
    ];
    if enter_content {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("\u{21e5}/\u{2193}"),
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
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
    ];

    if !row_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(row_items);
    }

    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("\u{21e7}"),
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
        HintSpan::Key("D"),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        HintSpan::Key("A"),
        HintSpan::Text("add"),
    ];
    append_open_in_github(&mut items, has_github_url);
    items.extend([
        HintSpan::Sep,
        HintSpan::Key("R"),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
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
    let mut items = vec![
        HintSpan::Key("D"),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        HintSpan::Key("A"),
        HintSpan::Text("add"),
    ];
    append_open_in_github(&mut items, has_github_url);
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

#[must_use]
pub fn secret_plain_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
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

#[must_use]
pub fn secret_add_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
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

#[must_use]
pub fn secret_role_header_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵"),
        HintSpan::Text("expand"),
        HintSpan::Sep,
        HintSpan::Key("←/→"),
        HintSpan::Text("collapse/expand"),
        HintSpan::Sep,
        HintSpan::Key("A"),
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
            let mut v = vec![HintSpan::Key("\u{2423}"), HintSpan::Text("cycle")];
            if shows_source_folder || shows_credential_block {
                v.extend([
                    HintSpan::Sep,
                    HintSpan::Key("\u{2193}"),
                    HintSpan::Text("navigate"),
                ]);
            }
            v.extend([
                HintSpan::GroupSep,
                HintSpan::Key("\u{21e5}"),
                HintSpan::Text("button row"),
            ]);
            v
        }
        AuthFormFocus::SourceFolder => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("browse"),
            HintSpan::Sep,
            HintSpan::Key("\u{2191}/\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("\u{21e5}"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::CredentialSource => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("set"),
            HintSpan::Sep,
            HintSpan::Key("\u{2191}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("\u{21e5}"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::Save | AuthFormFocus::Cancel | AuthFormFocus::Reset => vec![
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("\u{21e5}"),
            HintSpan::Text("fields"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
        ],
    };
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

fn append_open_in_github(items: &mut Vec<HintSpan<'static>>, has_github_url: bool) {
    if has_github_url {
        items.extend([
            HintSpan::Sep,
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
        HintSpan::Key("S"),
        HintSpan::Text(save_label),
    ]);
    if let Some(count) = dirty_change_count {
        items.push(HintSpan::Dyn(format!("({count} changes)")));
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text(if dirty_change_count.is_some() {
            "discard"
        } else {
            "back"
        }),
    ]);
}

#[cfg(test)]
mod tests;
