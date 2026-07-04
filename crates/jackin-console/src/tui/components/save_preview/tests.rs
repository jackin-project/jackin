// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `save_preview`.
use super::{
    SettingsEnvPreview, WorkspaceAuthChange, WorkspaceSaveMode, WorkspaceSavePreview,
    WorkspaceToggleSet, build_workspace_save_lines, workspace_create_display_name,
    workspace_save_lines,
};
use crate::mount_info_cache::MountInfoCache;
use crate::tui::screens::editor::model::EditorState;
use jackin_config::{
    AgentAuthConfig, AppConfig, AuthForwardMode, EnvValue, WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::env_model;
use std::path::PathBuf;

type TestEditorState =
    EditorState<WorkspaceConfig, MountInfoCache, (), (), EnvValue, (), (), (), (), (), ()>;

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
        original_toggles: WorkspaceToggleSet::default(),
        pending_toggles: WorkspaceToggleSet::default(),
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

fn edit_lines(original: WorkspaceConfig, pending: WorkspaceConfig) -> String {
    let config = AppConfig::default();
    let mut editor = TestEditorState::new_edit("demo".to_owned(), original);
    editor.pending = pending;
    line_text(&build_workspace_save_lines(&editor, &config, &[]))
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

#[test]
fn workspace_save_preview_lists_auth_mode_and_credential_without_secret_value() {
    let original = WorkspaceConfig {
        workdir: "/repo".to_owned(),
        ..Default::default()
    };
    let mut pending = original.clone();
    pending.claude = Some(AgentAuthConfig {
        auth_forward: AuthForwardMode::ApiKey,
        ..Default::default()
    });
    pending.env.insert(
        env_model::ANTHROPIC_API_KEY_ENV_NAME.to_owned(),
        EnvValue::Plain("super-secret".to_owned()),
    );

    let text = edit_lines(original, pending);

    assert!(text.contains("Auth:"));
    assert!(text.contains("Claude Code mode"));
    assert!(text.contains("    - sync"));
    assert!(text.contains("    + api_key"));
    assert!(text.contains("Claude Code credential"));
    assert!(text.contains("    - (unset)"));
    assert!(text.contains("    + (set)"));
    assert!(!text.contains("super-secret"), "{text}");
    assert!(!text.contains("ANTHROPIC_API_KEY ="), "{text}");
}

#[test]
fn workspace_save_preview_lists_source_folder_reset_to_default() {
    let original = WorkspaceConfig {
        workdir: "/repo".to_owned(),
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/workspace/claude")),
        }),
        ..Default::default()
    };
    let pending = WorkspaceConfig {
        workdir: "/repo".to_owned(),
        ..Default::default()
    };

    let text = edit_lines(original, pending);

    assert!(text.contains("Claude Code source folder"));
    assert!(text.contains("    - /workspace/claude"));
    assert!(text.contains("    + default: ~/.claude"));
}

#[test]
fn workspace_save_preview_lists_role_source_folder_change() {
    let original = WorkspaceConfig {
        workdir: "/repo".to_owned(),
        roles: [("smith".to_owned(), WorkspaceRoleOverride::default())].into(),
        ..Default::default()
    };
    let pending = WorkspaceConfig {
        workdir: "/repo".to_owned(),
        roles: [(
            "smith".to_owned(),
            WorkspaceRoleOverride {
                codex: Some(AgentAuthConfig {
                    auth_forward: AuthForwardMode::Sync,
                    sync_source_dir: Some(PathBuf::from("/role/codex")),
                }),
                ..Default::default()
            },
        )]
        .into(),
        ..Default::default()
    };

    let text = edit_lines(original, pending);

    assert!(text.contains("Role smith / Codex source folder"));
    assert!(text.contains("    - default: ~/.codex"));
    assert!(text.contains("    + /role/codex"));
}
