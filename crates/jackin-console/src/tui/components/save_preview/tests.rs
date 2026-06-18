//! Tests for `save_preview`.
use super::{
    SettingsEnvPreview, WorkspaceAuthChange, WorkspaceSaveMode, WorkspaceSavePreview,
    workspace_create_display_name, workspace_save_lines,
};

#[test]
fn workspace_create_display_name_uses_pending_or_visible_fallback() {
    assert_eq!(workspace_create_display_name(Some("demo")), "demo");
    assert_eq!(workspace_create_display_name(None), "(unnamed)");
}

fn empty_workspace_preview() -> WorkspaceSavePreview {
    WorkspaceSavePreview {
        mode: WorkspaceSaveMode::Edit {
            original_name: "demo".to_owned(),
            display_name: "demo".to_owned(),
            pending_name: None,
        },
        original_workdir: Some("/repo".to_owned()),
        pending_workdir: "/repo".to_owned(),
        mount_diffs: Vec::new(),
        auth_changes: Vec::new(),
        original_allowed_roles: Vec::new(),
        pending_allowed_roles: Vec::new(),
        role_count: 0,
        original_default_role: None,
        pending_default_role: None,
        original_keep_awake: false,
        pending_keep_awake: false,
        original_git_pull: false,
        pending_git_pull: false,
        env_original: SettingsEnvPreview::default(),
        env_pending: SettingsEnvPreview::default(),
        collapse_lines: Vec::new(),
    }
}

fn line_text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn workspace_save_lines_omits_auth_section_without_auth_changes() {
    let text = line_text(&workspace_save_lines(&empty_workspace_preview()));

    assert!(!text.contains("Auth:"));
}

#[test]
fn workspace_save_lines_renders_auth_old_new_pairs() {
    let mut preview = empty_workspace_preview();
    preview.auth_changes = vec![
        WorkspaceAuthChange {
            label: "Claude Code mode".to_owned(),
            original: "sync".to_owned(),
            pending: "api_key".to_owned(),
        },
        WorkspaceAuthChange {
            label: "Role smith / Codex source folder".to_owned(),
            original: "inherited: /global/codex".to_owned(),
            pending: "/role/codex".to_owned(),
        },
    ];

    let text = line_text(&workspace_save_lines(&preview));

    assert!(text.contains("Auth:"));
    assert!(text.contains("  Claude Code mode"));
    assert!(text.contains("    - sync"));
    assert!(text.contains("    + api_key"));
    assert!(text.contains("  Role smith / Codex source folder"));
    assert!(text.contains("    - inherited: /global/codex"));
    assert!(text.contains("    + /role/codex"));
}
