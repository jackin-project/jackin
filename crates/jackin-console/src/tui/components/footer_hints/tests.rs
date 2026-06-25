//! Tests for `footer_hints`.
use super::*;
use crate::tui::app::ConsoleManagerStageRoute;
use ratatui::layout::Rect;

fn labels(items: Vec<HintSpan<'static>>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|item| match item {
            HintSpan::Key(value) | HintSpan::Text(value) => Some(value.to_owned()),
            HintSpan::Dyn(value) => Some(value),
            HintSpan::Sep | HintSpan::GroupSep => None,
        })
        .collect()
}

#[test]
fn save_footer_labels_are_component_owned() {
    assert_eq!(editor_save_footer_label(), "save workspace");
    assert_eq!(settings_save_footer_label(), "save settings");
    assert_eq!(pick_list_select_footer_label(), "select");
    assert_eq!(pick_list_confirm_footer_label(), "confirm");
}

#[test]
fn workspace_list_footer_role_picker_includes_quit() {
    assert_eq!(
        labels(workspace_list_footer_items(
            WorkspaceListFooterMode::RolePicker {
                scroll_axes: ScrollAxes {
                    vertical: true,
                    horizontal: false,
                },
            }
        )),
        vec![
            "\u{2191}\u{2193}",
            "↵",
            "launch",
            "Esc",
            "return to workspaces",
            "type to filter",
            "↑↓/j/k",
            "scroll",
            "Ctrl-Q",
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
fn workspace_list_footer_facts_prioritize_inline_pickers() {
    assert_eq!(
        workspace_list_footer_mode_for_facts(WorkspaceListFooterFacts {
            inline_agent_picker: true,
            inline_role_picker: false,
            selected_instance: true,
            preview_focused: true,
            selected_instance_has_snapshot: true,
            selected_saved_workspace: true,
            selected_new_workspace: false,
            show_prewarm: true,
            show_expand: true,
            show_collapse: false,
            workspace_scroll_axes: ScrollAxes::none(),
            show_open_in_github: true,
        }),
        WorkspaceListFooterMode::AgentPicker {
            scroll_axes: ScrollAxes::none(),
        }
    );
}

#[test]
fn workspace_list_footer_facts_derive_row_flags_from_input() {
    assert_eq!(
        workspace_list_footer_facts(WorkspaceListFooterInputFacts {
            selected_row: ManagerListRow::SavedWorkspace(4),
            inline_agent_picker: false,
            inline_role_picker: true,
            preview_focused: true,
            selected_instance_has_snapshot: true,
            show_expand: false,
            show_collapse: true,
            workspace_scroll_axes: ScrollAxes {
                vertical: true,
                horizontal: false,
            },
            show_open_in_github: true,
        }),
        WorkspaceListFooterFacts {
            inline_agent_picker: false,
            inline_role_picker: true,
            selected_instance: false,
            preview_focused: true,
            selected_instance_has_snapshot: true,
            selected_saved_workspace: true,
            selected_new_workspace: false,
            show_prewarm: true,
            show_expand: false,
            show_collapse: true,
            workspace_scroll_axes: ScrollAxes {
                vertical: true,
                horizontal: false,
            },
            show_open_in_github: true,
        }
    );
}

#[test]
fn workspace_list_footer_row_facts_route_row_kinds() {
    assert_eq!(
        workspace_list_footer_row_facts(ManagerListRow::WorkspaceInstance(1, 2)),
        WorkspaceListFooterRowFacts {
            selected_instance: true,
            selected_saved_workspace: false,
            selected_new_workspace: false,
        }
    );
    assert_eq!(
        workspace_list_footer_row_facts(ManagerListRow::CurrentDirectoryInstance(0)),
        WorkspaceListFooterRowFacts {
            selected_instance: true,
            selected_saved_workspace: false,
            selected_new_workspace: false,
        }
    );
    assert_eq!(
        workspace_list_footer_row_facts(ManagerListRow::SavedWorkspace(3)),
        WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: true,
            selected_new_workspace: false,
        }
    );
    assert_eq!(
        workspace_list_footer_row_facts(ManagerListRow::NewWorkspace),
        WorkspaceListFooterRowFacts {
            selected_instance: false,
            selected_saved_workspace: false,
            selected_new_workspace: true,
        }
    );
}

#[test]
fn workspace_list_open_github_visible_requires_saved_workspace_with_mounts() {
    assert!(workspace_list_open_github_visible(
        ManagerListRow::SavedWorkspace(0),
        true
    ));
    assert!(!workspace_list_open_github_visible(
        ManagerListRow::SavedWorkspace(0),
        false
    ));
    assert!(!workspace_list_open_github_visible(
        ManagerListRow::CurrentDirectory,
        true
    ));
    assert!(!workspace_list_open_github_visible(
        ManagerListRow::NewWorkspace,
        true
    ));
}

#[test]
fn workspace_list_footer_facts_route_instance_preview_and_new_workspace() {
    assert_eq!(
        workspace_list_footer_mode_for_facts(WorkspaceListFooterFacts {
            inline_agent_picker: false,
            inline_role_picker: false,
            selected_instance: true,
            preview_focused: true,
            selected_instance_has_snapshot: true,
            selected_saved_workspace: false,
            selected_new_workspace: false,
            show_prewarm: false,
            show_expand: false,
            show_collapse: false,
            workspace_scroll_axes: ScrollAxes::none(),
            show_open_in_github: false,
        }),
        WorkspaceListFooterMode::PreviewPane
    );

    assert_eq!(
        workspace_list_footer_mode_for_facts(WorkspaceListFooterFacts {
            inline_agent_picker: false,
            inline_role_picker: false,
            selected_instance: false,
            preview_focused: false,
            selected_instance_has_snapshot: false,
            selected_saved_workspace: false,
            selected_new_workspace: true,
            show_prewarm: false,
            show_expand: false,
            show_collapse: false,
            workspace_scroll_axes: ScrollAxes::none(),
            show_open_in_github: false,
        }),
        WorkspaceListFooterMode::WorkspaceRow {
            scroll_axes: ScrollAxes::none(),
            enter_label: "setup",
            is_saved: false,
            show_prewarm: false,
            show_expand: false,
            show_collapse: false,
            show_open_in_github: false,
        }
    );
}

#[test]
fn workspace_footer_scroll_axes_prioritize_inline_then_focus_then_names() {
    let inline_axes = ScrollAxes {
        vertical: true,
        horizontal: false,
    };
    let focus_axes = ScrollAxes {
        vertical: false,
        horizontal: true,
    };

    assert_eq!(
        workspace_footer_scroll_axes(WorkspaceFooterScrollFacts {
            inline_agent_picker: true,
            inline_role_picker: false,
            inline_picker_scroll_axes: inline_axes,
            focused_block_scroll_axes: Some(focus_axes),
            list_names_focused: true,
            list_names_scroll_axes: ScrollAxes::none(),
            show_expand: false,
            show_collapse: false,
        }),
        inline_axes
    );
    assert_eq!(
        workspace_footer_scroll_axes(WorkspaceFooterScrollFacts {
            inline_agent_picker: false,
            inline_role_picker: false,
            inline_picker_scroll_axes: ScrollAxes::none(),
            focused_block_scroll_axes: Some(focus_axes),
            list_names_focused: true,
            list_names_scroll_axes: inline_axes,
            show_expand: false,
            show_collapse: false,
        }),
        focus_axes
    );
    assert_eq!(
        workspace_footer_scroll_axes(WorkspaceFooterScrollFacts {
            inline_agent_picker: false,
            inline_role_picker: false,
            inline_picker_scroll_axes: ScrollAxes::none(),
            focused_block_scroll_axes: None,
            list_names_focused: true,
            list_names_scroll_axes: inline_axes,
            show_expand: true,
            show_collapse: false,
        }),
        ScrollAxes::none()
    );
}

#[test]
fn workspace_inline_picker_content_height_prefers_agent_picker() {
    assert_eq!(
        workspace_inline_picker_content_height(WorkspaceInlinePickerContentFacts {
            agent_picker_count: Some(3),
            role_picker_count: Some(9),
        }),
        3
    );
    assert_eq!(
        workspace_inline_picker_content_height(WorkspaceInlinePickerContentFacts {
            agent_picker_count: None,
            role_picker_count: Some(9),
        }),
        9
    );
    assert_eq!(
        workspace_inline_picker_content_height(WorkspaceInlinePickerContentFacts::default()),
        0
    );
}

#[test]
fn workspace_screen_footer_prefers_modal_items() {
    let modal = vec![HintSpan::Key("Esc"), HintSpan::Text("dismiss")];
    let list = vec![HintSpan::Key("Q"), HintSpan::Text("quit")];

    assert_eq!(
        labels(workspace_screen_footer_items(
            WorkspaceScreenFooterFacts::List {
                list_items: list,
                modal_items: Some(modal),
            }
        )),
        vec!["Esc", "dismiss"]
    );
}

#[test]
fn workspace_screen_footer_plan_routes_workspace_screens() {
    assert_eq!(
        workspace_screen_footer_plan(ConsoleManagerStageRoute::List),
        WorkspaceScreenFooterPlan::List
    );
    assert_eq!(
        workspace_screen_footer_plan(ConsoleManagerStageRoute::CreatePrelude),
        WorkspaceScreenFooterPlan::CreatePrelude
    );
    assert_eq!(
        workspace_screen_footer_plan(ConsoleManagerStageRoute::ConfirmDelete),
        WorkspaceScreenFooterPlan::DestructiveConfirm
    );
    assert_eq!(
        workspace_screen_footer_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        WorkspaceScreenFooterPlan::DestructiveConfirm
    );
    assert_eq!(
        workspace_screen_footer_plan(ConsoleManagerStageRoute::Editor),
        WorkspaceScreenFooterPlan::ScreenOwned
    );
    assert_eq!(
        workspace_screen_footer_plan(ConsoleManagerStageRoute::Settings),
        WorkspaceScreenFooterPlan::ScreenOwned
    );
}

#[test]
fn workspace_screen_footer_routes_prelude_and_destructive_confirm() {
    assert_eq!(
        labels(workspace_screen_footer_items(
            WorkspaceScreenFooterFacts::CreatePrelude { modal_items: None },
        )),
        vec!["Create workspace — follow the prompts", "Esc", "cancel"]
    );
    assert_eq!(
        labels(workspace_screen_footer_items(
            WorkspaceScreenFooterFacts::DestructiveConfirm,
        )),
        vec!["↵", "confirm", "Y", "yes", "N/Esc", "no", "⇥", "focus"]
    );
}

#[test]
fn editor_screen_footer_routes_modal_tabbar_and_content() {
    assert_eq!(
        labels(editor_screen_footer_items(EditorScreenFooterFacts::Modal {
            items: vec![HintSpan::Key("Esc"), HintSpan::Text("dismiss")],
        })),
        vec!["Esc", "dismiss"]
    );
    assert_eq!(
        labels(editor_screen_footer_items(
            EditorScreenFooterFacts::TabBar {
                save_label: "save workspace",
                enter_content: true,
                dirty_change_count: Some(2),
            }
        )),
        vec![
            "\u{2190}\u{2192}",
            "switch tab",
            "\u{21e5}/\u{2193}",
            "enter content",
            "S",
            "save workspace",
            "(2 changes)",
            "Esc",
            "discard",
            "Ctrl-Q",
            "quit",
        ]
    );
    assert_eq!(
        labels(editor_screen_footer_items(
            EditorScreenFooterFacts::Content {
                save_label: "save workspace",
                row_items: vec![HintSpan::Key("↵"), HintSpan::Text("rename")],
                dirty_change_count: None,
            }
        )),
        vec![
            "\u{2191}\u{2193}",
            "navigate",
            "↵",
            "rename",
            "\u{21e4}",
            "tab bar",
            "S",
            "save workspace",
            "Esc",
            "back",
            "Ctrl-Q",
            "quit",
        ]
    );
}

#[test]
fn settings_screen_footer_prioritizes_modal_scopes() {
    let facts = SettingsScreenFooterFacts {
        auth_modal_items: Some(vec![HintSpan::Key("A"), HintSpan::Text("auth")]),
        env_modal_items: Some(vec![HintSpan::Key("E"), HintSpan::Text("env")]),
        mounts_modal_items: Some(vec![HintSpan::Key("M"), HintSpan::Text("mount")]),
        screen_items: vec![HintSpan::Key("S"), HintSpan::Text("screen")],
    };
    assert_eq!(
        labels(settings_screen_footer_items(facts)),
        vec!["A", "auth"]
    );

    let facts = SettingsScreenFooterFacts {
        auth_modal_items: None,
        env_modal_items: Some(vec![HintSpan::Key("E"), HintSpan::Text("env")]),
        mounts_modal_items: Some(vec![HintSpan::Key("M"), HintSpan::Text("mount")]),
        screen_items: vec![HintSpan::Key("S"), HintSpan::Text("screen")],
    };
    assert_eq!(
        labels(settings_screen_footer_items(facts)),
        vec!["E", "env"]
    );

    let facts = SettingsScreenFooterFacts {
        auth_modal_items: None,
        env_modal_items: None,
        mounts_modal_items: None,
        screen_items: vec![HintSpan::Key("S"), HintSpan::Text("screen")],
    };
    assert_eq!(
        labels(settings_screen_footer_items(facts)),
        vec!["S", "screen"]
    );
}

#[test]
fn selected_instance_snapshot_routes_by_row_kind() {
    assert!(selected_instance_snapshot_available(
        ManagerListRow::WorkspaceInstance(2, 3),
        |ws, inst| ws == 2 && inst == 3,
        |_| false,
    ));
    assert!(selected_instance_snapshot_available(
        ManagerListRow::CurrentDirectoryInstance(4),
        |_, _| false,
        |inst| inst == 4,
    ));
    assert!(!selected_instance_snapshot_available(
        ManagerListRow::SavedWorkspace(1),
        |_, _| true,
        |_| true,
    ));
}

#[test]
fn workspace_list_footer_saved_workspace_shows_row_actions() {
    assert_eq!(
        labels(workspace_list_footer_items(
            WorkspaceListFooterMode::WorkspaceRow {
                scroll_axes: ScrollAxes::none(),
                enter_label: "launch",
                is_saved: true,
                show_prewarm: true,
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
            "W",
            "prewarm",
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
            "Ctrl-Q",
            "quit",
        ]
    );
}

#[test]
fn workspace_list_footer_workspace_row_shows_scroll_axes() {
    assert_eq!(
        labels(workspace_list_footer_items(
            WorkspaceListFooterMode::WorkspaceRow {
                scroll_axes: ScrollAxes {
                    vertical: false,
                    horizontal: true,
                },
                enter_label: "launch",
                is_saved: false,
                show_prewarm: false,
                show_expand: false,
                show_collapse: false,
                show_open_in_github: false,
            }
        )),
        vec![
            "←→/h/l",
            "scroll",
            "↵",
            "launch",
            "N",
            "new",
            "S",
            "settings",
            "Ctrl-Q",
            "quit",
        ]
    );
}

#[test]
fn settings_context_footer_routes_mounts_and_auth() {
    assert_eq!(
        labels(settings_contextual_row_footer_items(
            SettingsContextFooterMode::MountAddRow,
            false,
        )),
        vec!["↵/A", "add"]
    );
    assert_eq!(
        labels(settings_contextual_row_footer_items(
            SettingsContextFooterMode::MountRow {
                has_github_url: true,
                scroll_axes: ScrollAxes {
                    vertical: false,
                    horizontal: true,
                },
            },
            false,
        )),
        vec![
            "D",
            "remove",
            "A",
            "add",
            "O",
            "open in GitHub",
            "R",
            "toggle ro/rw",
            "N",
            "rename",
            "1",
            "edit source",
            "2",
            "edit dst",
            "3",
            "edit scope",
            "←→/h/l",
            "scroll",
        ]
    );
    assert_eq!(
        labels(settings_contextual_row_footer_items(
            SettingsContextFooterMode::AuthEditSource,
            false,
        )),
        vec!["↵", "edit source"]
    );
}

#[test]
fn settings_context_footer_routes_env_rows() {
    assert_eq!(
        labels(settings_contextual_row_footer_items(
            SettingsContextFooterMode::EnvOpRefRow,
            true,
        )),
        vec![
            "↵",
            "P",
            "re-pick from 1Password",
            "D",
            "delete",
            "A",
            "add",
        ]
    );
    assert_eq!(
        labels(settings_contextual_row_footer_items(
            SettingsContextFooterMode::EnvAddRow,
            false,
        )),
        vec!["↵", "add"]
    );
    assert!(
        labels(settings_contextual_row_footer_items(
            SettingsContextFooterMode::Empty,
            true,
        ))
        .is_empty()
    );
}

#[test]
fn editor_context_footer_routes_general_mounts_and_roles() {
    assert_eq!(
        labels(editor_contextual_row_footer_items(
            EditorContextFooterMode::General {
                row: 0,
                has_mounts: true,
            },
            false,
        )),
        vec!["↵", "rename"]
    );
    assert_eq!(
        labels(editor_contextual_row_footer_items(
            EditorContextFooterMode::MountAddRow,
            false,
        )),
        vec!["↵/A", "add"]
    );
    assert_eq!(
        labels(editor_contextual_row_footer_items(
            EditorContextFooterMode::RoleRow {
                is_existing_role: false,
            },
            false,
        )),
        vec!["↵/A", "load role"]
    );
}

#[test]
fn editor_context_footer_routes_secret_and_auth_rows() {
    assert_eq!(
        labels(editor_contextual_row_footer_items(
            EditorContextFooterMode::SecretPlainRow,
            true,
        )),
        vec![
            "↵",
            "edit",
            "D",
            "delete",
            "A",
            "add",
            "M",
            "mask/unmask",
            "P",
            "1Password",
        ]
    );
    assert_eq!(
        labels(editor_contextual_row_footer_items(
            EditorContextFooterMode::AuthRoleHeader,
            false,
        )),
        vec!["↵", "expand", "←/→", "collapse/expand", "D", "reset",]
    );
    assert!(
        labels(editor_contextual_row_footer_items(
            EditorContextFooterMode::Empty,
            true,
        ))
        .is_empty()
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
            include_refresh: true,
            include_collapse: false,
        }
    );
}

#[test]
fn container_info_footer_derives_scroll_axes_from_dialog_rect() {
    assert_eq!(
        labels(container_info_footer_items_for_dialog(
            120,
            3,
            Rect::new(0, 0, 40, 10),
        )),
        vec![
            "←→/h/l",
            "scroll",
            "↵",
            "copy value",
            "R/O",
            "reveal diagnostics",
            "Esc",
            "dismiss"
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
    // confirm_hint_spans() drives both yes/no and destructive-confirm footers;
    // verify Esc is advertised (as part of "N/Esc no") so cancel is discoverable.
    let texts = labels(jackin_tui::components::confirm_hint_spans());
    assert!(
        texts.iter().any(|t| t.contains("Esc")),
        "Esc must be in confirm spans: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "yes" || t == "Y"),
        "Y/yes must be present: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "no"),
        "no must be present: {texts:?}"
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
    assert!(labels(settings_trust_row_footer_items(false, ScrollAxes::none())).is_empty());
    assert_eq!(
        labels(settings_trust_row_footer_items(true, ScrollAxes::none())),
        vec!["␣", "trust/untrust"]
    );
    assert_eq!(
        labels(settings_trust_row_footer_items(
            true,
            ScrollAxes {
                vertical: false,
                horizontal: true,
            }
        )),
        vec!["␣", "trust/untrust", "←→/h/l", "scroll"]
    );
}

#[test]
fn add_row_footer_uses_enter_or_a() {
    assert_eq!(
        labels(add_row_footer_items("add override")),
        vec!["↵/A", "add override"]
    );
}

#[test]
fn generate_token_footer_appends_group() {
    let mut items = vec![HintSpan::Key("Esc"), HintSpan::Text("cancel")];
    append_generate_token_footer_item(&mut items);
    assert_eq!(labels(items), vec!["Esc", "cancel", "G", "generate"]);
}

#[test]
fn settings_general_content_footer_has_no_duplicate_navigate_span() {
    // Defect 33 regression: before the fix, settings_general_row_footer_items()
    // returned [↑↓ navigate, ·, ␣ toggle] and content_footer_items() prepended
    // its own [↑↓ navigate], producing "↑↓ navigate   ↑↓ navigate · ␣ toggle".
    // Verify the composed hint set contains exactly one "navigate" text span.
    let row_items = settings_general_row_footer_items();
    let all = content_footer_items("save", row_items, None);
    let navigate_count = all
        .iter()
        .filter(|span| matches!(span, HintSpan::Text("navigate")))
        .count();
    assert_eq!(
        navigate_count, 1,
        "exactly one 'navigate' span; got {navigate_count}"
    );
}
