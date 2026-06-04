//! Tests for `footer_hints`.
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
fn workspace_list_footer_facts_prioritize_inline_pickers() {
    assert_eq!(
        workspace_list_footer_mode_for_facts(WorkspaceListFooterFacts {
            scroll_focused: true,
            inline_agent_picker: true,
            inline_role_picker: false,
            selected_instance: true,
            preview_focused: true,
            selected_instance_has_snapshot: true,
            selected_saved_workspace: true,
            selected_new_workspace: false,
            show_expand: true,
            show_collapse: false,
            show_open_in_github: true,
        }),
        WorkspaceListFooterMode::AgentPicker {
            scroll_focused: true,
        }
    );
}

#[test]
fn workspace_list_footer_facts_route_instance_preview_and_new_workspace() {
    assert_eq!(
        workspace_list_footer_mode_for_facts(WorkspaceListFooterFacts {
            scroll_focused: false,
            inline_agent_picker: false,
            inline_role_picker: false,
            selected_instance: true,
            preview_focused: true,
            selected_instance_has_snapshot: true,
            selected_saved_workspace: false,
            selected_new_workspace: false,
            show_expand: false,
            show_collapse: false,
            show_open_in_github: false,
        }),
        WorkspaceListFooterMode::PreviewPane
    );

    assert_eq!(
        workspace_list_footer_mode_for_facts(WorkspaceListFooterFacts {
            scroll_focused: false,
            inline_agent_picker: false,
            inline_role_picker: false,
            selected_instance: false,
            preview_focused: false,
            selected_instance_has_snapshot: false,
            selected_saved_workspace: false,
            selected_new_workspace: true,
            show_expand: false,
            show_collapse: false,
            show_open_in_github: false,
        }),
        WorkspaceListFooterMode::WorkspaceRow {
            scroll_focused: false,
            enter_label: "setup",
            is_saved: false,
            show_expand: false,
            show_collapse: false,
            show_open_in_github: false,
        }
    );
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
            "H/L",
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
