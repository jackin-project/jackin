// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn workdir_boundary_rejects_root_ancestors_and_noncanonical_aliases() {
    for workdir in ["/", "/home", "/jackin", "/home/agent", "/workspace/../"] {
        let mut config = instance_config(&[]);
        config.workdir = workdir.to_owned();
        let error = validate(&config).unwrap_err();
        assert!(
            error.to_string().contains("protected root"),
            "unexpected rejection for {workdir}: {error}"
        );
    }
}

#[test]
fn workdir_boundary_preserves_workspace_and_rejects_private_mount_ancestors() {
    let valid = instance_config(&[("codex-work", "api_key", "codex")]);
    validate(&valid).unwrap();

    let mut invalid = valid;
    invalid.workdir = "/workspace".to_owned();
    invalid.instance_mount_paths.insert(
        "codex-work".to_owned(),
        vec![
            "/home/agent/.slot-0".to_owned(),
            "/jackin/slot-0".to_owned(),
            "/workspace/private-slot".to_owned(),
        ],
    );
    let error = validate(&invalid).unwrap_err();
    assert!(
        error.to_string().contains("protected mount destination"),
        "unexpected rejection: {error}"
    );
}

#[test]
fn workspace_mounts_and_git_targets_preserve_valid_config() {
    let mut config = instance_config(&[("codex-work", "api_key", "codex")]);
    config.workspace_mounts = vec!["/workspace/other".to_owned()];
    config.worktree_git_targets = vec!["/jackin/host/workspace/other/.git".to_owned()];
    validate(&config).unwrap();
}

#[test]
fn workspace_mounts_reject_protected_roots_and_private_mount_ancestors() {
    for dst in [
        "/",
        "/home",
        "/home/agent",
        "/jackin",
        "/jackin/run",
        "/workspace/../jackin",
    ] {
        let mut config = instance_config(&[("codex-work", "api_key", "codex")]);
        config.workspace_mounts = vec![dst.to_owned()];
        let error = validate(&config).unwrap_err();
        assert!(
            error.to_string().contains("protected"),
            "unexpected rejection for {dst}: {error}"
        );
    }

    let mut config = instance_config(&[("codex-work", "api_key", "codex")]);
    config.workdir = "/workspace/project".to_owned();
    config.workspace_mounts = vec!["/workspace".to_owned()];
    config.instance_mount_paths.insert(
        "codex-work".to_owned(),
        vec![
            "/home/agent/.slot-0".to_owned(),
            "/jackin/slot-0".to_owned(),
            "/workspace/private-slot".to_owned(),
        ],
    );
    let error = validate(&config).unwrap_err();
    assert!(
        error.to_string().contains("protected mount destination"),
        "unexpected rejection: {error}"
    );
}

#[test]
fn worktree_git_targets_must_be_strict_host_descendants() {
    for target in [
        "/jackin/run/x",
        "/home/agent/x",
        "/jackin/host",
        "/workspace/x",
        "/jackin/host/../run/x",
        "/jackin/host/a/../b",
        "relative/path",
        "",
    ] {
        let mut config = instance_config(&[("codex-work", "api_key", "codex")]);
        config.worktree_git_targets = vec![target.to_owned()];
        let error = validate(&config).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("outside")
                || message.contains("must not contain")
                || message.contains("absolute"),
            "unexpected rejection for {target}: {message}"
        );
    }
}

#[test]
fn instance_boundary_rejects_protected_root_aliases() {
    let mut invalid_home = instance_config(&[("codex-work", "api_key", "codex")]);
    invalid_home
        .instance_home_dirs
        .insert("codex-work".to_owned(), "/home/agent/".to_owned());
    assert!(validate(&invalid_home).is_err());

    let mut invalid_forwarded = instance_config(&[("codex-work", "api_key", "codex")]);
    invalid_forwarded
        .instance_forwarded_dirs
        .insert("codex-work".to_owned(), "/jackin/".to_owned());
    assert!(validate(&invalid_forwarded).is_err());

    let mut invalid_mount = instance_config(&[("codex-work", "api_key", "codex")]);
    invalid_mount.instance_mount_paths.insert(
        "codex-work".to_owned(),
        vec!["/home/agent/".to_owned(), "/jackin/slot-0".to_owned()],
    );
    assert!(validate(&invalid_mount).is_err());
}

#[test]
fn auth_modes_are_complete_bounded_and_allowlisted() {
    let valid = instance_config(&[("codex-work", "api_key", "codex")]);
    validate(&valid).unwrap();

    let mut invalid = valid.clone();
    invalid
        .auth_modes
        .insert("codex-work".to_owned(), "private-mode".to_owned());
    assert!(validate(&invalid).is_err());
    invalid.auth_modes = BTreeMap::from([("claude-work".to_owned(), "sync".to_owned())]);
    assert!(validate(&invalid).is_err());
}

#[test]
fn protected_credentials_reject_profile_mode_and_arbitrary_environment() {
    let mut config = instance_config(&[("claude-work", "sync", "claude")]);
    let credentials = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "fixture"},
            },
        },
    }));
    validate_agent_credentials(&config, &credentials).unwrap_err();
    config
        .auth_modes
        .insert("claude-work".into(), "api_key".into());
    validate_agent_credentials(&config, &credentials).unwrap();
    let invalid = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"LD_PRELOAD": "/evil"},
            },
        },
    }));
    validate_agent_credentials(&config, &invalid).unwrap_err();
}

#[test]
fn protected_credentials_reject_foreign_codex_provider_and_oauth_keys() {
    let mut config = instance_config(&[("codex-work", "api_key", "codex")]);
    config
        .credential_provider_surfaces
        .insert("codex-work".to_owned(), "zai".to_owned());
    let routed = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "codex-work": {
                "agent": "codex",
                "account_id": "acc-codex",
                "env": {
                    "OPENAI_API_KEY": "selected-zai-key",
                    "OPENAI_BASE_URL": "https://api.z.ai/api/v1"
                },
            },
        },
    }));
    validate_agent_credentials(&config, &routed).unwrap();

    let foreign = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "codex-work": {
                "agent": "codex",
                "account_id": "acc-codex",
                "env": {
                    "OPENAI_API_KEY": "selected-zai-key",
                    "CLAUDE_CODE_OAUTH_TOKEN": "foreign-claude-sentinel",
                    "GEMINI_API_KEY": "foreign-google-sentinel"
                },
            },
        },
    }));
    let error = validate_agent_credentials(&config, &foreign).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!error.to_string().contains("foreign-claude-sentinel"));

    config
        .credential_provider_surfaces
        .insert("codex-work".to_owned(), "kimi".to_owned());
    let routed_kimi = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "codex-work": {
                "agent": "codex",
                "account_id": "acc-codex",
                "env": {
                    "KIMI_API_KEY": "selected-kimi-key",
                    "OPENAI_BASE_URL": "https://api.kimi.com/coding/v1"
                },
            },
        },
    }));
    validate_agent_credentials(&config, &routed_kimi).unwrap();

    config.credential_provider_surfaces.clear();
    let missing_provider_surface = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "codex-work": {
                "agent": "codex",
                "account_id": "acc-codex",
                "env": {
                    "KIMI_API_KEY": "selected-kimi-key",
                    "OPENAI_API_KEY": "foreign-openai-sentinel"
                },
            },
        },
    }));
    assert!(validate_agent_credentials(&config, &missing_provider_surface).is_err());
}

#[test]
fn protected_credentials_reject_foreign_opencode_provider_and_oauth_keys() {
    let mut config = instance_config(&[("opencode-work", "api_key", "opencode")]);
    config
        .credential_provider_surfaces
        .insert("opencode-work".to_owned(), "claude".to_owned());
    let valid = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "opencode-work": {
                "agent": "opencode",
                "account_id": "opencode-work",
                "env": {"ANTHROPIC_API_KEY": "selected-anthropic-key"},
            },
        },
    }));
    validate_agent_credentials(&config, &valid).unwrap();

    let foreign = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "opencode-work": {
                "agent": "opencode",
                "account_id": "opencode-work",
                "env": {
                    "ANTHROPIC_API_KEY": "selected-anthropic-key",
                    "CLAUDE_CODE_OAUTH_TOKEN": "foreign-claude-sentinel",
                    "OPENAI_API_KEY": "foreign-codex-sentinel"
                },
            },
        },
    }));
    let error = validate_agent_credentials(&config, &foreign).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!error.to_string().contains("foreign-claude-sentinel"));
}

#[test]
fn protected_credentials_admit_moonshot_opencode_key_and_reject_foreign_key() {
    let mut config = instance_config(&[("opencode-kimi", "api_key", "opencode")]);
    config
        .credential_provider_surfaces
        .insert("opencode-kimi".to_owned(), "kimi".to_owned());
    let valid = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "opencode-kimi": {
                "agent": "opencode",
                "account_id": "opencode-kimi",
                "env": {
                    "MOONSHOT_API_KEY": "selected-moonshot-key"
                },
            },
        },
    }));
    validate_agent_credentials(&config, &valid).unwrap();

    let foreign = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "opencode-kimi": {
                "agent": "opencode",
                "account_id": "opencode-kimi",
                "env": {
                    "MOONSHOT_API_KEY": "selected-moonshot-key",
                    "OPENAI_API_KEY": "foreign-openai-sentinel"
                },
            },
        },
    }));
    let error = validate_agent_credentials(&config, &foreign).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!error.to_string().contains("foreign-openai-sentinel"));
}

#[test]
fn protected_credentials_bind_claude_oauth_to_its_auth_family() {
    let config = instance_config(&[("claude-work", "oauth_token", "claude")]);
    let valid = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {
                    "CLAUDE_CODE_OAUTH_TOKEN": "selected-oauth-token",
                    "ANTHROPIC_BASE_URL": "https://anthropic.example"
                },
            },
        },
    }));
    validate_agent_credentials(&config, &valid).unwrap();

    let foreign = v2_credentials(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {
                    "CLAUDE_CODE_OAUTH_TOKEN": "selected-oauth-token",
                    "OPENAI_API_KEY": "foreign-codex-sentinel"
                },
            },
        },
    }));
    let error = validate_agent_credentials(&config, &foreign).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!error.to_string().contains("foreign-codex-sentinel"));
}
