//! Tests for `editor` contextual row items rendering.
//! Row-specific footer-hint composition for the editor tabs.
use jackin_config::{AppConfig, MountConfig, RoleSource, WorkspaceConfig};
use jackin_tui::HintSpan;
use ratatui::layout::Rect;

use crate::tui::screens::editor::view::editor_contextual_footer_items as contextual_row_items;
use crate::tui::state::{EditorState, EditorTab, FieldFocus};

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

fn config_with_agents(names: &[&str]) -> AppConfig {
    let mut config = AppConfig::default();
    for name in names {
        config.roles.insert((*name).into(), RoleSource::default());
    }
    config
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
