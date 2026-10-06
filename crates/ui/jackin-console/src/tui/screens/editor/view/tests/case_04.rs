// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn op_row_marker_column_is_5_chars_wide_with_brackets() {
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
        dump.contains("[op] "),
        "OpRef row must render the marker as exactly `[op] ` (5 chars \
             including trailing space); dump:\n{dump}"
    );
}

#[test]
fn plain_row_marker_column_is_5_blank_chars_for_alignment() {
    let mut env = BTreeMap::new();
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
    let mut env = BTreeMap::new();
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
    let mut env = BTreeMap::new();
    env.insert("DB_URL".into(), "postgres://localhost/db".into());
    let mut role_env = BTreeMap::new();
    role_env.insert("LOG_LEVEL".into(), "debug".into());
    let mut roles = BTreeMap::new();
    roles.insert(
        "agent-smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::new(),
            env: role_env,
            github: None,
            default_launch: None,
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
    let mut a_env = BTreeMap::new();
    a_env.insert("LEVEL_A".into(), "1".into());
    let mut b_env = BTreeMap::new();
    b_env.insert("LEVEL_B".into(), "2".into());
    let mut roles = BTreeMap::new();
    roles.insert(
        "agent-architect".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::new(),
            env: a_env,
            github: None,
            default_launch: None,
        },
    );
    roles.insert(
        "agent-smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::new(),
            env: b_env,
            github: None,
            default_launch: None,
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

#[test]
fn renderer_op_ref_with_subtitle_renders_text() {
    let mut env = BTreeMap::new();
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

#[test]
fn renderer_op_ref_with_attribute_query_renders_text() {
    let mut env = BTreeMap::new();
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

#[test]
fn renderer_op_ref_with_subtitle_section_and_query_renders_all() {
    let mut env = BTreeMap::new();
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

#[test]
fn renderer_plain_with_bare_op_uri_renders_as_literal_no_breadcrumb() {
    let mut env = BTreeMap::new();
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

#[test]
fn renderer_account_credential_op_ref_never_exposes_breadcrumb() {
    let mut env = BTreeMap::new();
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
        dump.contains("CLAUDE_CODE_OAUTH_TOKEN  ●●●"),
        "account-owned credential must stay masked; dump:\n{dump}"
    );
    assert!(
        !dump.contains("Private/Claude/security/auth token"),
        "account-owned credential breadcrumb leaked; dump:\n{dump}"
    );
}

#[test]
fn renderer_op_ref_with_malformed_path_renders_repick_placeholder_no_panic() {
    let mut env = BTreeMap::new();
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
    // Malformed breadcrumb → the shared core parser rejects it → no [op] marker.
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
