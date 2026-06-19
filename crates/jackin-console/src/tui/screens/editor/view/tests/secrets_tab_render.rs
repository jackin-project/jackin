//! Tests for `editor` secrets tab render rendering.
//! Render-buffer tests for the Secrets tab. Verifies the masking
//! default, the unmasked literal-value path, and that the flat-row
//! builder honours `secrets_expanded` for per-role override sections.
use super::super::render_secrets_tab;
use crate::tui::state::{EditorState, EditorTab, FieldFocus, SecretsScopeTag};
use jackin_config::AppConfig;
use jackin_config::{WorkspaceConfig, WorkspaceRoleOverride};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

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
        matches!(
            rows.first(),
            Some(crate::tui::state::SecretsRow::WorkspaceAddSentinel)
        ),
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
    assert!(matches!(&rows[0], crate::tui::state::SecretsRow::WorkspaceKeyRow(k) if k == "ALPHA"));
    assert!(matches!(&rows[1], crate::tui::state::SecretsRow::WorkspaceKeyRow(k) if k == "BETA"));
    assert!(matches!(
        &rows[2],
        crate::tui::state::SecretsRow::SectionSpacer
    ));
    assert!(matches!(
        &rows[3],
        crate::tui::state::SecretsRow::WorkspaceAddSentinel
    ));
    assert!(matches!(
        &rows[4],
        crate::tui::state::SecretsRow::SectionSpacer
    ));
    assert!(
        matches!(&rows[5], crate::tui::state::SecretsRow::RoleHeader { role, expanded: true } if role == "agent-a")
    );
    assert!(
        matches!(&rows[6], crate::tui::state::SecretsRow::RoleKeyRow { role, key } if role == "agent-a" && key == "KEY")
    );
    assert!(matches!(
        &rows[7],
        crate::tui::state::SecretsRow::SectionSpacer
    ));
    assert!(
        matches!(&rows[8], crate::tui::state::SecretsRow::RoleAddSentinel(a) if a == "agent-a")
    );
    assert!(matches!(
        &rows[9],
        crate::tui::state::SecretsRow::SectionSpacer
    ));
    assert!(
        matches!(&rows[10], crate::tui::state::SecretsRow::RoleHeader { role, expanded: false } if role == "agent-b")
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
        matches!(
            rows.get(3),
            Some(crate::tui::state::SecretsRow::SectionSpacer)
        ),
        "row 3 must be a SectionSpacer between workspace add row \
             and first role header; got {:?}",
        rows.get(3)
    );
    assert!(
        matches!(
            rows.get(4),
            Some(crate::tui::state::SecretsRow::RoleHeader { .. })
        ),
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
        matches!(
            rows.get(1),
            Some(crate::tui::state::SecretsRow::SectionSpacer)
        ),
        "spacer expected before the first role header; rows={rows:?}"
    );
    assert!(
        matches!(
            rows.get(3),
            Some(crate::tui::state::SecretsRow::SectionSpacer)
        ),
        "spacer expected between consecutive role sections; rows={rows:?}"
    );
    assert!(
        !matches!(
            rows.last(),
            Some(crate::tui::state::SecretsRow::SectionSpacer)
        ),
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
