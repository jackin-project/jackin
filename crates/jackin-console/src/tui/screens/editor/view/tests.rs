//! Tests for `view`.

use super::*;

fn hint_labels(items: Vec<HintSpan<'static>>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|span| match span {
            HintSpan::Key(value) | HintSpan::Text(value) => Some(value.to_owned()),
            HintSpan::Dyn(value) => Some(value),
            HintSpan::Sep | HintSpan::GroupSep => None,
        })
        .collect()
}

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
fn editor_contextual_footer_items_detect_op_refs() {
    type TestEditor = WorkspaceEditorState<(), (), jackin_core::EnvValue, (), (), (), (), (), ()>;
    let mut state = TestEditor::new_edit("ws".into(), WorkspaceConfig::default());
    state.active_tab = EditorTab::Secrets;
    state.pending.env.insert(
        "TOKEN".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".to_owned(),
            path: "Vault/Item/Field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );

    let labels = hint_labels(editor_contextual_footer_items(
        &state,
        &AppConfig::default(),
        true,
        Rect::new(0, 0, 80, 10),
    ));

    assert!(labels.iter().any(|label| label == "re-pick from 1Password"));
    assert!(!labels.iter().any(|label| label == "mask/unmask"));
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
        SecretsRow::WorkspaceKeyRow("TOKEN".to_owned()),
        SecretsRow::WorkspaceAddSentinel,
        SecretsRow::RoleHeader {
            role: "alpha".to_owned(),
            expanded: true,
        },
        SecretsRow::RoleKeyRow {
            role: "alpha".to_owned(),
            key: "ROLE_TOKEN".to_owned(),
        },
        SecretsRow::RoleAddSentinel("alpha".to_owned()),
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

// Tests for `editor` agents tab render rendering.
// Pins `[x]`/`[ ]` to the *effectively allowed* state — empty
// `allowed_roles` is the "all allowed" shorthand.
use super::prepare_editor_tab_for_area;
use super::render_roles_tab;
use crate::tui::state::{EditorState, EditorTab, FieldFocus};
use jackin_config::WorkspaceConfig;
use jackin_config::{AppConfig, RoleSource};
use jackin_tui::components::scrollable_panel::viewport_width as scroll_viewport_width;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
    WorkspaceConfig {
        allowed_roles: names.iter().map(|s| (*s).into()).collect(),
        ..WorkspaceConfig::default()
    }
}

fn config_with_agents(names: &[&str]) -> AppConfig {
    let mut config = AppConfig::default();
    for name in names {
        config.roles.insert((*name).into(), RoleSource::default());
    }
    config
}

fn render_roles_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Roles;
    editor.active_field = FieldFocus::Row(0);
    let backend = TestBackend::new(60, 10);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_roles_tab(f, Rect::new(0, 0, 60, 10), &editor, config);
    })
    .unwrap();
    let buf = term.backend().buffer();
    // Collapse the buffer to newline-delimited rows so the test
    // assertion can match per-row semantics ("row N contains `[x]`").
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
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
    editor.tab_scroll_x = u16::MAX;
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
        editor.tab_scroll_x,
        jackin_tui::components::scrollable_panel::max_offset(editor.tab_content_width, viewport)
    );
    assert!(editor.tab_scroll_x > 0);
}

/// The default-role row carries the `★` marker; non-default rows
/// render a plain space in the marker column. Pins the glyph that
/// the `*` keybinding produces in the rendered list.
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

// Tests for `editor` contextual row items rendering.
// Row-specific footer-hint composition for the editor tabs.
use jackin_config::MountConfig;
use jackin_tui::HintSpan;

use crate::tui::screens::editor::view::editor_contextual_footer_items as contextual_row_items;

fn text_labels<'a>(items: &'a [HintSpan<'a>]) -> Vec<&'a str> {
    items
        .iter()
        .filter_map(|it| {
            if let HintSpan::Text(t) = it {
                Some(*t)
            } else {
                None
            }
        })
        .collect()
}

fn key_glyphs<'a>(items: &'a [HintSpan<'a>]) -> Vec<&'a str> {
    items
        .iter()
        .filter_map(|it| {
            if let HintSpan::Key(k) = it {
                Some(*k)
            } else {
                None
            }
        })
        .collect()
}

fn editor_at_mounts_row0(src: &str) -> EditorState<'static> {
    let ws = WorkspaceConfig {
        mounts: vec![MountConfig {
            src: src.to_owned(),
            dst: src.to_owned(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.active_field = FieldFocus::Row(0);
    editor
}

fn body_area() -> Rect {
    Rect::new(0, 0, 120, 40)
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

/// Guard that every footer hint built by `contextual_row_items` exposes
/// single-letter hotkeys in uppercase. Multi-character glyphs (Enter,
/// Tab, Esc, arrows, `*`) pass through unchanged.
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

fn assert_hint_hotkeys_uppercase(hint: &[HintSpan<'_>], context: &str) {
    for item in hint {
        if let HintSpan::Key(k) = item {
            let chars: Vec<char> = k.chars().collect();
            if chars.len() == 1 {
                let c = chars[0];
                if c.is_alphabetic() {
                    assert!(
                        c.is_uppercase(),
                        "[{context}] single-letter hotkey must be uppercase; got {k:?}"
                    );
                }
            }
        }
    }
}

// Tests for `editor` general tab render rendering.
use super::render_general_tab;

#[test]
fn general_tab_clamps_horizontal_scroll_with_shared_scrollable_block() {
    let ws = WorkspaceConfig {
        workdir: "/workspace/path/that/is/long/enough/to/require/horizontal/scrolling".into(),
        ..Default::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_field = FieldFocus::Row(1);
    editor.set_tab_content_scroll_focused(true);
    editor.tab_scroll_x = u16::MAX;
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
        editor.tab_scroll_x,
        jackin_tui::components::scrollable_panel::max_offset(editor.tab_content_width, viewport)
    );
    assert!(editor.tab_scroll_x > 0);
}

// Tests for `editor` mounts tab render rendering.
use super::render_editor_with_footer as render_editor;

#[test]
fn readonly_mount_renders_ro_mode() {
    let ws = WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/host/a".into(),
            dst: "/host/a".into(),
            readonly: true,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);

    let config = AppConfig::default();
    let backend = TestBackend::new(80, 10);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_editor(f, f.area(), &editor, &config, true);
    })
    .unwrap();

    let buf = term.backend().buffer();
    let found = (0..buf.area.height).any(|y| {
        let row = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>();
        row.contains(" ro ") || row.trim_end().ends_with(" ro") || row.contains(" ro  ")
    });
    assert!(
        found,
        "readonly mount render must show `ro` in the mode column"
    );
}

// Tests for `editor` secrets tab render rendering.
// Render-buffer tests for the Secrets tab. Verifies the masking
// default, the unmasked literal-value path, and that the flat-row
// builder honours `secrets_expanded` for per-role override sections.
use super::render_secrets_tab;
use jackin_config::WorkspaceRoleOverride;

/// Build an editor sitting on the Secrets tab with a single
/// workspace-level env key (`DB_URL = postgres://localhost/db`).
fn editor_with_workspace_env() -> EditorState<'static> {
    let mut env = std::collections::BTreeMap::new();
    env.insert("DB_URL".into(), "postgres://localhost/db".into());
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);
    editor
}

/// Build an editor sitting on the Secrets tab with one role override
/// carrying a single env key (`agent-smith`: `LOG_LEVEL = debug`).
fn editor_with_agent_override() -> EditorState<'static> {
    let mut role_env = std::collections::BTreeMap::new();
    role_env.insert("LOG_LEVEL".into(), "debug".into());
    let mut roles = std::collections::BTreeMap::new();
    roles.insert(
        "agent-smith".into(),
        WorkspaceRoleOverride {
            env: role_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    let ws = WorkspaceConfig {
        roles,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);
    editor
}

/// Render the Secrets tab to a 80x15 `TestBackend`, return the raw
/// buffer as newline-delimited rows so tests can search for glyphs.
fn render_to_dump(editor: &EditorState<'_>) -> String {
    let config = AppConfig::default();
    let backend = TestBackend::new(80, 15);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_secrets_tab(f, Rect::new(0, 0, 80, 15), editor, &config);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn secrets_tab_defaults_to_masked() {
    // `new_edit` leaves `unmasked_rows` empty, so every plain-text
    // value renders masked by default.
    let editor = editor_with_workspace_env();
    assert!(
        editor.unmasked_rows.is_empty(),
        "new_edit must leave unmasked_rows empty (default = all masked)"
    );
    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("●●●●●●●●●●●"),
        "masked-default render must show the mask glyph; got:\n{dump}"
    );
    assert!(
        !dump.contains("postgres://localhost/db"),
        "masked-default render must hide the literal value; got:\n{dump}"
    );
}

#[test]
fn secrets_tab_unmasked_shows_literal_value() {
    let mut editor = editor_with_workspace_env();
    editor
        .unmasked_rows
        .insert((SecretsScopeTag::Workspace, "DB_URL".into()));
    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("postgres://localhost/db"),
        "unmasked render must show literal value; got:\n{dump}"
    );
    assert!(
        !dump.contains("●●●●●●●●●●●"),
        "unmasked render must not show the mask glyph; got:\n{dump}"
    );
}

#[test]
fn secrets_tab_collapsed_agent_omits_key_rows() {
    // `secrets_expanded` is empty by default (set by `new_edit`), so
    // the role section header renders but its `LOG_LEVEL` key row
    // does not.
    let editor = editor_with_agent_override();
    assert!(editor.secrets_expanded.is_empty());
    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("agent-smith"),
        "role header must render; got:\n{dump}"
    );
    assert!(
        !dump.contains("LOG_LEVEL"),
        "collapsed role section must omit key rows; got:\n{dump}"
    );
}

#[test]
fn secrets_tab_expanded_agent_shows_key_rows() {
    let mut editor = editor_with_agent_override();
    editor.secrets_expanded.insert("agent-smith".into());
    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("agent-smith"),
        "role header must still render when expanded; got:\n{dump}"
    );
    assert!(
        dump.contains("LOG_LEVEL"),
        "expanded role section must show its key rows; got:\n{dump}"
    );
}

#[test]
fn secrets_tab_cursor_skips_workspace_header_label() {
    let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    let rows = editor.secrets_flat_rows();
    assert!(
        !rows.is_empty(),
        "secrets_flat_rows must always include at least the WorkspaceAddSentinel"
    );
    assert!(
        matches!(rows.first(), Some(SecretsRow::WorkspaceAddSentinel)),
        "row 0 must be the focusable `+ Add` sentinel, not a header; got {:?}",
        rows.first()
    );
    assert!(
        matches!(editor.active_field, FieldFocus::Row(0)),
        "editor must open on row 0 = sentinel"
    );
}

/// Pins the exact flat-row sequence for a workspace with env vars,
/// one expanded role (with keys), and one collapsed role. Cursor
/// arithmetic in `input/editor.rs` is derived directly from this
/// sequence, so a wrong order causes silent wrong-row selections.
#[test]
fn secrets_flat_rows_sequence_is_canonical() {
    use jackin_config::WorkspaceRoleOverride;

    let mut env = std::collections::BTreeMap::new();
    env.insert("ALPHA".into(), "1".into());
    env.insert("BETA".into(), "2".into());

    let mut role_env = std::collections::BTreeMap::new();
    role_env.insert("KEY".into(), "v".into());

    let mut roles = std::collections::BTreeMap::new();
    roles.insert(
        "agent-a".into(),
        WorkspaceRoleOverride {
            env: role_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    roles.insert(
        "agent-b".into(),
        WorkspaceRoleOverride {
            env: std::collections::BTreeMap::new(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );

    let ws = WorkspaceConfig {
        env,
        roles,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    // Expand agent-a, leave agent-b collapsed.
    editor.secrets_expanded.insert("agent-a".into());

    let rows = editor.secrets_flat_rows();
    // Expected sequence:
    //  0  WorkspaceKeyRow("ALPHA")
    //  1  WorkspaceKeyRow("BETA")
    //  2  SectionSpacer
    //  3  WorkspaceAddSentinel
    //  4  SectionSpacer
    //  5  AgentHeader { role: "agent-a", expanded: true }
    //  6  AgentKeyRow { role: "agent-a", key: "KEY" }
    //  7  SectionSpacer
    //  8  AgentAddSentinel("agent-a")
    //  9  SectionSpacer
    // 10  AgentHeader { role: "agent-b", expanded: false }
    assert_eq!(rows.len(), 11, "unexpected row count: {rows:?}");
    assert!(matches!(&rows[0], SecretsRow::WorkspaceKeyRow(k) if k == "ALPHA"));
    assert!(matches!(&rows[1], SecretsRow::WorkspaceKeyRow(k) if k == "BETA"));
    assert!(matches!(&rows[2], SecretsRow::SectionSpacer));
    assert!(matches!(&rows[3], SecretsRow::WorkspaceAddSentinel));
    assert!(matches!(&rows[4], SecretsRow::SectionSpacer));
    assert!(
        matches!(&rows[5], SecretsRow::RoleHeader { role, expanded: true } if role == "agent-a")
    );
    assert!(
        matches!(&rows[6], SecretsRow::RoleKeyRow { role, key } if role == "agent-a" && key == "KEY")
    );
    assert!(matches!(&rows[7], SecretsRow::SectionSpacer));
    assert!(matches!(&rows[8], SecretsRow::RoleAddSentinel(a) if a == "agent-a"));
    assert!(matches!(&rows[9], SecretsRow::SectionSpacer));
    assert!(
        matches!(&rows[10], SecretsRow::RoleHeader { role, expanded: false } if role == "agent-b")
    );
}

#[test]
fn secrets_tab_empty_renders_only_sentinel() {
    let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    let dump = render_to_dump(&editor);

    assert!(
        dump.contains("+ Add environment variable"),
        "the `+ Add environment variable` sentinel must render; dump:\n{dump}"
    );
    assert!(
        !dump.contains("Workspace env"),
        "the `Workspace env` preamble label must NOT render; dump:\n{dump}"
    );
    assert!(
        !dump.contains("(no env vars)"),
        "the `(no env vars)` placeholder must NOT render; dump:\n{dump}"
    );
    assert!(
        !dump.contains("env var"),
        "TUI text must say `environment variable`, not `env var`; dump:\n{dump}"
    );
}

#[test]
fn op_row_breadcrumb_render_three_segment() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://Work/db/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("Work"),
        "breadcrumb must render vault segment; dump:\n{dump}"
    );
    assert!(
        dump.contains("db"),
        "breadcrumb must render item segment; dump:\n{dump}"
    );
    assert!(
        dump.contains("password"),
        "breadcrumb must render field segment; dump:\n{dump}"
    );
    assert!(
        dump.contains("\u{2192}"),
        "breadcrumb must include the → glyph between item and field; dump:\n{dump}"
    );
    assert!(
        !dump.contains("op://"),
        "op:// scheme prefix must not appear in the breadcrumb; dump:\n{dump}"
    );
    // Mask glyph must not appear on OpRef rows even though
    // editor defaults to all-masked.
    assert!(
        editor.unmasked_rows.is_empty(),
        "default state is all-masked; OpRef rows must still bypass masking"
    );
    assert!(
        !dump.contains("●●●"),
        "OpRef rows must never render the mask glyph; dump:\n{dump}"
    );
}

/// 4-segment is `vault/item/section/field` per the 1Password CLI
/// syntax — not the earlier `account/vault/item/field` reading.
#[test]
fn op_row_breadcrumb_render_four_segment_with_section() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "API_KEY".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://Personal/API Keys/auth/secret_key".into(),
            path: "Personal/API Keys/auth/secret_key".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    // All four components must appear, in order, with the arrow
    // glyph between the section and the field.
    assert!(
        dump.contains("Personal"),
        "vault must render; dump:\n{dump}"
    );
    assert!(dump.contains("API Keys"), "item must render; dump:\n{dump}");
    assert!(
        dump.contains("auth"),
        "section must render between item and field; dump:\n{dump}"
    );
    assert!(
        dump.contains("secret_key"),
        "field must render; dump:\n{dump}"
    );
    assert!(
        dump.contains("\u{2192}"),
        "arrow glyph must precede the field; dump:\n{dump}"
    );
    // The account-prefix branch is dead — no email-style rendering
    // for 4-segment refs.
    assert!(
        !dump.contains('@'),
        "4-segment refs must not render an account email prefix; dump:\n{dump}"
    );
}

/// Text marker (not glyph) — `⚿` rendered inconsistently across
/// terminals; `[op]` reads as "1Password" at a glance.
#[test]
fn op_row_renders_with_op_text_marker() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://Work/db/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("[op]"),
        "OpRef row must render the `[op]` text marker; dump:\n{dump}"
    );
    assert!(
        !dump.contains("\u{26BF}"),
        "the legacy `⚿` glyph must not appear after the marker swap; dump:\n{dump}"
    );
}

#[test]
fn plain_row_renders_without_op_marker() {
    let mut env = std::collections::BTreeMap::new();
    env.insert("DEBUG".into(), "1".into());
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    assert!(
        !dump.contains("[op]"),
        "plain-text row must not render the `[op]` marker; dump:\n{dump}"
    );
}

#[test]
fn op_row_marker_column_is_5_chars_wide_with_brackets() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "DB_URL".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://Work/db/password".into(),
            path: "Work/db/password".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    assert!(
        dump.contains("[op] "),
        "OpRef row must render the marker as exactly `[op] ` (5 chars \
             including trailing space); dump:\n{dump}"
    );
}

#[test]
fn plain_row_marker_column_is_5_blank_chars_for_alignment() {
    let mut env = std::collections::BTreeMap::new();
    env.insert("DEBUG".into(), "1".into());
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    // 7-char prefix region = cursor (1..3) + marker (3..8); on
    // a plain row, cells 3..8 are all blanks.
    let backend = TestBackend::new(80, 15);
    let mut term = Terminal::new(backend).unwrap();
    let config = AppConfig::default();
    term.draw(|f| {
        render_secrets_tab(f, Rect::new(0, 0, 80, 15), &editor, &config);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut cells = String::new();
    for x in 3..8 {
        cells.push_str(buf[(x, 1)].symbol());
    }
    assert_eq!(
        cells, "     ",
        "plain row marker column (cells 3..8 of row 1) must be 5 \
             blank spaces for alignment; got {cells:?}"
    );
}

#[test]
fn secrets_tab_renders_keys_in_alphabetical_order() {
    let mut env = std::collections::BTreeMap::new();
    env.insert("ZULU".into(), "z".into());
    env.insert("ALPHA".into(), "a".into());
    env.insert("MIKE".into(), "m".into());
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    let alpha = dump.find("ALPHA").expect("ALPHA must appear");
    let mike = dump.find("MIKE").expect("MIKE must appear");
    let zulu = dump.find("ZULU").expect("ZULU must appear");
    assert!(
        alpha < mike && mike < zulu,
        "keys must render alphabetically (ALPHA < MIKE < ZULU); offsets {alpha}/{mike}/{zulu}\n{dump}"
    );
}

#[test]
fn section_spacer_appears_between_workspace_and_first_agent_section() {
    let mut env = std::collections::BTreeMap::new();
    env.insert("DB_URL".into(), "postgres://localhost/db".into());
    let mut role_env = std::collections::BTreeMap::new();
    role_env.insert("LOG_LEVEL".into(), "debug".into());
    let mut roles = std::collections::BTreeMap::new();
    roles.insert(
        "agent-smith".into(),
        WorkspaceRoleOverride {
            env: role_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    let ws = WorkspaceConfig {
        env,
        roles,
        ..WorkspaceConfig::default()
    };
    let editor = EditorState::new_edit("ws".into(), ws);
    let rows = editor.secrets_flat_rows();
    assert!(
        matches!(rows.get(3), Some(SecretsRow::SectionSpacer)),
        "row 3 must be a SectionSpacer between workspace add row \
             and first role header; got {:?}",
        rows.get(3)
    );
    assert!(
        matches!(rows.get(4), Some(SecretsRow::RoleHeader { .. })),
        "row 4 must be the role header right after the spacer; \
             got {:?}",
        rows.get(4)
    );
}

#[test]
fn section_spacer_appears_between_consecutive_agent_sections() {
    let mut a_env = std::collections::BTreeMap::new();
    a_env.insert("LEVEL_A".into(), "1".into());
    let mut b_env = std::collections::BTreeMap::new();
    b_env.insert("LEVEL_B".into(), "2".into());
    let mut roles = std::collections::BTreeMap::new();
    roles.insert(
        "agent-architect".into(),
        WorkspaceRoleOverride {
            env: a_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    roles.insert(
        "agent-smith".into(),
        WorkspaceRoleOverride {
            env: b_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    let ws = WorkspaceConfig {
        roles,
        ..WorkspaceConfig::default()
    };
    let editor = EditorState::new_edit("ws".into(), ws);
    let rows = editor.secrets_flat_rows();
    assert!(
        matches!(rows.get(1), Some(SecretsRow::SectionSpacer)),
        "spacer expected before the first role header; rows={rows:?}"
    );
    assert!(
        matches!(rows.get(3), Some(SecretsRow::SectionSpacer)),
        "spacer expected between consecutive role sections; rows={rows:?}"
    );
    assert!(
        !matches!(rows.last(), Some(SecretsRow::SectionSpacer)),
        "no trailing spacer after the final section; rows={rows:?}"
    );
}

/// Helper that renders the Secrets tab to a wider (120-column) terminal
/// so long breadcrumbs (subtitle + section + field) are not truncated.
fn render_to_dump_wide(editor: &EditorState<'_>) -> String {
    let config = AppConfig::default();
    let backend = TestBackend::new(120, 15);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_secrets_tab(f, Rect::new(0, 0, 120, 15), editor, &config);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// `OpRef` whose `path` contains the `[subtitle]` disambiguation form.
/// The subtitle must appear in the rendered output between the item
/// name and the next " / " separator.
#[test]
fn renderer_op_ref_with_subtitle_renders_text() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc/def/fld".into(),
            path: "Private/Claude[alexey@zhokhov.com]/security/auth token".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    // Use the wide terminal so the subtitle and field are not truncated.
    let dump = render_to_dump_wide(&editor);
    // The row must carry the [op] marker (OpRef variant).
    assert!(
        dump.contains("[op]"),
        "OpRef row with subtitle must render `[op]` marker; dump:\n{dump}"
    );
    // Subtitle text must appear in the rendered output.
    assert!(
        dump.contains("alexey@zhokhov.com"),
        "subtitle text must appear in the breadcrumb; dump:\n{dump}"
    );
    // Vault, item, section, and field must all render.
    assert!(dump.contains("Private"), "vault must render; dump:\n{dump}");
    assert!(
        dump.contains("Claude"),
        "item name must render; dump:\n{dump}"
    );
    assert!(
        dump.contains("security"),
        "section must render; dump:\n{dump}"
    );
    assert!(
        dump.contains("auth token"),
        "field must render; dump:\n{dump}"
    );
}

/// `OpRef` whose `path` carries an `?attribute=otp` query suffix. The
/// query must appear in the rendered output after the field name.
#[test]
fn renderer_op_ref_with_attribute_query_renders_text() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "OTP".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc/def/fld?attribute=otp".into(),
            path: "Private/GitHub/one-time password?attribute=otp".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    // Use the wide terminal so `?attribute=otp` is not truncated.
    let dump = render_to_dump_wide(&editor);
    // The row must carry the [op] marker.
    assert!(
        dump.contains("[op]"),
        "OpRef row with attribute query must render `[op]` marker; dump:\n{dump}"
    );
    // The query suffix must appear in the output.
    assert!(
        dump.contains("?attribute=otp"),
        "attribute query must appear in breadcrumb; dump:\n{dump}"
    );
    // Field name must also render.
    assert!(
        dump.contains("one-time password"),
        "field must render; dump:\n{dump}"
    );
}

/// `OpRef` with BOTH a subtitle disambiguation AND an `?attribute=otp`
/// query suffix. Asserts that all six visible pieces appear in the
/// expected left-to-right order: vault → item → subtitle → section →
/// field → query.
#[test]
fn renderer_op_ref_with_subtitle_section_and_query_renders_all() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc/def/sec/fld?attribute=otp".into(),
            path: "Private/Claude[alexey@zhokhov.com]/security/auth token?attribute=otp".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    // Use the wide terminal so no piece is truncated.
    let dump = render_to_dump_wide(&editor);

    // All visible pieces must appear in order:
    // vault → item → subtitle → section → field → query.
    let v_pos = dump.find("Private").expect("vault present");
    let i_pos = dump.find("Claude").expect("item present");
    let s_pos = dump.find("alexey@zhokhov.com").expect("subtitle present");
    let sec_pos = dump.find("security").expect("section present");
    let f_pos = dump.find("auth token").expect("field present");
    let q_pos = dump.find("?attribute=otp").expect("query present");
    assert!(v_pos < i_pos, "vault before item");
    assert!(i_pos < s_pos, "item before subtitle");
    assert!(s_pos < sec_pos, "subtitle before section");
    assert!(sec_pos < f_pos, "section before field");
    assert!(f_pos < q_pos, "field before query");
}

/// A `Plain` row containing a bare `op://...` string gets NO `[op]`
/// marker — it renders as a literal masked value, the visual signal
/// that the operator needs to re-pick it.
#[test]
fn renderer_plain_with_bare_op_uri_renders_as_literal_no_breadcrumb() {
    let mut env = std::collections::BTreeMap::new();
    env.insert("DB_URL".into(), "op://Vault/Item/Field".into());
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    let dump = render_to_dump(&editor);
    // Plain rows carrying a legacy op:// string must NOT render the
    // [op] marker — the visual distinction signals the need to re-pick.
    assert!(
        !dump.contains("[op]"),
        "Plain rows must NOT carry [op] marker; dump:\n{dump}"
    );
    // The breadcrumb separators must not appear — this is a plain
    // masked/literal row, not a breadcrumb render.
    assert!(
        !dump.contains(" / Vault / "),
        "Plain op:// strings must not render vault breadcrumb; dump:\n{dump}"
    );
    // The mask glyph must appear (plain row, masked by default).
    assert!(
        dump.contains("●●●"),
        "Plain row must render masked by default; dump:\n{dump}"
    );
}

/// Single env var → `label_width` equals key length. Without the explicit
/// two-space span, the screenshot bug (`CLAUDE_CODE_OAUTH_TOKENPrivate` / ...)
/// recurs.
#[test]
fn renderer_key_value_separator_always_at_least_two_spaces() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc/def/fld".into(),
            path: "Private/Claude/security/auth token".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);

    // Use the wide terminal so the breadcrumb is not truncated.
    let dump = render_to_dump_wide(&editor);
    assert!(
        dump.contains("CLAUDE_CODE_OAUTH_TOKEN  Private"),
        "expected at least 2 spaces between key and breadcrumb; dump:\n{dump}"
    );
    assert!(
        !dump.contains("CLAUDE_CODE_OAUTH_TOKENPrivate"),
        "no space is the bug; dump:\n{dump}"
    );
}

/// `OpRef` whose `path` doesn't parse as a 3- or 4-segment breadcrumb.
/// The renderer must NOT panic; it shows a re-pick placeholder in the
/// value column without the `[op]` marker, and must NOT leak the UUID URI.
#[test]
fn renderer_op_ref_with_malformed_path_renders_repick_placeholder_no_panic() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc/def/fld".into(),
            path: "garbage-no-slashes".into(),
            account: None,
            on_demand: false,
        }),
    );
    let ws = WorkspaceConfig {
        env,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);
    // Unmask so the placeholder is rendered as text rather than ●●●.
    editor
        .unmasked_rows
        .insert((SecretsScopeTag::Workspace, "TOKEN".into()));

    let dump = render_to_dump_wide(&editor);
    // Malformed path → parse_path_breadcrumb returns None → no [op] marker.
    assert!(!dump.contains("[op]"), "no [op] marker; dump:\n{dump}");
    // Re-pick placeholder must be shown instead of the UUID URI.
    assert!(
        dump.contains("<unparseable path \u{2014} re-pick>"),
        "expected re-pick placeholder; dump:\n{dump}"
    );
    // UUID URI must NOT be visible to the operator.
    assert!(
        !dump.contains("op://abc/def/fld"),
        "UUID URI must NOT leak; dump:\n{dump}"
    );
}
