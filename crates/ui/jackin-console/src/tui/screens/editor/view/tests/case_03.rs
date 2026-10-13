// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn secrets_flat_rows_sequence_is_canonical() {
    use jackin_config::WorkspaceRoleOverride;

    let mut env = BTreeMap::new();
    env.insert("ALPHA".into(), "1".into());
    env.insert("BETA".into(), "2".into());

    let mut role_env = BTreeMap::new();
    role_env.insert("KEY".into(), "v".into());

    let mut roles = BTreeMap::new();
    roles.insert(
        "agent-a".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::new(),
            env: role_env,
            github: None,
            default_launch: None,
        },
    );
    roles.insert(
        "agent-b".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::new(),
            env: BTreeMap::new(),
            github: None,
            default_launch: None,
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
    let mut env = BTreeMap::new();
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

#[test]
fn op_row_breadcrumb_render_four_segment_with_section() {
    let mut env = BTreeMap::new();
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

#[test]
fn op_row_renders_with_op_text_marker() {
    let mut env = BTreeMap::new();
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
    let mut env = BTreeMap::new();
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
