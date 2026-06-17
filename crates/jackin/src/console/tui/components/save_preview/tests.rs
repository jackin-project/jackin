use crate::config::{AgentAuthConfig, AppConfig, AuthForwardMode, WorkspaceRoleOverride};
use crate::console::tui::state::EditorState;
use crate::operator_env::EnvValue;
use crate::workspace::WorkspaceConfig;
use std::path::PathBuf;

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
    let mut editor = EditorState::new_edit("demo".to_owned(), original);
    editor.pending = pending;
    line_text(
        &jackin_console::tui::components::save_preview::build_workspace_save_lines(
            &editor,
            &config,
            &[],
        ),
    )
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
        "ANTHROPIC_API_KEY".to_owned(),
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
