//! Tests for `config`.
use super::*;
use crate::{
    GithubAuthMode, MountConfig, MountEntry, resolve_github_mode, resolve_mode,
    validate_workspace_config,
};
use jackin_core::JackinPaths;
use jackin_core::WorkspaceName;
use tempfile::tempdir;

fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

#[test]
fn deserializes_scoped_docker_mounts() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "~/.chainargos/secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "~/.chainargos/brown", dst = "/config" }
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let mounts = &config.docker.mounts;
    match mounts.get("chainargos/*").unwrap() {
        MountEntry::Scoped(scope) => {
            let m = scope.get("chainargos-secrets").unwrap();
            assert_eq!(m.dst, "/secrets");
            assert!(m.readonly);
        }
        MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
    }
    match mounts.get("chainargos/agent-brown").unwrap() {
        MountEntry::Scoped(scope) => {
            let m = scope.get("brown-config").unwrap();
            assert_eq!(m.dst, "/config");
            assert!(!m.readonly);
        }
        MountEntry::Mount(_) => panic!("expected MountEntry::Scoped"),
    }
}

#[test]
fn deserializes_saved_workspaces() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/Users/donbeave/Projects/chainargos/big-monorepo"
default_role = "agent-smith"
allowed_roles = ["agent-smith", "chainargos/the-architect"]

[[workspaces.big-monorepo.mounts]]
src = "/Users/donbeave/Projects/chainargos/big-monorepo"
dst = "/Users/donbeave/Projects/chainargos/big-monorepo"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/cache"
dst = "/workspace/cache"
readonly = true
"#;

    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let workspace = config.workspaces.get("big-monorepo").unwrap();

    assert_eq!(
        workspace.workdir,
        "/Users/donbeave/Projects/chainargos/big-monorepo"
    );
    assert_eq!(workspace.mounts.len(), 2);
    assert_eq!(workspace.default_role.as_deref(), Some("agent-smith"));
    assert_eq!(workspace.allowed_roles.len(), 2);
    assert!(workspace.mounts[1].readonly);
}

#[test]
fn deserializes_global_telemetry_config() {
    let toml_str = r#"
[telemetry]
level = "trace"
categories = ["docker", "launch"]
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();

    assert_eq!(
        config.telemetry.level,
        Some(crate::TelemetryLevelConfig::Trace)
    );
    assert_eq!(config.telemetry.categories, vec!["docker", "launch"]);
}

#[test]
fn default_telemetry_config_is_not_serialized() {
    let config = AppConfig::default();
    let toml = toml::to_string_pretty(&config).unwrap();

    assert!(!toml.contains("[telemetry]"), "{toml}");
}

#[test]
fn rejects_workspace_with_workdir_outside_mounts() {
    let temp = tempdir().unwrap();

    let workspace = WorkspaceConfig {
        workdir: "/workspace/project".to_owned(),
        mounts: vec![MountConfig {
            src: temp.path().display().to_string(),
            dst: "/workspace/src".to_owned(),
            readonly: false,
            isolation: crate::MountIsolation::Shared,
        }],
        ..Default::default()
    };

    let error =
        validate_workspace_config(&WorkspaceName::parse("big-monorepo").unwrap(), &workspace)
            .unwrap_err();

    assert!(error.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn edit_workspace_does_not_persist_invalid_mutation() {
    use crate::WorkspaceEdit;
    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    let src = temp.path().display().to_string();

    config
        .create_workspace(
            &WorkspaceName::parse("big-monorepo").unwrap(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_owned(),
                mounts: vec![MountConfig {
                    src,
                    dst: "/workspace/project".to_owned(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    let error = config
        .edit_workspace(
            &wn("big-monorepo"),
            WorkspaceEdit {
                workdir: Some("/workspace/missing".to_owned()),
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(error.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
    assert_eq!(
        config.workspaces.get("big-monorepo").unwrap().workdir,
        "/workspace/project"
    );
}

#[test]
fn load_or_init_rejects_invalid_saved_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        &paths.config_file,
        r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp"
dst = "/workspace/src"
"#,
    )
    .unwrap();

    let error = AppConfig::load_or_init(&paths).unwrap_err();

    assert!(error.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn load_or_init_rejects_invalid_persisted_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mount_src = temp.path().join("workspace-src");
    std::fs::create_dir_all(&mount_src).unwrap();

    let toml_str = format!(
        r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.broken]
workdir = "/workspace/project"

[[workspaces.broken.mounts]]
src = "{}"
dst = "/workspace/src"
"#,
        mount_src.display()
    );

    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, toml_str).unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    assert!(err.to_string().contains("workspace \"broken\" workdir must be equal to, inside, or a parent of one of the workspace mount destinations"));
}

#[test]
fn existing_config_without_claude_section_deserializes_with_defaults() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(
        config.claude.is_none(),
        "absent [claude] block must deserialize to None"
    );
    assert_eq!(
        resolve_mode(&config, Agent::Claude, None, "agent-smith",),
        AuthForwardMode::Sync
    );
}

#[test]
fn auth_forward_mode_from_str_accepts_oauth_token() {
    use std::str::FromStr;
    assert_eq!(
        AuthForwardMode::from_str("oauth_token").unwrap(),
        AuthForwardMode::OAuthToken
    );
}

#[test]
fn auth_forward_mode_display_emits_oauth_token() {
    assert_eq!(AuthForwardMode::OAuthToken.to_string(), "oauth_token");
}

#[test]
fn auth_forward_mode_deserializes_oauth_token() {
    let toml_str = r#"
[claude]
auth_forward = "oauth_token"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.claude.as_ref().unwrap().auth_forward,
        AuthForwardMode::OAuthToken
    );
}

#[test]
fn parse_app_config_agent_auth_blocks() {
    let toml = r#"
[claude]
auth_forward = "sync"

[codex]
auth_forward = "api_key"

[amp]
auth_forward = "ignore"
"#;
    let cfg: AppConfig = toml::from_str(toml).unwrap();
    assert_eq!(
        cfg.claude.as_ref().unwrap().auth_forward,
        AuthForwardMode::Sync
    );
    assert_eq!(
        cfg.codex.as_ref().unwrap().auth_forward,
        AuthForwardMode::ApiKey
    );
    assert_eq!(
        cfg.amp.as_ref().unwrap().auth_forward,
        AuthForwardMode::Ignore
    );
}

#[test]
fn parse_app_config_no_agent_blocks() {
    let toml = "";
    let cfg: AppConfig = toml::from_str(toml).unwrap();
    assert!(
        cfg.claude.is_none(),
        "claude must be None when [claude] absent"
    );
    assert!(
        cfg.codex.is_none(),
        "codex must be None when [codex] absent"
    );
    assert!(cfg.amp.is_none(), "amp must be None when [amp] absent");
    assert!(
        cfg.opencode.is_none(),
        "opencode must be None when [opencode] absent"
    );
}

#[test]
fn reject_codex_oauth_token_global() {
    // Phase 3: oauth_token is now rejected by validate_auth_modes() after
    // parse (not at serde time) since the newtype validator is gone.
    let toml = r#"
[codex]
auth_forward = "oauth_token"
"#;
    let cfg = toml::from_str::<AppConfig>(toml).expect("parse should succeed");
    let err = cfg.validate_auth_modes().expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported for codex"),
        "expected codex-rejection message, got: {msg}"
    );
}

#[test]
fn reject_amp_oauth_token_global() {
    // Phase 3: same as codex — post-parse validation replaces serde newtype check.
    let toml = r#"
[amp]
auth_forward = "oauth_token"
"#;
    let cfg = toml::from_str::<AppConfig>(toml).expect("parse should succeed");
    let err = cfg.validate_auth_modes().expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported for amp"),
        "expected amp-rejection message, got: {msg}"
    );
}

#[test]
fn auth_forward_mode_from_str_error_lists_oauth_token() {
    use std::str::FromStr;
    let err = AuthForwardMode::from_str("nope").unwrap_err();
    assert!(
        err.contains("oauth_token"),
        "error message should advertise the oauth_token mode; got: {err}"
    );
}

#[test]
fn edit_workspace_rejects_upsert_that_introduces_child_under_existing_parent() {
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

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

    let err = config
        .edit_workspace(
            &wn("test"),
            WorkspaceEdit {
                upsert_mounts: vec![MountConfig {
                    src: "/a/b".into(),
                    dst: "/a/b".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("already covered") || msg.contains("redundant"),
        "expected 'already covered' or 'redundant' in error message, got: {msg}"
    );
}

#[test]
fn edit_workspace_rejects_upsert_with_readonly_mismatch_vs_existing_child() {
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a/b".into(),
                mounts: vec![MountConfig {
                    src: "/a/b".into(),
                    dst: "/a/b".into(),
                    readonly: true,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        )
        .unwrap();

    let err = config
        .edit_workspace(
            &wn("test"),
            WorkspaceEdit {
                upsert_mounts: vec![MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap_err();

    assert!(err.to_string().contains("readonly"));
}

#[test]
fn edit_workspace_accepts_pre_collapsed_upsert_that_replaces_children() {
    // CLI's job is to pre-collapse. Here we simulate it: instead of
    // upserting just the parent (which would leave children as redundants
    // and fail the post-condition), the CLI removes the children via
    // remove_destinations AND upserts the parent in the same edit.
    use crate::{MountConfig, WorkspaceConfig, WorkspaceEdit};

    let mut config = AppConfig::default();
    config
        .create_workspace(
            &WorkspaceName::parse("test").unwrap(),
            WorkspaceConfig {
                workdir: "/a/b".into(),
                mounts: vec![
                    MountConfig {
                        src: "/a/b".into(),
                        dst: "/a/b".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                    MountConfig {
                        src: "/a/c".into(),
                        dst: "/a/c".into(),
                        readonly: false,
                        isolation: crate::MountIsolation::Shared,
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();

    config
        .edit_workspace(
            &wn("test"),
            WorkspaceEdit {
                upsert_mounts: vec![MountConfig {
                    src: "/a".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::MountIsolation::Shared,
                }],
                remove_destinations: vec!["/a/b".into(), "/a/c".into()],
                ..WorkspaceEdit::default()
            },
        )
        .unwrap();

    let ws = config
        .list_workspaces()
        .into_iter()
        .find(|(n, _)| *n == "test")
        .map(|(_, w)| w)
        .expect("workspace should exist");
    assert_eq!(ws.mounts.len(), 1);
    assert_eq!(ws.mounts[0].src, "/a");
}

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
fn auth_forward_mode_default_is_sync() {
    assert_eq!(AuthForwardMode::default(), AuthForwardMode::Sync);
}

#[test]
fn auth_forward_mode_from_str_accepts_sync_and_ignore() {
    use std::str::FromStr;
    assert_eq!(
        AuthForwardMode::from_str("sync").unwrap(),
        AuthForwardMode::Sync
    );
    assert_eq!(
        AuthForwardMode::from_str("ignore").unwrap(),
        AuthForwardMode::Ignore
    );
}

#[test]
fn auth_forward_mode_from_str_rejects_unknown_values() {
    use std::str::FromStr;
    AuthForwardMode::from_str("bogus").unwrap_err();
}

#[test]
fn auth_forward_mode_display_emits_canonical_names() {
    assert_eq!(AuthForwardMode::Sync.to_string(), "sync");
    assert_eq!(AuthForwardMode::Ignore.to_string(), "ignore");
    assert_eq!(AuthForwardMode::ApiKey.to_string(), "api_key");
    assert_eq!(AuthForwardMode::OAuthToken.to_string(), "oauth_token");
}

#[test]
fn parse_agent_auth_config_sync() {
    let toml = r#"auth_forward = "sync""#;
    let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.auth_forward, AuthForwardMode::Sync);
}

#[test]
fn parse_agent_auth_config_api_key() {
    let toml = r#"auth_forward = "api_key""#;
    let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.auth_forward, AuthForwardMode::ApiKey);
}

#[test]
fn parse_agent_auth_config_oauth_token() {
    let toml = r#"auth_forward = "oauth_token""#;
    let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.auth_forward, AuthForwardMode::OAuthToken);
}

#[test]
fn parse_agent_auth_config_ignore() {
    let toml = r#"auth_forward = "ignore""#;
    let cfg: AgentAuthConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.auth_forward, AuthForwardMode::Ignore);
}

#[test]
fn agent_auth_config_serializes_canonical_names() {
    for (mode, expected) in [
        (AuthForwardMode::Sync, "sync"),
        (AuthForwardMode::ApiKey, "api_key"),
        (AuthForwardMode::OAuthToken, "oauth_token"),
        (AuthForwardMode::Ignore, "ignore"),
    ] {
        let cfg = AgentAuthConfig {
            auth_forward: mode,
            ..Default::default()
        };
        let s = toml::to_string(&cfg).expect("serialize must succeed");
        assert!(
            s.contains(&format!("auth_forward = \"{expected}\"")),
            "mode {mode:?} must serialize as auth_forward = \"{expected}\", got: {s}"
        );
    }
}

#[test]
fn agent_auth_config_rejects_unknown_field() {
    let toml = "auth_forward = \"sync\"\nbogus = true";
    let err = toml::from_str::<AgentAuthConfig>(toml).expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field `bogus`") || msg.contains("unknown field \"bogus\""),
        "expected unknown-field error, got: {msg}"
    );
}

/// `oauth_token` is no longer a field on `AgentAuthConfig` — credentials
/// live in the `[env]` block. Configs that still carry the old field
/// are rejected by `deny_unknown_fields`.
#[test]
fn agent_auth_config_rejects_legacy_oauth_token_field() {
    let toml = "auth_forward = \"oauth_token\"\noauth_token = \"sk-ant-oat01-literal\"";
    let err = toml::from_str::<AgentAuthConfig>(toml).expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field"),
        "expected unknown-field rejection, got: {msg}"
    );
}

/// `oauth_token` is not a field on `AgentAuthConfig` — unknown fields are rejected.
#[test]
fn codex_auth_config_rejects_oauth_token_field() {
    let toml = "auth_forward = \"api_key\"\noauth_token = \"doesnt-belong\"";
    let err = toml::from_str::<AgentAuthConfig>(toml).expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field"),
        "expected unknown-field rejection, got: {msg}"
    );
}

/// Same rejection through the top-level `AppConfig` parse path.
#[test]
fn reject_codex_oauth_token_field_at_app_config_layer() {
    let toml = "[codex]\nauth_forward = \"api_key\"\noauth_token = \"wrong-place\"";
    let err = toml::from_str::<AppConfig>(toml).expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown field"),
        "expected unknown-field rejection at AppConfig layer, got: {msg}"
    );
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
fn agent_auth_config_serializes_without_extraneous_fields() {
    let cfg = AgentAuthConfig {
        auth_forward: AuthForwardMode::Sync,
        ..Default::default()
    };
    let s = toml::to_string(&cfg).unwrap();
    assert!(
        !s.contains("oauth_token"),
        "serialized config must not contain oauth_token, got:\n{s}"
    );
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

// ── GitHub auth schema ──────────────────────────────────────────────

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

#[test]
fn deserializes_per_agent_env_map() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.env]
AGENT_TOKEN = "op://Shared/smith/token"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let role = config.roles.get("agent-smith").unwrap();
    assert_eq!(
        role.env.get("AGENT_TOKEN").unwrap().as_persisted_str(),
        "op://Shared/smith/token"
    );
}

#[test]
fn deserializes_per_workspace_env_map() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.env]
WORKSPACE_VAR = "literal"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let ws = config.workspaces.get("big-monorepo").unwrap();
    assert_eq!(
        ws.env.get("WORKSPACE_VAR").unwrap().as_persisted_str(),
        "literal"
    );
}

#[test]
fn deserializes_workspace_agent_override_env() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.roles.agent-smith.env]
PER_WORKSPACE_PER_AGENT = "specific"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let ws = config.workspaces.get("big-monorepo").unwrap();
    let override_ = ws.roles.get("agent-smith").unwrap();
    assert_eq!(
        override_
            .env
            .get("PER_WORKSPACE_PER_AGENT")
            .unwrap()
            .as_persisted_str(),
        "specific"
    );
}

#[test]
fn env_maps_default_to_empty_when_omitted() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(config.env.is_empty());
    assert!(config.roles.get("agent-smith").unwrap().env.is_empty());
}

#[test]
fn deserializes_agent_with_slash_in_name_using_quoted_keys() {
    // The spec calls out `[roles."chainargos/agent-jones".env]`
    // and `[workspaces.<ws>.roles."chainargos/agent-jones".env]`
    // as the TOML shape for third-party role selectors that
    // include a `/`. Standard TOML quoted keys suffice — this
    // test locks in that shape so a future refactor does not
    // accidentally require un-quoted identifiers.
    let toml_str = r#"
[roles."chainargos/agent-jones"]
git = "https://github.com/chainargos/jackin-agent-jones.git"

[roles."chainargos/agent-jones".env]
DATABASE_URL = "op://Work/agent-jones/db"

[workspaces.big-monorepo]
workdir = "/workspace/project"

[[workspaces.big-monorepo.mounts]]
src = "/tmp/src"
dst = "/workspace/project"

[workspaces.big-monorepo.roles."chainargos/agent-jones".env]
OPENAI_API_KEY = "op://Work/big-monorepo/OpenAI"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let role = config.roles.get("chainargos/agent-jones").unwrap();
    assert_eq!(
        role.env.get("DATABASE_URL").unwrap().as_persisted_str(),
        "op://Work/agent-jones/db"
    );
    let ws = config.workspaces.get("big-monorepo").unwrap();
    let override_ = ws.roles.get("chainargos/agent-jones").unwrap();
    assert_eq!(
        override_
            .env
            .get("OPENAI_API_KEY")
            .unwrap()
            .as_persisted_str(),
        "op://Work/big-monorepo/OpenAI"
    );
}

#[test]
fn git_config_coauthor_trailer_round_trips() {
    let toml_str = "[git]\ncoauthor_trailer = true\n";
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(config.git.coauthor_trailer);
    let serialized = toml::to_string(&config).unwrap();
    assert!(
        serialized.contains("coauthor_trailer = true"),
        "{serialized}"
    );
}

#[test]
fn git_config_default_omits_git_table_from_serialized_output() {
    let config = AppConfig::default();
    assert!(!config.git.coauthor_trailer);
    assert!(!config.git.dco);
    let serialized = toml::to_string(&config).unwrap();
    assert!(!serialized.contains("[git]"), "{serialized}");
    assert!(!serialized.contains("coauthor_trailer"), "{serialized}");
    assert!(!serialized.contains("dco"), "{serialized}");
}
