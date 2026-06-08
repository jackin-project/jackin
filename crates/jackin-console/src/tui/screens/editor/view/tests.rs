//! Tests for `view`.
use super::*;

#[test]
fn general_lines_highlight_selected_row() {
    let lines = general_lines(2, true, "demo", "~/repo", true, false);

    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].spans[0].content.as_ref(), "  Name           ");
    assert_eq!(
        lines[2].spans[0].content.as_ref(),
        "\u{25b8} Keep awake     "
    );
    assert_eq!(lines[2].spans[1].content.as_ref(), "enabled (macOS only)");
    assert_eq!(lines[3].spans[1].content.as_ref(), "disabled");
}

#[test]
fn editor_frame_areas_match_header_tabs_body_footer_contract() {
    let areas = editor_frame_areas(Rect::new(0, 0, 80, 20), 2);

    assert_eq!(areas.header, Rect::new(0, 0, 80, 3));
    assert_eq!(areas.tabs, Rect::new(0, 3, 80, 2));
    assert_eq!(areas.body, Rect::new(0, 5, 80, 13));
    assert_eq!(areas.footer, Rect::new(0, 18, 80, 2));
    assert_eq!(editor_body_area(Rect::new(0, 0, 80, 20), 2), areas.body);
}

#[test]
fn secret_delete_confirm_prompt_names_key() {
    assert_eq!(
        secret_delete_confirm_prompt("TOKEN"),
        "Delete environment variable TOKEN?"
    );
}

#[test]
fn editor_modal_state_helpers_name_fields() {
    assert_eq!(editor_name_input_state("demo").label, "Rename workspace");
    assert_eq!(editor_name_input_state("demo").value(), "demo");
    assert_eq!(
        secret_value_input_state("TOKEN", "value").label,
        "Edit TOKEN"
    );
    assert!(secret_value_input_state("TOKEN", "").is_valid());
    assert_eq!(secret_value_current_text(Some("value")), "value");
    assert_eq!(secret_value_current_text(None), "");
    assert_eq!(
        secret_new_value_input_state("TOKEN").label,
        "Value for TOKEN"
    );
    assert!(secret_new_value_input_state("TOKEN").is_valid());
    assert_eq!(
        mount_destination_input_state("/workspace").label,
        "Destination"
    );
    assert_eq!(
        mount_destination_input_state("/workspace").value(),
        "/workspace"
    );
}

#[test]
fn secret_source_picker_state_names_key() {
    let state = secret_source_picker_state("TOKEN", true);

    assert_eq!(state.key, "TOKEN");
    assert!(state.op_available);
}

#[test]
fn secret_new_key_labels_follow_scope() {
    assert_eq!(
        secret_new_key_label(&SecretsScopeTag::Workspace),
        "New workspace environment key"
    );
    assert_eq!(
        secret_new_key_label(&SecretsScopeTag::Role("alpha".to_owned())),
        "New alpha environment key"
    );
    assert_eq!(
        secret_new_key_after_picker_label(&SecretsScopeTag::Workspace),
        "New environment key for workspace"
    );
    assert_eq!(
        secret_new_key_after_picker_label(&SecretsScopeTag::Role("alpha".to_owned())),
        "New environment key for alpha"
    );
    assert_eq!(secret_empty_key_label(), "Key cannot be empty");
}

#[test]
fn role_load_input_state_names_registry_guard() {
    let state = role_load_input_state(vec!["known/role".to_owned()]);

    assert_eq!(state.label, "Load role");
    assert_eq!(state.forbidden_label, "trusted role registry");
    assert_eq!(state.value(), "");
}

#[test]
fn secret_delete_confirm_state_uses_key_prompt() {
    let state = secret_delete_confirm_state("TOKEN");

    let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind() else {
        panic!("expected default confirm");
    };
    assert_eq!(prompt, "Delete environment variable TOKEN?");
}

#[test]
fn role_trust_confirm_state_names_role_and_repository() {
    let state =
        role_trust_confirm_state("alpha".to_owned(), "https://example.test/role".to_owned());

    assert_eq!(state.title(), "Trust role source");
    let jackin_tui::components::ConfirmKind::Details { prompt, rows, .. } = state.kind() else {
        panic!("expected detail confirm");
    };
    assert_eq!(prompt, "Trust this role source?");
    assert!(
        rows.iter()
            .any(|(label, value)| label == "Repository" && value == "https://example.test/role")
    );
}

#[test]
fn isolated_state_save_confirm_state_lists_containers() {
    let state = isolated_state_save_confirm_state(&["one".to_owned(), "two".to_owned()]);

    let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind() else {
        panic!("expected default confirm");
    };
    assert!(prompt.contains("2 stopped container(s)"));
    assert!(prompt.contains("one"));
    assert!(prompt.contains("two"));
}

#[test]
fn running_isolated_state_save_block_message_lists_containers() {
    let message =
        running_isolated_state_save_block_message(&["alpha".to_owned(), "beta".to_owned()]);

    assert_eq!(
        message,
        "Cannot save: 2 container(s) are running with isolated state for an affected mount: alpha, beta; eject them first.",
    );
}

#[test]
fn secret_key_input_state_marks_scope_duplicates() {
    let state = secret_key_input_state(
        &SecretsScopeTag::Role("alpha".to_owned()),
        "New alpha key",
        "TOKEN",
        vec!["TOKEN".to_owned()],
    );

    assert_eq!(state.label, "New alpha key");
    assert_eq!(state.forbidden_label, "role alpha");
    assert!(state.is_duplicate());
}

#[test]
fn secret_key_input_state_from_pending_marks_scope_duplicates() {
    #[derive(Default)]
    struct Role {
        env: std::collections::BTreeMap<String, String>,
    }

    let mut roles = std::collections::BTreeMap::new();
    roles.insert(
        "alpha".to_owned(),
        Role {
            env: std::collections::BTreeMap::from([("TOKEN".to_owned(), "x".to_owned())]),
        },
    );
    let workspace = std::collections::BTreeMap::from([("WORKSPACE".to_owned(), "x".to_owned())]);

    let state = secret_key_input_state_from_pending(
        &workspace,
        &roles,
        &SecretsScopeTag::Role("alpha".to_owned()),
        "New alpha key",
        "TOKEN",
        |role| &role.env,
    );

    assert_eq!(state.forbidden_label, "role alpha");
    assert!(state.is_duplicate());
}

#[test]
fn mount_lines_render_header_rows_and_sentinel() {
    let rows = [MountDisplayRow {
        destination: "/workspace".to_owned(),
        host_source: Some("host: ~/project".to_owned()),
        mode: "rw",
        isolation: "shared",
        kind: "bind".to_owned(),
    }];

    let lines = mount_lines(&rows, 1, Some(0), true);

    assert_eq!(
        lines[0].spans[0].content.as_ref(),
        "  Destination      Mode  Isolation  Type"
    );
    assert_eq!(lines[1].spans[0].content.as_ref(), "  /workspace       ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "  host: ~/project");
    assert_eq!(lines[4].spans[0].content.as_ref(), "\u{25b8} + Add mount");
    assert_eq!(editor_mount_add_row_width(), text_width("  + Add mount"));
}

#[test]
fn editor_header_title_is_screen_owned() {
    assert_eq!(
        editor_header_title(&EditorMode::Edit {
            name: "demo".to_owned(),
        }),
        "edit workspace · demo"
    );
    assert_eq!(editor_header_title(&EditorMode::Create), "create workspace");
}

#[test]
fn editor_name_value_uses_pending_or_mode_fallback() {
    let edit = EditorMode::Edit {
        name: "saved".to_owned(),
    };

    assert_eq!(editor_name_value(&edit, Some("pending"), ""), "pending");
    assert_eq!(editor_name_value(&edit, None, ""), "saved");
    assert_eq!(
        editor_name_value(&EditorMode::Create, Some("pending"), "(new workspace)"),
        "pending"
    );
    assert_eq!(
        editor_name_value(&EditorMode::Create, None, "(new workspace)"),
        "(new workspace)"
    );
}

#[test]
fn general_content_width_uses_rendered_row_vocabulary() {
    let width = editor_general_content_width("demo", "~/project", true, false);
    let expected = [
        editor_row_width("Name", "demo"),
        editor_row_width("Working dir", "~/project"),
        editor_row_width("Keep awake", "enabled (macOS only)"),
        editor_row_width("Git pull", "disabled"),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    assert_eq!(width, expected);
}

#[test]
fn role_lines_render_status_rows_roles_and_sentinel() {
    let rows = vec![
        EditorRoleRow {
            name: "alpha".to_owned(),
            effectively_allowed: true,
            is_default: false,
        },
        EditorRoleRow {
            name: "beta".to_owned(),
            effectively_allowed: false,
            is_default: true,
        },
    ];

    let lines = role_lines(&rows, 1, false, 2, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  Allowed roles:  ");
    assert_eq!(lines[0].spans[1].content.as_ref(), "  custom  ");
    assert_eq!(lines[0].spans[2].content.as_ref(), "   (1 of 2 allowed)");
    assert_eq!(lines[2].spans[0].content.as_ref(), "  [x]   alpha");
    assert_eq!(lines[3].spans[0].content.as_ref(), "  [ ] \u{2605} beta");
    assert_eq!(lines[5].spans[0].content.as_ref(), "\u{25b8} + Load role");
    assert_eq!(
        editor_roles_status_width(false, 1, 2),
        text_width("  Allowed roles:    custom     (1 of 2 allowed)")
    );
    assert_eq!(editor_role_row_width("alpha"), text_width("  [x] * alpha"));
    assert_eq!(editor_role_load_row_width(), text_width("  + Load role"));
}

#[test]
fn secret_lines_render_workspace_and_role_rows() {
    let rows = vec![
        super::super::model::SecretsRow::WorkspaceKeyRow("TOKEN".to_owned()),
        super::super::model::SecretsRow::WorkspaceAddSentinel,
        super::super::model::SecretsRow::RoleHeader {
            role: "alpha".to_owned(),
            expanded: true,
        },
        super::super::model::SecretsRow::RoleKeyRow {
            role: "alpha".to_owned(),
            key: "ROLE_TOKEN".to_owned(),
        },
        super::super::model::SecretsRow::RoleAddSentinel("alpha".to_owned()),
    ];

    let lines = secret_lines(
        &rows,
        3,
        true,
        80,
        |scope, key| match (scope, key) {
            (SecretsScopeTag::Workspace, "TOKEN") => Some(SecretValueDisplay::Plain("secret")),
            (SecretsScopeTag::Role(role), "ROLE_TOKEN") if role == "alpha" => {
                Some(SecretValueDisplay::OpRefPath("op://Vault/Item/field"))
            }
            _ => None,
        },
        |scope, key| matches!((scope, key), (SecretsScopeTag::Workspace, "TOKEN")),
        |_| true,
        |_| 1,
    );

    assert_eq!(lines[0].spans[2].content.as_ref(), "TOKEN                 ");
    assert_eq!(
        lines[1].spans[0].content.as_ref(),
        "  + Add environment variable"
    );
    assert_eq!(lines[2].spans[2].content.as_ref(), " Role: alpha  (1 vars)");
    assert_eq!(lines[3].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(
        lines[4].spans[0].content.as_ref(),
        "       + Add alpha environment variable"
    );
    assert_eq!(
        editor_secret_line_width(
            &rows[0],
            80,
            |scope, key| match (scope, key) {
                (SecretsScopeTag::Workspace, "TOKEN") => Some(SecretValueDisplay::Plain("secret")),
                _ => None,
            },
            |scope, key| matches!((scope, key), (SecretsScopeTag::Workspace, "TOKEN")),
            |_| true,
            |_| 1,
        ),
        39
    );
    assert_eq!(
        editor_secret_line_width(&rows[1], 80, |_, _| None, |_, _| false, |_| true, |_| 1),
        padded_width("  + Add environment variable")
    );
    assert_eq!(
        editor_secret_line_width(&rows[2], 80, |_, _| None, |_, _| false, |_| true, |_| 1),
        padded_width("       \u{25bc} Role: alpha  (1 vars)")
    );
    assert_eq!(
        editor_secret_line_width(&rows[4], 80, |_, _| None, |_, _| false, |_| true, |_| 1),
        padded_width("       + Add alpha environment variable")
    );
}

#[test]
fn auth_lines_render_kind_mode_source_and_sentinel() {
    let rows = vec![
        EditorAuthLineRow::AuthKind {
            label: "Claude".to_owned(),
        },
        EditorAuthLineRow::WorkspaceMode {
            mode_label: "api-key".to_owned(),
            inherited: true,
        },
        EditorAuthLineRow::WorkspaceSource {
            display: AuthSourceDisplay::Unset {
                env_name: "CLAUDE_API_KEY".to_owned(),
                mode_label: "api-key".to_owned(),
            },
        },
        EditorAuthLineRow::RoleHeader {
            role: "alpha".to_owned(),
            expanded: false,
        },
        EditorAuthLineRow::AddSentinel { eligible: 0 },
    ];

    let lines = auth_lines(&rows, 1, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(lines[1].spans[2].content.as_ref(), "api-key");
    assert_eq!(lines[1].spans[3].content.as_ref(), " (inherited)");
    assert_eq!(
        lines[2].spans[2].content.as_ref(),
        "unset  (CLAUDE_API_KEY for api-key)"
    );
    assert_eq!(lines[3].spans[1].content.as_ref(), " Role: alpha");
    // AddSentinel row: cursor + action label only (no suffix, per action-row style rule).
    assert_eq!(lines[4].spans[1].content.as_ref(), "+ Override for a role");
    assert_eq!(editor_auth_line_width(&rows[0]), padded_width("  Claude"));
    assert_eq!(
        editor_auth_line_width(&rows[1]),
        padded_width("  Mode        api-key (inherited)")
    );
    assert_eq!(
        editor_auth_line_width(&rows[2]),
        padded_width("  Source      unset  (CLAUDE_API_KEY for api-key)")
    );
    assert_eq!(
        editor_auth_line_width(&rows[4]),
        padded_width("  + Override for a role")
    );
}

#[test]
fn auth_workspace_source_rows_reserve_cursor_gutter() {
    let rows = vec![
        EditorAuthLineRow::WorkspaceMode {
            mode_label: "sync".to_owned(),
            inherited: false,
        },
        EditorAuthLineRow::WorkspaceSource {
            display: AuthSourceDisplay::NotRequired,
        },
        EditorAuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Default,
                path: "~/.claude".to_owned(),
                env_var: Some("CLAUDE_CONFIG_DIR".to_owned()),
            },
        },
        EditorAuthLineRow::AddSentinel { eligible: 1 },
    ];

    let source_selected = auth_lines(&rows, 1, true);
    assert_eq!(source_selected[0].spans[0].content.as_ref(), "  ");
    assert_eq!(source_selected[1].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(source_selected[1].spans[1].content.as_ref(), "Source      ");
    assert_eq!(source_selected[2].spans[0].content.as_ref(), "  ");
    assert_eq!(source_selected[3].spans[0].content.as_ref(), "  ");

    let folder_selected = auth_lines(&rows, 2, true);
    assert_eq!(folder_selected[1].spans[0].content.as_ref(), "  ");
    assert_eq!(folder_selected[2].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(
        folder_selected[2].spans[1].content.as_ref(),
        "Source folder"
    );
}
