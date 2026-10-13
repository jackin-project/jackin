// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn edit_workspace_rejects_leaving_pre_existing_violation() {
    // A workspace already containing a rule-C violation. An unrelated edit
    // (e.g., adding an allowed role) should be blocked by the post-check.
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

    let mut config = AppConfig::default();
    config.insert_workspace_raw(
        "legacy",
        WorkspaceConfig {
            workdir: "/a".into(),
            mounts: vec![
                MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                },
                MountConfig {
                    src: "/a/b".into(),
                    dst: "/a/b".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                },
            ],
            ..Default::default()
        },
    );

    let err = config
        .edit_workspace(
            &wn("legacy"),
            WorkspaceEdit {
                allowed_roles_to_add: vec!["agent-x".into()],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("redundant") || msg.contains("already covered"),
        "expected 'redundant' or 'already covered' in error message, got: {msg}"
    );
}

#[test]
fn create_workspace_errors_on_child_under_parent_in_initial_mounts() {
    use {MountConfig, WorkspaceConfig};

    let mut config = AppConfig::default();
    let err = config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a".into(),
                mounts: vec![
                    MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("redundant") || msg.contains("already covered"),
        "expected 'redundant' or 'already covered' in error message, got: {msg}"
    );
}

#[test]
fn create_workspace_errors_on_readonly_mismatch_in_initial_mounts() {
    use {MountConfig, WorkspaceConfig};

    let mut config = AppConfig::default();
    let err = config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a".into(),
                mounts: vec![
                    MountConfig {
                        src: "/a".into(),
                        dst: "/a".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: true,
                        isolation: crate::MountIsolation::Shared,
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains("readonly"));
}

#[test]
fn create_workspace_accepts_already_collapsed_mount_set() {
    use {MountConfig, WorkspaceConfig};

    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a".into(),
                mounts: vec![MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();
}

#[test]
fn app_config_role_repo_refresh_ttl_defaults_when_absent() {
    let cfg: AppConfig = toml::from_str("").unwrap();

    assert_eq!(cfg.role_repo_refresh_ttl_seconds, None);
    assert_eq!(
        cfg.role_repo_refresh_ttl(),
        std::time::Duration::from_secs(DEFAULT_ROLE_REPO_REFRESH_TTL_SECONDS)
    );
}

#[test]
fn app_config_role_repo_refresh_ttl_accepts_zero() {
    let cfg: AppConfig = toml::from_str("role_repo_refresh_ttl_seconds = 0").unwrap();

    assert_eq!(cfg.role_repo_refresh_ttl_seconds, Some(0));
    assert_eq!(cfg.role_repo_refresh_ttl(), std::time::Duration::ZERO);
}

#[test]
fn reject_legacy_role_claude_block() {
    let toml = r#"
[roles.smith]
git = "git@example.com:smith.git"
trusted = true

[roles.smith.claude]
auth_forward = "ignore"
"#;
    let err = toml::from_str::<AppConfig>(toml).expect_err("must reject legacy block");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field `claude`") || msg.contains("unknown field \"claude\""),
        "expected unknown-field error for legacy [roles.X.claude] block, got: {msg}"
    );
}

#[test]
fn parse_app_config_with_global_github_block() {
    let toml = r#"
[github]
auth_forward = "sync"
"#;
    let cfg: AppConfig = toml::from_str(toml).unwrap();
    let g = cfg.github.as_ref().expect("[github] must parse");
    assert_eq!(g.auth_forward, GithubAuthMode::Sync);
    assert!(g.env.is_empty());
}

#[test]
fn parse_app_config_with_github_token_and_env() {
    let toml = r#"
[github]
auth_forward = "token"

[github.env]
GH_TOKEN = "$GH_TOKEN"
GH_HOST = "ghe.acme.com"
"#;
    let cfg: AppConfig = toml::from_str(toml).unwrap();
    let g = cfg.github.as_ref().unwrap();
    assert_eq!(g.auth_forward, GithubAuthMode::Token);
    assert!(g.env.contains_key("GH_TOKEN"));
    assert!(g.env.contains_key("GH_HOST"));
}

#[test]
fn parse_workspace_github_block() {
    let toml = r#"
[roles.smith]
git = "https://github.com/example/smith.git"

[workspaces.acme]
workdir = "/workspace/proj"

[[workspaces.acme.mounts]]
src = "/tmp/proj"
dst = "/workspace/proj"

[workspaces.acme.github]
auth_forward = "token"

[workspaces.acme.github.env]
GH_TOKEN = "op://Work/ACME/gh-pat"
"#;
    let cfg: AppConfig = toml::from_str(toml).unwrap();
    let ws = cfg.workspaces.get("acme").unwrap();
    let g = ws.github.as_ref().unwrap();
    assert_eq!(g.auth_forward, GithubAuthMode::Token);
    assert!(g.env.contains_key("GH_TOKEN"));
}

#[test]
fn parse_workspace_role_override_github_block() {
    let toml = r#"
[roles.smith]
git = "https://github.com/example/smith.git"

[workspaces.acme]
workdir = "/workspace/proj"

[[workspaces.acme.mounts]]
src = "/tmp/proj"
dst = "/workspace/proj"

[workspaces.acme.roles.smith.github]
auth_forward = "ignore"
"#;
    let cfg: AppConfig = toml::from_str(toml).unwrap();
    let ov = cfg
        .workspaces
        .get("acme")
        .and_then(|ws| ws.roles.get("smith"))
        .expect("override must exist");
    let g = ov.github.as_ref().unwrap();
    assert_eq!(g.auth_forward, GithubAuthMode::Ignore);
}

#[test]
fn github_auth_mode_default_is_sync() {
    assert_eq!(GithubAuthMode::default(), GithubAuthMode::Sync);
}

#[test]
fn github_auth_mode_from_str_round_trips() {
    use std::str::FromStr;
    assert_eq!(
        GithubAuthMode::from_str("sync").unwrap(),
        GithubAuthMode::Sync
    );
    assert_eq!(
        GithubAuthMode::from_str("token").unwrap(),
        GithubAuthMode::Token
    );
    assert_eq!(
        GithubAuthMode::from_str("ignore").unwrap(),
        GithubAuthMode::Ignore
    );
    GithubAuthMode::from_str("api_key").unwrap_err();
    GithubAuthMode::from_str("oauth_token").unwrap_err();
    GithubAuthMode::from_str("nope").unwrap_err();
}

#[test]
fn github_auth_mode_display_emits_canonical_names() {
    assert_eq!(GithubAuthMode::Sync.to_string(), "sync");
    assert_eq!(GithubAuthMode::Token.to_string(), "token");
    assert_eq!(GithubAuthMode::Ignore.to_string(), "ignore");
}

#[test]
fn github_auth_config_rejects_unknown_field() {
    let toml = r#"
auth_forward = "sync"
bogus = true
"#;
    let err = toml::from_str::<GithubAuthConfig>(toml).expect_err("unknown field must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field `bogus`") || msg.contains("unknown field \"bogus\""),
        "expected unknown-field error, got: {msg}"
    );
}

#[test]
fn resolve_github_mode_layered_precedence() {
    use crate::{WorkspaceConfig, WorkspaceRoleOverride};
    let mut cfg = AppConfig::default();
    // Default — Sync
    assert_eq!(
        resolve_github_mode(&cfg, Some(&wn("proj")), "smith"),
        GithubAuthMode::Sync
    );
    // Global only
    cfg.github = Some(GithubAuthConfig {
        auth_forward: GithubAuthMode::Ignore,
        env: BTreeMap::new(),
    });
    assert_eq!(
        resolve_github_mode(&cfg, Some(&wn("proj")), "smith"),
        GithubAuthMode::Ignore
    );
    // Workspace overrides global
    let ws = WorkspaceConfig {
        workdir: "/x".into(),
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Token,
            env: BTreeMap::new(),
        }),
        ..Default::default()
    };
    cfg.workspaces.insert("proj".into(), ws);
    assert_eq!(
        resolve_github_mode(&cfg, Some(&wn("proj")), "smith"),
        GithubAuthMode::Token
    );
    // Role override wins
    let ov = WorkspaceRoleOverride {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env: BTreeMap::new(),
        }),
        ..WorkspaceRoleOverride::default()
    };
    cfg.workspaces
        .get_mut("proj")
        .unwrap()
        .roles
        .insert("smith".into(), ov);
    assert_eq!(
        resolve_github_mode(&cfg, Some(&wn("proj")), "smith"),
        GithubAuthMode::Sync
    );
}

#[test]
fn deserializes_global_env_map() {
    let toml_str = r#"
[env]
OPERATOR_GLOBAL = "literal"
OPERATOR_SECRET = "op://Personal/api/token"
OPERATOR_HOST = "$HOME_VAR"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config
            .env
            .get("OPERATOR_GLOBAL")
            .unwrap()
            .as_persisted_str(),
        "literal"
    );
    assert_eq!(
        config
            .env
            .get("OPERATOR_SECRET")
            .unwrap()
            .as_persisted_str(),
        "op://Personal/api/token"
    );
    assert_eq!(
        config.env.get("OPERATOR_HOST").unwrap().as_persisted_str(),
        "$HOME_VAR"
    );
}
