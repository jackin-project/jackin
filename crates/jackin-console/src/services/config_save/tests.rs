use std::path::PathBuf;

use jackin_config::{
    AgentAuthConfig, AuthForwardMode, EnvScope, EnvValue, GithubAuthConfig, GithubAuthMode,
    WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_core::Agent;

use super::{WorkspaceSaveDiffOp, workspace_save_diff_plan};
use crate::services::config_save::validate_settings_env;
use crate::tui::screens::settings::model::{SettingsEnvConfig, SettingsTrustRow};

#[test]
fn workspace_save_diff_plan_captures_auth_and_source_dir_changes() {
    let original = WorkspaceConfig::default();
    let mut pending = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            sync_source_dir: Some(PathBuf::from("/host/claude")),
        }),
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            env: Default::default(),
        }),
        ..Default::default()
    };
    pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            codex: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::OAuthToken,
                sync_source_dir: Some(PathBuf::from("/host/codex")),
            }),
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Ignore,
                env: Default::default(),
            }),
            ..Default::default()
        },
    );

    let ops = workspace_save_diff_plan("proj", &original, &pending);

    assert!(ops.contains(&WorkspaceSaveDiffOp::WorkspaceAuthForward {
        agent: Agent::Claude,
        mode: Some(AuthForwardMode::ApiKey),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::WorkspaceSyncSourceDir {
        agent: Agent::Claude,
        source: Some(PathBuf::from("/host/claude")),
    }));
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceGithubAuthForward {
            mode: Some(GithubAuthMode::Token),
        })
    );
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceRoleAuthForward {
            role: "smith".into(),
            agent: Agent::Codex,
            mode: Some(AuthForwardMode::OAuthToken),
        })
    );
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceRoleSyncSourceDir {
            role: "smith".into(),
            agent: Agent::Codex,
            source: Some(PathBuf::from("/host/codex")),
        })
    );
    assert!(
        ops.contains(&WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward {
            role: "smith".into(),
            mode: Some(GithubAuthMode::Ignore),
        })
    );
}

#[test]
fn workspace_save_diff_plan_captures_env_set_and_remove_for_layers() {
    let mut original = WorkspaceConfig::default();
    original
        .env
        .insert("OLD".into(), EnvValue::Plain("remove".into()));
    original
        .env
        .insert("KEEP".into(), EnvValue::Plain("same".into()));
    original.github = Some(GithubAuthConfig {
        env: [("GH_OLD".into(), EnvValue::Plain("remove".into()))].into(),
        ..Default::default()
    });
    original.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            env: [("ROLE_OLD".into(), EnvValue::Plain("remove".into()))].into(),
            github: Some(GithubAuthConfig {
                env: [("ROLE_GH_OLD".into(), EnvValue::Plain("remove".into()))].into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut pending = WorkspaceConfig::default();
    pending
        .env
        .insert("KEEP".into(), EnvValue::Plain("same".into()));
    pending
        .env
        .insert("NEW".into(), EnvValue::Plain("set".into()));
    pending.github = Some(GithubAuthConfig {
        env: [("GH_NEW".into(), EnvValue::Plain("set".into()))].into(),
        ..Default::default()
    });
    pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            env: [("ROLE_NEW".into(), EnvValue::Plain("set".into()))].into(),
            github: Some(GithubAuthConfig {
                env: [("ROLE_GH_NEW".into(), EnvValue::Plain("set".into()))].into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let ops = workspace_save_diff_plan("proj", &original, &pending);

    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvSet {
        scope: EnvScope::Workspace("proj".into()),
        key: "NEW".into(),
        value: EnvValue::Plain("set".into()),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvRemove {
        scope: EnvScope::Workspace("proj".into()),
        key: "OLD".into(),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvSet {
        scope: EnvScope::WorkspaceGithub("proj".into()),
        key: "GH_NEW".into(),
        value: EnvValue::Plain("set".into()),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvRemove {
        scope: EnvScope::WorkspaceGithub("proj".into()),
        key: "GH_OLD".into(),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvSet {
        scope: EnvScope::WorkspaceRole {
            workspace: "proj".into(),
            role: "smith".into(),
        },
        key: "ROLE_NEW".into(),
        value: EnvValue::Plain("set".into()),
    }));
    assert!(ops.contains(&WorkspaceSaveDiffOp::EnvRemove {
        scope: EnvScope::WorkspaceRoleGithub {
            workspace: "proj".into(),
            role: "smith".into(),
        },
        key: "ROLE_GH_OLD".into(),
    }));
}

#[test]
fn validate_settings_env_accepts_registered_roles_and_regular_keys() {
    let env = SettingsEnvConfig {
        env: [("PROJECT_ENV".to_owned(), "value")].into(),
        roles: [(
            "smith".to_owned(),
            [("ROLE_ENV".to_owned(), "value")].into(),
        )]
        .into(),
    };
    let roles = vec![SettingsTrustRow {
        role: "smith".into(),
        git: "builtin".into(),
        trusted: true,
    }];

    validate_settings_env(&env, &roles).unwrap();
}

#[test]
fn validate_settings_env_rejects_unregistered_role_keys() {
    let env = SettingsEnvConfig {
        env: Default::default(),
        roles: [(
            "unknown".to_owned(),
            [("ROLE_ENV".to_owned(), "value")].into(),
        )]
        .into(),
    };

    let error = validate_settings_env(&env, &[]).unwrap_err().to_string();

    assert!(error.contains("role \"unknown\" is not registered"));
}

#[test]
fn validate_settings_env_rejects_empty_and_reserved_keys() {
    let empty = SettingsEnvConfig {
        env: [(" ".to_owned(), "value")].into(),
        roles: Default::default(),
    };
    assert!(
        validate_settings_env(&empty, &[])
            .unwrap_err()
            .to_string()
            .contains("env var key cannot be empty")
    );

    let reserved = SettingsEnvConfig {
        env: [("JACKIN_WORKDIR".to_owned(), "value")].into(),
        roles: Default::default(),
    };
    assert!(
        validate_settings_env(&reserved, &[])
            .unwrap_err()
            .to_string()
            .contains("is reserved by the jackin runtime")
    );
}
