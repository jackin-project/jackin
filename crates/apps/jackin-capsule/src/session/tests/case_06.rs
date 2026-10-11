// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn google_alias_is_scrubbed_from_siblings_while_selected_credential_is_injected() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "gemini-work": {
                    "agent": "gemini",
                    "account_id": "acc-work",
                    "env": {"GEMINI_API_KEY": "work-secret"},
                },
                "gemini-personal": {
                    "agent": "gemini",
                    "account_id": "acc-personal",
                    "env": {"GEMINI_API_KEY": "personal-secret"},
                },
            },
        }))
        .expect("v2 fixture must decode");
    let ambient = vec![(
        jackin_core::GOOGLE_API_KEY_ENV_NAME.to_owned(),
        "ambient-secret".to_owned(),
    )];

    let mut unselected = build_agent_command(&spawn_spec(
        "gemini",
        "gemini-unselected",
        Some("ignore"),
        &ambient,
    ));
    apply_account_env(
        &mut unselected,
        "gemini-unselected",
        Some("ignore"),
        None,
        &credentials,
    );
    assert!(
        unselected
            .get_env(jackin_core::GOOGLE_API_KEY_ENV_NAME)
            .is_none()
    );
    assert!(
        unselected
            .get_env(jackin_core::GEMINI_API_KEY_ENV_NAME)
            .is_none()
    );

    let mut work = build_agent_command(&spawn_spec(
        "gemini",
        "gemini-work",
        Some("api_key"),
        &ambient,
    ));
    apply_account_env(
        &mut work,
        "gemini-work",
        Some("api_key"),
        Some("google"),
        &credentials,
    );
    assert_eq!(
        work.get_env(jackin_core::GEMINI_API_KEY_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("work-secret")
    );
    assert!(work.get_env(jackin_core::GOOGLE_API_KEY_ENV_NAME).is_none());

    let mut personal = build_agent_command(&spawn_spec(
        "gemini",
        "gemini-personal",
        Some("api_key"),
        &ambient,
    ));
    apply_account_env(
        &mut personal,
        "gemini-personal",
        Some("api_key"),
        Some("google"),
        &credentials,
    );
    assert_eq!(
        personal
            .get_env(jackin_core::GEMINI_API_KEY_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("personal-secret")
    );
    assert!(
        personal
            .get_env(jackin_core::GOOGLE_API_KEY_ENV_NAME)
            .is_none()
    );
}

#[test]
fn unassigned_instance_cannot_inherit_another_instances_provider_key() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "opencode-personal": {
                    "agent": "opencode",
                    "account_id": "acc-personal",
                    "env": {"OPENAI_API_KEY": "opencode-secret"},
                },
            },
        }))
        .expect("v2 fixture must decode");
    let empty: Vec<(String, String)> = Vec::new();
    let mut cmd = build_agent_command(&spawn_spec("codex", "codex-work", Some("ignore"), &empty));
    apply_account_env(&mut cmd, "codex-work", Some("ignore"), None, &credentials);
    assert!(cmd.get_env("OPENAI_API_KEY").is_none());
    assert!(!format!("{credentials:?}").contains("opencode-secret"));
}

#[test]
fn claude_session_owns_its_durable_config_directory_for_every_auth_mode() {
    let passthrough = vec![("CLAUDE_CONFIG_DIR".to_owned(), "/stale-profile".to_owned())];
    for mode in ["sync", "api_key", "oauth_token", "ignore"] {
        let command = build_agent_command(&spawn_spec(
            "claude",
            "claude-work",
            Some(mode),
            &passthrough,
        ));
        assert_eq!(
            command.get_env("CLAUDE_CONFIG_DIR"),
            Some(std::ffi::OsStr::new(
                jackin_core::container_paths::CLAUDE_CONFIG_DIR
            ))
        );
    }
}

#[test]
fn secondary_instance_gets_its_own_home_and_forwarded_dir() {
    let hostile = vec![
        ("CLAUDE_CONFIG_DIR".to_owned(), "/stale-profile".to_owned()),
        ("CODEX_HOME".to_owned(), "/foreign-codex".to_owned()),
        ("HOME".to_owned(), "/foreign-home".to_owned()),
    ];
    let spec = AgentSpawnSpec {
        agent: "claude",
        instance: "claude-personal",
        home_dir: "/home/agent/.claude-claude-personal",
        forwarded_dir: "/jackin/claude-claude-personal",
        model: None,
        effort: None,
        auth_mode: Some("sync"),
        env_passthrough: &hostile,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_001,
            gid: 2_001,
        },
    };
    let cmd = build_agent_command(&spec);
    let env = |name: &str| cmd.get_env(name).and_then(|v| v.to_str());
    assert_eq!(
        env("CLAUDE_CONFIG_DIR"),
        Some("/home/agent/.claude-claude-personal")
    );
    assert!(env("CODEX_HOME").is_none());
    assert_eq!(env("HOME"), Some("/home/agent/.claude-claude-personal"));
    assert_eq!(env(jackin_protocol::INSTANCE_ENV), Some("claude-personal"));
    assert_eq!(
        env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV),
        Some("/jackin/claude-claude-personal")
    );
    assert_eq!(env("JACKIN_AGENT"), Some("claude"));

    // A codex pane never inherits another runtime's folder var either.
    let spec = AgentSpawnSpec {
        agent: "codex",
        instance: "codex-work",
        home_dir: "/home/agent/.codex",
        forwarded_dir: "/jackin/codex",
        model: None,
        effort: None,
        auth_mode: Some("sync"),
        env_passthrough: &hostile,
        cwd: Path::new("/workspace"),
        codename: "test",
        identity: jackin_protocol::SessionIdentity {
            uid: 2_002,
            gid: 2_002,
        },
    };
    let cmd = build_agent_command(&spec);
    let env = |name: &str| cmd.get_env(name).and_then(|v| v.to_str());
    assert_eq!(env("CODEX_HOME"), Some("/home/agent/.codex"));
    assert_eq!(env("HOME"), Some("/home/agent/.codex"));
    assert!(env("CLAUDE_CONFIG_DIR").is_none());
}

#[test]
fn agent_home_matches_folder_var_target_for_parent_and_xdg_kinds() {
    // `HOME` echoes the instance home (the folder-var target) for every
    // folder-var kind — not just `Dir`. Values pinned by the slot-layout
    // tests; the spawn layer must carry them through unchanged.
    for (agent, home_dir, folder_var) in [
        ("gemini", "/home/agent", "GEMINI_CLI_HOME"),
        ("amp", "/home/agent/.local/share", "XDG_DATA_HOME"),
    ] {
        let spec = AgentSpawnSpec {
            agent,
            instance: "test-instance",
            home_dir,
            forwarded_dir: "/jackin/test-instance",
            model: None,
            effort: None,
            auth_mode: Some("sync"),
            env_passthrough: &[],
            cwd: Path::new("/workspace"),
            codename: "test",
            identity: jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        };
        let cmd = build_agent_command(&spec);
        let env = |name: &str| cmd.get_env(name).and_then(|v| v.to_str());
        assert_eq!(env(folder_var), Some(home_dir), "{agent} folder var");
        assert_eq!(env("HOME"), Some(home_dir), "{agent} HOME");
    }
}

#[test]
fn same_agent_instances_keep_model_home_endpoint_and_credential_bound_to_config_id() {
    let credentials: jackin_protocol::AgentCredentialEnv =
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "instances": {
                "codex-work": {
                    "agent": "codex",
                    "account_id": "openai-work",
                    "env": {
                        "OPENAI_API_KEY": "work-key",
                        "OPENAI_BASE_URL": "https://work.example.test/v1",
                    },
                },
                "codex-personal": {
                    "agent": "codex",
                    "account_id": "openai-personal",
                    "env": {
                        "OPENAI_API_KEY": "personal-key",
                        "OPENAI_BASE_URL": "https://personal.example.test/v1",
                    },
                },
            },
        }))
        .expect("v2 fixture must decode");
    let hostile_passthrough = vec![
        ("CODEX_HOME".to_owned(), "/foreign-codex".to_owned()),
        ("OPENAI_API_KEY".to_owned(), "ambient-key".to_owned()),
        (
            "OPENAI_BASE_URL".to_owned(),
            "https://ambient.example.test/v1".to_owned(),
        ),
        (
            jackin_core::CODEX_LANE_MODEL_ENV_NAME.to_owned(),
            "ambient-model".to_owned(),
        ),
        (
            jackin_core::CODEX_LANE_EFFORT_ENV_NAME.to_owned(),
            "high".to_owned(),
        ),
    ];
    let fixtures = [
        (
            "codex-work",
            "openai-work",
            "/home/agent/.codex",
            "/jackin/codex",
            "gpt-5.2-codex",
            "medium",
            "work-key",
            "https://work.example.test/v1",
        ),
        (
            "codex-personal",
            "openai-personal",
            "/home/agent/.codex-codex-personal",
            "/jackin/codex-codex-personal",
            "gpt-5.3-codex",
            "low",
            "personal-key",
            "https://personal.example.test/v1",
        ),
    ];

    for (instance_id, account_id, home_dir, forwarded_dir, model, effort, own_key, own_endpoint) in
        fixtures
    {
        let spec = AgentSpawnSpec {
            agent: "codex",
            instance: instance_id,
            home_dir,
            forwarded_dir,
            model: Some(model),
            effort: Some(effort),
            auth_mode: Some("api_key"),
            env_passthrough: &hostile_passthrough,
            cwd: Path::new("/workspace"),
            codename: "test",
            identity: jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        };
        let mut command = build_agent_command(&spec);
        apply_account_env(
            &mut command,
            instance_id,
            Some("api_key"),
            Some("codex"),
            &credentials,
        );
        let env = |name: &str| command.get_env(name).and_then(|value| value.to_str());
        let argv = command
            .get_argv()
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(env(jackin_protocol::INSTANCE_ENV), Some(instance_id));
        assert_eq!(env("CODEX_HOME"), Some(home_dir));
        assert_eq!(
            env(jackin_protocol::INSTANCE_FORWARDED_DIR_ENV),
            Some(forwarded_dir)
        );
        assert_eq!(env("OPENAI_API_KEY"), Some(own_key));
        assert_eq!(env("OPENAI_BASE_URL"), Some(own_endpoint));
        assert_eq!(env(jackin_core::CODEX_LANE_MODEL_ENV_NAME), Some(model));
        assert_eq!(env(jackin_core::CODEX_LANE_EFFORT_ENV_NAME), Some(effort));
        assert_eq!(argv[1..].to_vec(), vec!["-m".to_owned(), model.to_owned()]);
        assert_eq!(
            credentials
                .for_instance(instance_id)
                .and_then(|env| env.get("OPENAI_API_KEY"))
                .map(String::as_str),
            Some(own_key),
            "the fixture's account binding for {instance_id} must stay exact"
        );
        assert_eq!(
            credentials
                .instance(instance_id)
                .map(|entry| entry.account_id.as_str()),
            Some(account_id),
            "the fixture must name the account selected by {instance_id}"
        );
    }
}
