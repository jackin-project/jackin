// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn hint_labels(items: Vec<HintSpan<'static>>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|span| match span {
            HintSpan::Key(value) | HintSpan::Text(value) => Some(value.to_owned()),
            HintSpan::Dyn(value) | HintSpan::DynKey(value) => Some(value),
            HintSpan::Sep | HintSpan::GroupSep => None,
        })
        .collect()
}

pub(super) fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
    WorkspaceConfig {
        allowed_roles: names.iter().map(|s| (*s).into()).collect(),
        ..WorkspaceConfig::default()
    }
}

pub(super) fn render_roles_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
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

pub(super) fn text_labels<'a>(items: &'a [HintSpan<'a>]) -> Vec<&'a str> {
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

pub(super) fn key_glyphs<'a>(items: &'a [HintSpan<'a>]) -> Vec<&'a str> {
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

pub(super) fn editor_at_mounts_row0(src: &str) -> EditorState<'static> {
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

pub(super) fn body_area() -> Rect {
    Rect::new(0, 0, 120, 40)
}

pub(super) fn assert_hint_hotkeys_uppercase(hint: &[HintSpan<'_>], context: &str) {
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

pub(super) fn editor_with_workspace_env() -> EditorState<'static> {
    let mut env = BTreeMap::new();
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

pub(super) fn editor_with_agent_override() -> EditorState<'static> {
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
        roles,
        ..WorkspaceConfig::default()
    };
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.active_field = FieldFocus::Row(0);
    editor
}

pub(super) fn render_to_dump(editor: &EditorState<'_>) -> String {
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

pub(super) fn render_to_dump_wide(editor: &EditorState<'_>) -> String {
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
