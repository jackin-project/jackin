//! Tests for `view`.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    Credential,
}

#[test]
fn general_lines_highlight_selected_setting() {
    let lines = general_lines(1, true, false, true);

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[0].spans[2].content.as_ref(), "enabled");
    assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(lines[1].spans[2].content.as_ref(), "disabled");
}

#[test]
fn settings_frame_areas_match_header_tabs_body_footer_contract() {
    let areas = settings_frame_areas(Rect::new(0, 0, 80, 20), 2);

    assert_eq!(areas.header, Rect::new(0, 0, 80, 3));
    assert_eq!(areas.tabs, Rect::new(0, 3, 80, 2));
    assert_eq!(areas.body, Rect::new(0, 5, 80, 13));
    assert_eq!(areas.footer, Rect::new(0, 18, 80, 2));
}

#[test]
fn settings_header_title_is_screen_owned() {
    assert_eq!(settings_header_title(), "settings");
}

#[test]
fn clamp_mounts_scroll_x_for_frame_uses_settings_body_area() {
    let mut scroll_x = u16::MAX;
    let area = Rect::new(0, 0, 80, 20);

    clamp_mounts_scroll_x_for_frame(area, 100, &mut scroll_x);

    let body = settings_frame_areas(area, 2).body;
    let expected = jackin_tui::components::scrollable_panel::max_offset(
        100,
        jackin_tui::components::scrollable_panel::viewport_width(body),
    );
    assert_eq!(scroll_x, expected);
}

#[test]
fn tab_content_heights_account_for_error_rows() {
    assert_eq!(mounts_content_height(4, false), 4);
    assert_eq!(mounts_content_height(4, true), 6);
    assert_eq!(env_content_height(3, true), 5);
    assert_eq!(trust_content_height(0, false), 2);
    assert_eq!(trust_content_height(3, true), 6);
}

#[test]
fn global_mount_confirm_prompts_are_settings_owned() {
    assert_eq!(
        global_mount_confirm_prompt(GlobalMountConfirm::Remove),
        "Remove selected global mount?"
    );
    assert_eq!(
        global_mount_confirm_prompt(GlobalMountConfirm::Sensitive),
        "Sensitive global mount path detected. Save anyway?"
    );
}

#[test]
fn global_mount_confirm_state_uses_settings_prompt() {
    let state = global_mount_confirm_state(GlobalMountConfirm::Discard);

    assert_eq!(state.title(), "Confirm");
    let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind() else {
        panic!("expected default confirm state");
    };
    assert_eq!(prompt, "Discard unsaved global mount changes?");
}

#[test]
fn settings_env_text_input_state_allows_empty_values_only() {
    let value_target = SettingsEnvTextTarget::EnvValue {
        scope: SettingsEnvScope::Global,
        key: "TOKEN".to_owned(),
    };
    let key_target = SettingsEnvTextTarget::EnvKey {
        scope: SettingsEnvScope::Global,
    };

    let value_state = settings_env_text_input_state(&value_target, "Edit TOKEN", "");
    let key_state = settings_env_text_input_state(&key_target, "New key", "");

    assert!(value_state.is_valid());
    assert!(!key_state.is_valid());
}

#[test]
fn settings_env_value_text_label_names_key() {
    assert_eq!(settings_env_value_text_label("TOKEN"), "Edit TOKEN");
    assert_eq!(settings_env_value_current_text(Some("value")), "value");
    assert_eq!(settings_env_value_current_text(None), "");
}

#[test]
fn settings_env_source_picker_state_names_key() {
    let state = settings_env_source_picker_state("TOKEN");

    assert_eq!(state.key, "TOKEN");
    assert!(state.op_available);
}

#[test]
fn settings_env_delete_confirm_state_uses_key_prompt() {
    let state = settings_env_delete_confirm_state("TOKEN");

    let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind() else {
        panic!("expected default confirm state");
    };
    assert_eq!(prompt, "Delete environment variable TOKEN?");
}

#[test]
fn global_mount_text_input_state_names_label() {
    let state = global_mount_text_input_state("Destination", "/workspace");

    assert_eq!(state.label, "Destination");
    assert_eq!(state.value(), "/workspace");
}

#[test]
fn global_mount_scope_text_value_uses_empty_global_fallback() {
    assert_eq!(global_mount_scope_text_value(Some("ops")), "ops");
    assert_eq!(global_mount_scope_text_value(None), "");
}

#[test]
fn global_mount_text_target_labels_are_settings_owned() {
    assert_eq!(
        global_mount_text_target_label(&GlobalMountTextTarget::Rename),
        Some("Rename mount")
    );
    assert_eq!(
        global_mount_text_target_label(&GlobalMountTextTarget::AddScope),
        Some("Scope (empty = global)")
    );
    assert_eq!(
        global_mount_text_target_label(&GlobalMountTextTarget::AddDestination),
        Some("Destination")
    );
}

#[test]
fn settings_env_delete_confirm_prompt_names_key() {
    assert_eq!(
        settings_env_delete_confirm_prompt("TOKEN"),
        "Delete environment variable TOKEN?"
    );
}

#[test]
fn settings_env_key_input_state_marks_scope_duplicates() {
    let mut pending = SettingsEnvConfig {
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
    };
    pending.env.insert("GLOBAL".to_owned(), "1".to_owned());
    pending
        .roles
        .entry("alpha".to_owned())
        .or_default()
        .insert("ROLE_TOKEN".to_owned(), "2".to_owned());

    let state = settings_env_key_input_state(
        &pending,
        &SettingsEnvScope::Role("alpha".to_owned()),
        "New alpha environment key",
        "",
    );

    assert_eq!(state.label, "New alpha environment key");
    assert_eq!(state.forbidden_label, "role alpha");
    assert!(!state.is_duplicate());

    let duplicate = settings_env_key_input_state(
        &pending,
        &SettingsEnvScope::Role("alpha".to_owned()),
        "New alpha environment key",
        "ROLE_TOKEN",
    );
    assert!(duplicate.is_duplicate());
}

#[test]
fn settings_env_new_key_labels_name_scope() {
    assert_eq!(
        settings_env_new_key_label(&SettingsEnvScope::Global),
        "New global environment key"
    );
    assert_eq!(
        settings_env_new_key_label(&SettingsEnvScope::Role("alpha".to_owned())),
        "New alpha environment key"
    );
    assert_eq!(
        settings_env_new_key_after_picker_label(&SettingsEnvScope::Global),
        "New environment key for global"
    );
    assert_eq!(
        settings_env_new_key_after_picker_label(&SettingsEnvScope::Role("alpha".to_owned())),
        "New environment key for alpha"
    );
    assert_eq!(settings_env_empty_key_label(), "Key cannot be empty");
    assert_eq!(
        settings_env_empty_key_error_message(),
        "Env key cannot be empty."
    );
    assert_eq!(
        global_mount_name_empty_message(),
        "Mount name cannot be empty."
    );
    assert_eq!(
        global_mount_gone_message(),
        "Mount no longer exists; selection was cleared."
    );
    assert_eq!(
        global_mount_add_draft_lost_message(),
        "Add-mount draft was lost; press 'a' to start over."
    );
    assert_eq!(
        global_mount_destination_empty_message(),
        "Mount destination cannot be empty."
    );
    assert_eq!(
        global_mount_no_github_url_message(),
        "no GitHub URL for this mount"
    );
    assert_eq!(
        settings_no_registered_roles_error_message(),
        "No registered roles available."
    );
    assert_eq!(
        settings_sensitive_paths_not_confirmed_message(),
        "Save aborted: sensitive paths not confirmed."
    );
    assert_eq!(settings_error_popup_title(), "Settings error");
    assert_eq!(
        settings_auth_op_read_failed_message("bad"),
        "1Password read failed: bad"
    );
}

#[test]
fn trust_lines_include_header_empty_row_and_truncate_long_role() {
    let rows = [SettingsTrustRow {
        role: "very-long-role-name-that-will-truncate".to_owned(),
        git: "https://github.com/example/role".to_owned(),
        trusted: true,
    }];

    let empty = trust_lines(&[], 0, None, false);
    assert_eq!(
        empty[0].spans[0].content.as_ref(),
        "  Role                         Trust      Git"
    );
    assert_eq!(empty[1].spans[0].content.as_ref(), "  (none)");

    let lines = trust_lines(&rows, 0, None, true);
    let rendered = lines[1].spans[0].content.as_ref();
    assert!(rendered.starts_with("\u{25b8} very-long-role-name-that-wi\u{2026}"));
    assert!(rendered.contains("trusted"));
    assert!(rendered.contains("https://github.com/example/role"));
}

#[test]
fn auth_lines_render_kind_mode_source_and_spacer() {
    let rows = vec![
        SettingsAuthLineRow::Kind {
            label: "Claude".to_owned(),
        },
        SettingsAuthLineRow::Mode {
            mode_label: "api-key".to_owned(),
        },
        SettingsAuthLineRow::Source {
            display: AuthSourceDisplay::MaskedPlain { chars: 20 },
        },
        SettingsAuthLineRow::SourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Default,
                path: "~/.claude".to_owned(),
                env_var: Some("CLAUDE_CONFIG_DIR".to_owned()),
            },
        },
        SettingsAuthLineRow::Spacer,
    ];

    let lines = auth_lines(&rows, 2, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  Claude");
    assert_eq!(lines[1].spans[1].content.as_ref(), "Mode          ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(
        lines[2].spans[2].content.as_ref(),
        "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}"
    );
    assert_eq!(lines[3].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[3].spans[1].content.as_ref(), "Source folder ");
    assert!(lines[4].spans.is_empty());

    let folder_selected = auth_lines(&rows, 3, true);
    assert_eq!(folder_selected[2].spans[0].content.as_ref(), "  ");
    assert_eq!(folder_selected[3].spans[0].content.as_ref(), "\u{25b8} ");
}

#[test]
fn env_lines_render_key_header_and_sentinels() {
    let rows = vec![
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: "TOKEN".to_owned(),
        },
        SettingsEnvRow::GlobalAddSentinel,
        SettingsEnvRow::RoleHeader {
            role: "architect".to_owned(),
            expanded: true,
        },
        SettingsEnvRow::RoleAddSentinel("architect".to_owned()),
    ];

    let lines = env_lines(
        &rows,
        1,
        true,
        80,
        |_, key| (key == "TOKEN").then_some(SecretValueDisplay::Plain("secret")),
        |_, key| key == "TOKEN",
        |_| 2,
    );

    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[1].spans[0].content.as_ref(),
        "\u{25b8} + Add environment variable"
    );
    assert!(
        lines[2].spans[2]
            .content
            .contains("Role: architect  (2 vars)")
    );
    assert_eq!(
        lines[3].spans[0].content.as_ref(),
        "  + Add architect environment variable"
    );
}

#[test]
fn global_mount_lines_render_header_rows_and_sentinel() {
    let rows = [MountDisplayRow {
        destination: "/workspace".to_owned(),
        host_source: Some("host: ~/project".to_owned()),
        mode: "ro",
        isolation: "shared",
        kind: "bind".to_owned(),
    }];

    let lines = global_mount_lines(&rows, Some(1), true);

    assert_eq!(
        lines[0].spans[0].content.as_ref(),
        "  Destination      Mode  Type"
    );
    assert_eq!(lines[1].spans[0].content.as_ref(), "  /workspace       ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "  host: ~/project");
    assert_eq!(lines[4].spans[0].content.as_ref(), "\u{25b8} + Add mount");
}

#[test]
fn auth_content_height_lists_all_kinds_before_drill_in() {
    let rows = vec![
        SettingsAuthRow {
            kind: Kind::Plain,
            mode: false,
            sync_source_dir: None,
        },
        SettingsAuthRow {
            kind: Kind::Credential,
            mode: true,
            sync_source_dir: None,
        },
    ];

    assert_eq!(
        auth_content_height(None, &rows, |_, mode| usize::from(*mode) + 1, false),
        2
    );
}

#[test]
fn auth_content_height_drill_in_tracks_credential_row_and_error() {
    let rows = vec![SettingsAuthRow {
        kind: Kind::Credential,
        mode: true,
        sync_source_dir: None,
    }];

    assert_eq!(
        auth_content_height(
            Some(Kind::Credential),
            &rows,
            |_, mode| usize::from(*mode) + 1,
            true
        ),
        5
    );
}
