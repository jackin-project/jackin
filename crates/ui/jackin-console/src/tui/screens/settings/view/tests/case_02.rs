// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn settings_env_key_text_plans_own_target_and_label() {
    assert_eq!(
        settings_env_new_key_text_plan(SettingsEnvScope::Global),
        SettingsEnvKeyTextPlan {
            scope: SettingsEnvScope::Global,
            target: SettingsEnvTextTarget::EnvKey {
                scope: SettingsEnvScope::Global,
            },
            label: "New global environment key".to_owned(),
        }
    );
    assert_eq!(
        settings_env_new_key_after_picker_text_plan(SettingsEnvScope::Role("alpha".to_owned())),
        SettingsEnvKeyTextPlan {
            scope: SettingsEnvScope::Role("alpha".to_owned()),
            target: SettingsEnvTextTarget::EnvKey {
                scope: SettingsEnvScope::Role("alpha".to_owned()),
            },
            label: "New environment key for alpha".to_owned(),
        }
    );
    assert_eq!(
        settings_env_empty_key_text_plan(SettingsEnvScope::Global),
        SettingsEnvKeyTextPlan {
            scope: SettingsEnvScope::Global,
            target: SettingsEnvTextTarget::EnvKey {
                scope: SettingsEnvScope::Global,
            },
            label: "Key cannot be empty".to_owned(),
        }
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
        AuthLineRow::AuthKind {
            label: "Claude".to_owned(),
        },
        AuthLineRow::WorkspaceMode {
            mode_label: "api-key".to_owned(),
            inherited: false,
        },
        AuthLineRow::WorkspaceSource {
            display: AuthSourceDisplay::MaskedPlain { chars: 20 },
        },
        AuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Default,
                path: "~/.claude".to_owned(),
            },
        },
        AuthLineRow::Spacer,
    ];

    let lines = auth_lines(&rows, 2, true);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[0].spans[1].content.as_ref(), "Claude");
    assert_eq!(lines[1].spans[1].content.as_ref(), "Mode          ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(
        lines[2].spans[2].content.as_ref(),
        "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}"
    );
    assert_eq!(lines[3].spans[0].content.as_ref(), "  ");
    assert_eq!(lines[3].spans[1].content.as_ref(), "Source folder ");
    assert_eq!(lines[3].spans[2].content.as_ref(), "default: ~/.claude");
    assert!(lines[4].spans.is_empty());

    let folder_selected = auth_lines(&rows, 3, true);
    assert_eq!(folder_selected[2].spans[0].content.as_ref(), "  ");
    assert_eq!(folder_selected[3].spans[0].content.as_ref(), "\u{25b8} ");
}

#[test]
fn auth_source_folder_rows_render_display_kinds_without_env_suffix() {
    let rows = vec![
        AuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Default,
                path: "~/.claude".to_owned(),
            },
        },
        AuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Inherited,
                path: "/global/claude".to_owned(),
            },
        },
        AuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Explicit,
                path: "/settings/claude".to_owned(),
            },
        },
    ];
    let lines = auth_lines(&rows, 0, true);

    assert_eq!(lines[0].spans[2].content.as_ref(), "default: ~/.claude");
    assert_eq!(
        lines[1].spans[2].content.as_ref(),
        "inherited: /global/claude"
    );
    assert_eq!(lines[2].spans[2].content.as_ref(), "/settings/claude");
    for line in lines {
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(!text.contains("explicit:"), "{text}");
        assert!(!text.contains('('), "{text}");
    }
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
        "       + Add architect environment variable"
    );
}

#[test]
fn env_lines_redact_account_owned_sentinel_even_when_unmasked() {
    let rows = [SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "ANTHROPIC_API_KEY".to_owned(),
    }];

    let lines = env_lines(
        &rows,
        0,
        true,
        100,
        |_, _| Some(SecretValueDisplay::Plain("settings-view-sentinel")),
        |_, _| true,
        |_| 0,
    );
    let rendered = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert!(!rendered.contains("settings-view-sentinel"));
    assert!(rendered.contains("ANTHROPIC_API_KEY"));
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
        "  Destination      Mode"
    );
    assert_eq!(lines[1].spans[0].content.as_ref(), "  /workspace       ");
    assert_eq!(lines[2].spans[0].content.as_ref(), "  host: ~/project");
    assert_eq!(lines[4].spans[0].content.as_ref(), "\u{25b8} + Add mount");
}

#[test]
fn shared_auth_rows_render_settings_and_editor_rows_identically() {
    let rows = vec![
        AuthLineRow::AuthKind {
            label: "Claude".to_owned(),
        },
        AuthLineRow::WorkspaceMode {
            mode_label: "api-key".to_owned(),
            inherited: false,
        },
        AuthLineRow::WorkspaceSource {
            display: AuthSourceDisplay::NotRequired,
        },
        AuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Explicit,
                path: "~/.claude".to_owned(),
            },
        },
    ];

    let settings_lines = auth_lines(&rows, 1, true);
    let editor_lines = crate::tui::screens::editor::view::auth_lines(&rows, 1, true);

    let settings_text: Vec<String> = settings_lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    let editor_text: Vec<String> = editor_lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert_eq!(settings_text, editor_text);
}

#[test]
fn accounts_height_includes_registry_add_actions_github_and_scan() {
    let config = jackin_config::AppConfig::default();
    let settings = crate::tui::state::SettingsAuthState::from_config(&config);
    assert_eq!(settings.row_count(), model::ACCOUNT_KINDS.len() + 2);
}

#[test]
fn auth_state_lines_renders_scan_row_and_scanned_badges() {
    let mut auth = scan_view_auth();
    auth.scan.scanned_ids.insert("default-claude".to_owned());
    let env = scan_view_env();
    let texts = line_texts(&auth_state_lines(&auth, &env, true));
    assert!(texts[0].contains("· scanned"), "{texts:?}");
    assert!(
        texts.iter().any(|line| line.contains("Scan for accounts…")),
        "{texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|line| line.contains("Scanning for accounts")),
        "{texts:?}"
    );

    auth.scan.in_flight = true;
    let texts = line_texts(&auth_state_lines(&auth, &env, true));
    assert!(
        texts
            .iter()
            .any(|line| line.contains("Scanning for accounts…")),
        "{texts:?}"
    );
}
