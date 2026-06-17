use super::*;
use crate::agent::Agent;
use crate::config::{AgentAuthConfig, AuthForwardMode, WorkspaceRoleOverride};
use crate::workspace::WorkspaceConfig;
use std::path::PathBuf;

#[test]
fn editor_source_folder_display_marks_inherited_and_default_paths() {
    let mut cfg = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/global/claude")),
        }),
        ..Default::default()
    };
    cfg.workspaces.insert(
        "proj".into(),
        WorkspaceConfig {
            roles: [("smith".into(), WorkspaceRoleOverride::default())].into(),
            ..Default::default()
        },
    );

    let workspace = editor_source_folder_display(
        &cfg,
        "proj",
        "",
        jackin_console::tui::auth::AuthKind::Claude,
    );
    assert_eq!(workspace.kind, AuthSourceFolderKind::Inherited);
    assert_eq!(workspace.path, "/global/claude");

    let role = editor_source_folder_display(
        &cfg,
        "proj",
        "smith",
        jackin_console::tui::auth::AuthKind::Claude,
    );
    assert_eq!(role.kind, AuthSourceFolderKind::Inherited);
    assert_eq!(role.path, "/global/claude");

    cfg.claude = None;
    let default = editor_source_folder_display(
        &cfg,
        "proj",
        "",
        jackin_console::tui::auth::AuthKind::Claude,
    );
    assert_eq!(default.kind, AuthSourceFolderKind::Default);
    assert_eq!(
        default.path,
        format!("~/{}", Agent::Claude.runtime().state_paths().credential_dir)
    );
}

#[test]
fn editor_source_folder_display_prefers_explicit_role_path() {
    let mut cfg = AppConfig::default();
    let mut workspace = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            sync_source_dir: Some(PathBuf::from("/workspace/claude")),
        }),
        ..Default::default()
    };
    workspace.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Sync,
                sync_source_dir: Some(PathBuf::from("/role/claude")),
            }),
            ..Default::default()
        },
    );
    cfg.workspaces.insert("proj".into(), workspace);

    let display = editor_source_folder_display(
        &cfg,
        "proj",
        "smith",
        jackin_console::tui::auth::AuthKind::Claude,
    );

    assert_eq!(display.kind, AuthSourceFolderKind::Explicit);
    assert_eq!(display.path, "/role/claude");
}
