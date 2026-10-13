// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        padded_width("  Mode          api-key (inherited)")
    );
    assert_eq!(
        editor_auth_line_width(&rows[2]),
        padded_width("  Source        unset  (CLAUDE_API_KEY for api-key)")
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
            },
        },
        EditorAuthLineRow::AddSentinel { eligible: 1 },
    ];

    let source_selected = auth_lines(&rows, 1, true);
    assert_eq!(source_selected[0].spans[0].content.as_ref(), "  ");
    assert_eq!(source_selected[1].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(
        source_selected[1].spans[1].content.as_ref(),
        "Source        "
    );
    assert_eq!(source_selected[2].spans[0].content.as_ref(), "  ");
    assert_eq!(source_selected[3].spans[0].content.as_ref(), "  ");

    let folder_selected = auth_lines(&rows, 2, true);
    assert_eq!(folder_selected[1].spans[0].content.as_ref(), "  ");
    assert_eq!(folder_selected[2].spans[0].content.as_ref(), "\u{25b8} ");
    assert_eq!(
        folder_selected[2].spans[1].content.as_ref(),
        "Source folder "
    );
    assert_eq!(
        folder_selected[2].spans[2].content.as_ref(),
        "default: ~/.claude"
    );
}

#[test]
fn auth_source_folder_rows_render_display_kinds_without_env_suffix() {
    let rows = vec![
        EditorAuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Default,
                path: "~/.claude".to_owned(),
            },
        },
        EditorAuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Inherited,
                path: "/global/claude".to_owned(),
            },
        },
        EditorAuthLineRow::WorkspaceSourceFolder {
            display: AuthSourceFolderDisplay {
                kind: AuthSourceFolderKind::Explicit,
                path: "/workspace/claude".to_owned(),
            },
        },
    ];
    let lines = auth_lines(&rows, 0, true);

    assert_eq!(lines[0].spans[2].content.as_ref(), "default: ~/.claude");
    assert_eq!(
        lines[1].spans[2].content.as_ref(),
        "inherited: /global/claude"
    );
    assert_eq!(lines[2].spans[2].content.as_ref(), "/workspace/claude");
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
fn in_all_mode_all_rows_render_as_checked() {
    // Empty `allowed_roles` ⇒ "all" mode ⇒ every row is `[x]`.
    let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
    let ws = ws_with_allowed(&[]);
    let dump = render_roles_to_dump(ws, &cfg);

    // Every role name should appear on a line that also carries `[x]`.
    for name in ["alpha", "beta", "gamma"] {
        let line = dump
            .lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
        assert!(
            line.contains("[x]"),
            "in 'all' mode role `{name}` row must render `[x]`; got `{line}`"
        );
        assert!(
            !line.contains("[ ]"),
            "in 'all' mode role `{name}` must not render `[ ]`; got `{line}`"
        );
    }
}

#[test]
fn roles_tab_clamps_horizontal_scroll_with_shared_state() {
    let cfg = config_with_agents(&["chainargos/agent-brown-with-extra-long-role-name-for-scroll"]);
    let ws = ws_with_allowed(&[]);
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Roles;
    editor.active_field = FieldFocus::Row(0);
    editor.set_tab_content_scroll_focused(true);
    crate::tui::scroll_block::scroll_area_set_x(&mut editor.tab_scroll, u16::MAX);
    let area = Rect::new(0, 0, 42, 8);
    prepare_editor_tab_for_area(area, &mut editor, &cfg);
    let backend = TestBackend::new(42, 8);
    let mut term = Terminal::new(backend).unwrap();

    term.draw(|f| {
        render_roles_tab(f, area, &editor, &cfg);
    })
    .unwrap();

    let viewport = scroll_viewport_width(area);
    assert_eq!(
        editor.tab_scroll.offset_x(),
        termrock::scroll::max_offset_u16(editor.tab_content_width, viewport)
    );
    assert!(editor.tab_scroll.offset_x() > 0);
}

#[test]
fn default_agent_row_carries_star_marker() {
    let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
    let mut ws = ws_with_allowed(&[]);
    ws.default_role = Some("beta".into());
    let dump = render_roles_to_dump(ws, &cfg);

    let beta_line = dump
        .lines()
        .find(|l| l.contains("beta"))
        .expect("beta must render");
    assert!(
        beta_line.contains('\u{2605}'),
        "default role row must carry the `★` marker; got `{beta_line}`"
    );

    let alpha_line = dump
        .lines()
        .find(|l| l.contains("alpha"))
        .expect("alpha must render");
    assert!(
        !alpha_line.contains('\u{2605}'),
        "non-default rows must not carry `★`; got `{alpha_line}`"
    );
}

#[test]
fn in_custom_mode_only_listed_agents_show_checked() {
    // Non-empty list ⇒ "custom" mode ⇒ only listed rows are `[x]`.
    let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
    let ws = ws_with_allowed(&["beta"]);
    let dump = render_roles_to_dump(ws, &cfg);

    let beta_line = dump
        .lines()
        .find(|l| l.contains("beta"))
        .expect("beta must render");
    assert!(
        beta_line.contains("[x]"),
        "listed role `beta` must render `[x]`; got `{beta_line}`"
    );

    for name in ["alpha", "gamma"] {
        let line = dump
            .lines()
            .find(|l| l.contains(name))
            .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
        assert!(
            line.contains("[ ]"),
            "unlisted role `{name}` must render `[ ]` in 'custom' mode; got `{line}`"
        );
    }
}

#[test]
fn github_mount_row_includes_open_in_github_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let git_dir = tmp.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        git_dir.join("config"),
        r#"[remote "origin"]
    url = git@github.com:owner/repo.git
"#,
    )
    .unwrap();

    let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    editor.mount_info_cache.store_entries([(
        tmp.path().display().to_string(),
        crate::mount_info::inspect(&tmp.path().display().to_string()),
    )]);
    let config = AppConfig::default();
    let hint = contextual_row_items(&editor, &config, true, body_area());
    let keys = key_glyphs(&hint);
    let labels = text_labels(&hint);
    assert!(
        keys.contains(&"O"),
        "GitHub mount row must include `O` key hint; got keys={keys:?}"
    );
    assert!(
        labels.contains(&"open in GitHub"),
        "GitHub mount row must include `open in GitHub` label; got labels={labels:?}"
    );
    assert!(keys.contains(&"D"));
    assert!(keys.contains(&"A"));
}

#[test]
fn non_github_mount_row_omits_open_in_github_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    let config = AppConfig::default();
    let hint = contextual_row_items(&editor, &config, true, body_area());
    let keys = key_glyphs(&hint);
    assert!(
        !keys.contains(&"O"),
        "plain-folder mount must not include `O`; got keys={keys:?}"
    );
    assert!(keys.contains(&"D"));
    assert!(keys.contains(&"A"));
}

#[test]
fn mount_row_includes_toggle_readonly_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    let config = AppConfig::default();
    let hint = contextual_row_items(&editor, &config, true, body_area());
    let keys = key_glyphs(&hint);
    let labels = text_labels(&hint);
    assert!(
        keys.contains(&"R"),
        "mount data row must include `R` key hint; got keys={keys:?}"
    );
    assert!(
        labels.contains(&"toggle ro/rw"),
        "mount data row must include `toggle ro/rw` label; got labels={labels:?}"
    );
}

#[test]
fn mounts_sentinel_row_omits_toggle_readonly_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    editor.active_field = FieldFocus::Row(editor.pending.mounts.len());
    let config = AppConfig::default();
    let hint = contextual_row_items(&editor, &config, true, body_area());
    let keys = key_glyphs(&hint);
    assert!(
        !keys.contains(&"R"),
        "sentinel row must not advertise R; got keys={keys:?}"
    );
}

#[test]
fn footer_hotkeys_are_uppercase() {
    let tmp = tempfile::tempdir().unwrap();
    let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    let config = config_with_agents(&["agent-smith"]);

    let mounts_row = contextual_row_items(&editor, &config, true, body_area());
    assert_hint_hotkeys_uppercase(&mounts_row, "Mounts row 0");

    let mut sentinel_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    sentinel_editor.active_field = FieldFocus::Row(sentinel_editor.pending.mounts.len());
    let sentinel_row = contextual_row_items(&sentinel_editor, &config, true, body_area());
    assert_hint_hotkeys_uppercase(&sentinel_row, "Mounts sentinel");

    let mut roles_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
    roles_editor.active_tab = EditorTab::Roles;
    let roles_row = contextual_row_items(&roles_editor, &config, true, body_area());
    assert_hint_hotkeys_uppercase(&roles_row, "Roles");
}

#[test]
fn general_tab_clamps_horizontal_scroll_with_shared_scrollable_block() {
    let ws = WorkspaceConfig {
        workdir: "/workspace/path/that/is/long/enough/to/require/horizontal/scrolling".into(),
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_field = FieldFocus::Row(1);
    editor.set_tab_content_scroll_focused(true);
    crate::tui::scroll_block::scroll_area_set_x(&mut editor.tab_scroll, u16::MAX);
    let area = Rect::new(0, 0, 42, 8);
    prepare_editor_tab_for_area(area, &mut editor, &AppConfig::default());

    let backend = TestBackend::new(42, 8);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_general_tab(f, area, &editor);
    })
    .unwrap();

    let viewport = scroll_viewport_width(area);
    assert_eq!(
        editor.tab_scroll.offset_x(),
        termrock::scroll::max_offset_u16(editor.tab_content_width, viewport)
    );
    assert!(editor.tab_scroll.offset_x() > 0);
}
