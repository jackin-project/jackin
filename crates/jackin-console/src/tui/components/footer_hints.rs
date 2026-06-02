//! Shared footer hint fragments for modal pickers and confirmations.

use jackin_tui::HintSpan;

use crate::tui::components::op_picker::OpPickerStage;
use crate::tui::screens::settings::model::AuthFormFocus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceListFooterMode {
    AgentPicker {
        scroll_focused: bool,
    },
    RolePicker {
        scroll_focused: bool,
    },
    PreviewPane,
    InstanceRow {
        has_snapshot: bool,
    },
    WorkspaceRow {
        scroll_focused: bool,
        enter_label: &'static str,
        is_saved: bool,
        show_expand: bool,
        show_collapse: bool,
        show_open_in_github: bool,
    },
}

#[must_use]
pub fn workspace_list_footer_items(mode: WorkspaceListFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        WorkspaceListFooterMode::AgentPicker { scroll_focused } => {
            workspace_picker_footer_items(scroll_focused, false)
        }
        WorkspaceListFooterMode::RolePicker { scroll_focused } => {
            workspace_picker_footer_items(scroll_focused, true)
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
            scroll_focused,
            enter_label,
            is_saved,
            show_expand,
            show_collapse,
            show_open_in_github,
        } => {
            let mut items: Vec<HintSpan<'static>> = if scroll_focused {
                vec![
                    HintSpan::Key("\u{2191}\u{2193}/\u{2190}\u{2192}"),
                    HintSpan::Text("scroll block"),
                    HintSpan::GroupSep,
                    HintSpan::Key("↵"),
                    HintSpan::Text(enter_label),
                    HintSpan::GroupSep,
                ]
            } else {
                vec![
                    HintSpan::Key("\u{2191}\u{2193}"),
                    HintSpan::Sep,
                    HintSpan::Key("↵"),
                    HintSpan::Text(enter_label),
                    HintSpan::GroupSep,
                ]
            };
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
pub fn create_prelude_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Dyn("Create workspace — follow the prompts".to_string()),
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
    vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::Sep,
        HintSpan::Key("␣"),
        HintSpan::Text("toggle"),
    ]
}

#[must_use]
pub fn settings_trust_row_footer_items(has_roles: bool) -> Vec<HintSpan<'static>> {
    if has_roles {
        vec![
            HintSpan::Key("␣"),
            HintSpan::Text("trust/untrust"),
            HintSpan::Sep,
            HintSpan::Key("H/L"),
            HintSpan::Text("scroll"),
        ]
    } else {
        Vec::new()
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
    scroll_focused: bool,
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
    if scroll_focused {
        items.push(HintSpan::GroupSep);
        items.push(HintSpan::Key("←/→"));
        items.push(HintSpan::Text("scroll block"));
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
        scrollable: bool,
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
            shows_credential_block,
            can_generate_token,
        } => {
            let mut items = auth_form_footer_items(focus, shows_credential_block);
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
        ModalFooterMode::ConfirmSave { scrollable } => confirm_save_footer_items(scrollable),
        ModalFooterMode::SaveDiscardCancel => save_discard_cancel_footer_items(),
        ModalFooterMode::ErrorPopup => error_popup_footer_items(),
        ModalFooterMode::ContainerInfo => container_info_footer_items(),
        ModalFooterMode::StatusPopup => status_popup_footer_items(),
        ModalFooterMode::OpSection => op_section_footer_items(),
        ModalFooterMode::FilteredPicker { include_refresh } => {
            filtered_picker_footer_items(include_refresh)
        }
        ModalFooterMode::YesNo => yes_no_footer_items(),
    }
}

#[must_use]
pub fn confirm_save_footer_items(scrollable: bool) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ];
    if scrollable {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("scroll"),
        ]);
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

#[must_use]
pub fn container_info_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("↵/Esc"),
        HintSpan::Text("dismiss"),
        HintSpan::GroupSep,
        HintSpan::Key("click"),
        HintSpan::Text("copy value"),
    ]
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
        HintSpan::Key("\u{21e7}Tab"),
        HintSpan::Text("tab bar"),
        HintSpan::GroupSep,
    ]);
    append_save_and_escape(&mut items, save_label, dirty_change_count);
    items
}

#[must_use]
pub fn workspace_mount_row_footer_items(has_github_url: bool) -> Vec<HintSpan<'static>> {
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
        HintSpan::Sep,
        HintSpan::Key("H/L"),
        HintSpan::Text("scroll"),
    ]);
    items
}

#[must_use]
pub fn global_mount_row_footer_items(has_github_url: bool) -> Vec<HintSpan<'static>> {
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
        HintSpan::Sep,
        HintSpan::Key("H/L"),
        HintSpan::Text("scroll"),
    ]);
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
    shows_credential_block: bool,
) -> Vec<HintSpan<'static>> {
    let mut items: Vec<HintSpan<'static>> = match focus {
        AuthFormFocus::Mode => {
            let mut v = vec![HintSpan::Key("\u{2423}"), HintSpan::Text("cycle")];
            if shows_credential_block {
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
mod tests {
    use super::*;

    fn labels(items: Vec<HintSpan<'static>>) -> Vec<String> {
        items
            .into_iter()
            .filter_map(|item| match item {
                HintSpan::Key(value) | HintSpan::Text(value) => Some(value.to_string()),
                HintSpan::Dyn(value) => Some(value),
                HintSpan::Sep | HintSpan::GroupSep => None,
            })
            .collect()
    }

    #[test]
    fn workspace_list_footer_role_picker_includes_quit() {
        assert_eq!(
            labels(workspace_list_footer_items(
                WorkspaceListFooterMode::RolePicker {
                    scroll_focused: true
                }
            )),
            vec![
                "\u{2191}\u{2193}",
                "↵",
                "launch",
                "Esc",
                "return to workspaces",
                "←/→",
                "scroll block",
                "Q",
                "quit",
            ]
        );
    }

    #[test]
    fn workspace_list_footer_instance_snapshot_can_enter_preview() {
        let labels = labels(workspace_list_footer_items(
            WorkspaceListFooterMode::InstanceRow { has_snapshot: true },
        ));
        assert!(labels.windows(2).any(|pair| pair == ["⇥", "into preview"]));
    }

    #[test]
    fn workspace_list_footer_saved_workspace_shows_row_actions() {
        assert_eq!(
            labels(workspace_list_footer_items(
                WorkspaceListFooterMode::WorkspaceRow {
                    scroll_focused: false,
                    enter_label: "launch",
                    is_saved: true,
                    show_expand: true,
                    show_collapse: false,
                    show_open_in_github: true,
                }
            )),
            vec![
                "\u{2191}\u{2193}",
                "↵",
                "launch",
                "E",
                "edit",
                "N",
                "new",
                "D",
                "delete",
                "S",
                "settings",
                "\u{2192}",
                "expand",
                "O",
                "open in GitHub",
                "Q",
                "quit",
            ]
        );
    }

    #[test]
    fn op_picker_modal_footer_mode_routes_naming_section_and_filtered_stages() {
        assert_eq!(
            op_picker_modal_footer_mode(OpPickerStage::NewItemName, true, true),
            ModalFooterMode::OpNamingTextInput
        );
        assert_eq!(
            op_picker_modal_footer_mode(OpPickerStage::Section, false, true),
            ModalFooterMode::OpSection
        );
        assert_eq!(
            op_picker_modal_footer_mode(OpPickerStage::Item, false, true),
            ModalFooterMode::FilteredPicker {
                include_refresh: true
            }
        );
    }

    #[test]
    fn create_prelude_footer_names_prompt_flow() {
        assert_eq!(
            labels(create_prelude_footer_items()),
            vec!["Create workspace — follow the prompts", "Esc", "cancel"]
        );
    }

    #[test]
    fn destructive_confirm_footer_keeps_escape_cancel() {
        assert_eq!(
            labels(destructive_confirm_footer_items()),
            vec!["Y", "yes", "N", "no", "Esc", "cancel"]
        );
    }

    #[test]
    fn editor_general_footer_rows_match_expected_actions() {
        assert_eq!(
            labels(editor_general_row_footer_items(0, true)),
            vec!["↵", "rename"]
        );
        assert_eq!(
            labels(editor_general_row_footer_items(1, true)),
            vec!["↵", "pick working directory"]
        );
        assert!(labels(editor_general_row_footer_items(1, false)).is_empty());
        assert_eq!(
            labels(editor_general_row_footer_items(2, true)),
            vec!["␣", "toggle"]
        );
    }

    #[test]
    fn auth_footer_role_header_includes_reset() {
        assert_eq!(
            labels(auth_row_footer_items(AuthRowFooterMode::RoleHeader)),
            vec!["↵", "expand", "←/→", "collapse/expand", "D", "reset"]
        );
    }

    #[test]
    fn settings_trust_footer_depends_on_roles() {
        assert!(labels(settings_trust_row_footer_items(false)).is_empty());
        assert_eq!(
            labels(settings_trust_row_footer_items(true)),
            vec!["␣", "trust/untrust", "H/L", "scroll"]
        );
    }

    #[test]
    fn add_row_footer_uses_enter_or_a() {
        assert_eq!(labels(add_row_footer_items("add override")), vec!["↵/A", "add override"]);
    }

    #[test]
    fn generate_token_footer_appends_group() {
        let mut items = vec![HintSpan::Key("Esc"), HintSpan::Text("cancel")];
        append_generate_token_footer_item(&mut items);
        assert_eq!(labels(items), vec!["Esc", "cancel", "G", "generate"]);
    }
}
