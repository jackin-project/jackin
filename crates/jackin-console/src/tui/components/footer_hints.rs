//! Shared footer hint fragments for modal pickers and confirmations.

use jackin_tui::HintSpan;

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
}
