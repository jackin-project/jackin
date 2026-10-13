// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn same_account_claude_oauth_instances_stage_distinct_endpoints_and_isolate_secrets() {
    let oauth_account = |name: &str, token: &str| AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::OAuthToken {
            agent: Agent::Claude,
            value: EnvValue::from(token),
        },
    };
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "shared".into(),
        oauth_account("Shared Claude", "shared-oauth"),
    );
    cfg.accounts.insert(
        "unselected".into(),
        oauth_account("Unselected Claude", "unselected-oauth"),
    );
    for (id, endpoint) in [
        ("claude-work", "https://work.example/v1"),
        ("claude-personal", "https://personal.example/v1"),
    ] {
        cfg.agent_configurations.insert(
            id.into(),
            configuration_with_endpoint(Agent::Claude, "shared", None, endpoint),
        );
    }

    let instances = launch(&cfg, &["claude-work", "claude-personal"]);
    let credentials = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();

    assert_eq!(credentials.schema_version(), 2);
    assert_eq!(credentials.iter().count(), 2);
    for (id, endpoint) in [
        ("claude-work", "https://work.example/v1"),
        ("claude-personal", "https://personal.example/v1"),
    ] {
        let envelope = credentials.instance(id).unwrap();
        assert_eq!(envelope.agent, "claude");
        assert_eq!(envelope.account_id, "shared");
        assert_eq!(
            envelope.env,
            BTreeMap::from([
                ("ANTHROPIC_BASE_URL".into(), endpoint.into()),
                ("CLAUDE_CODE_OAUTH_TOKEN".into(), "shared-oauth".into()),
            ])
        );
        assert!(
            !envelope
                .env
                .values()
                .any(|value| value == "unselected-oauth")
        );
    }
    assert!(!format!("{credentials:?}").contains("shared-oauth"));
    assert!(!format!("{credentials:?}").contains("unselected-oauth"));
}

#[test]
fn invalid_instance_endpoint_fails_before_secret_resolution() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "shared".into(),
        AccountConfig {
            enabled: true,
            name: "Shared Anthropic".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("$SHARED_TOKEN"),
                base_url: Some("https://account.example/v1".into()),
                model: None,
            },
        },
    );
    let instances = vec![jackin_config::ResolvedInstance {
        config_id: "claude".into(),
        agent: Agent::Claude,
        account_id: "shared".into(),
        model: None,
        base_url: Some("ftp://invalid.example/v1".into()),
        xdg_roots: None,
        label: "Claude".into(),
        synthesized: false,
    }];

    let error = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        panic!("invalid endpoint must not resolve a fallback secret")
    })
    .unwrap_err();
    assert!(error.to_string().contains("HTTP(S) endpoint"), "{error:?}");
}

#[test]
fn synthesized_config_id_used_verbatim() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Amp,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("test-key"),
                base_url: None,
                model: None,
            },
        },
    );
    let instances = jackin_config::resolve_launch(&cfg, None, "role", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "work@amp");
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(
        env.for_instance("work@amp").unwrap()["AMP_API_KEY"],
        "test-key"
    );
}

#[test]
fn empty_credential_rejected() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("work".into(), account("   "));
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Codex, "work"));
    let instances = launch(&cfg, &["primary"]);
    resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
}

#[test]
fn on_demand_credential_rejected() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        account_with_model(
            EnvValue::Extended(Extended {
                value: "test-key".into(),
                on_demand: true,
            }),
            None,
        ),
    );
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Codex, "work"));
    let instances = launch(&cfg, &["primary"]);
    let error = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
    assert!(
        error.to_string().contains("must resolve at launch"),
        "{error:?}"
    );
}

#[test]
fn generic_env_cannot_bypass_account_admission() {
    let mut cfg = AppConfig::default();
    for key in jackin_core::account_env_names() {
        cfg.env.insert(key.into(), EnvValue::from("test-bypass"));
    }
    cfg.env.insert("EDITOR".into(), EnvValue::from("vim"));
    let env = crate::resolve_operator_env_with(&cfg, None, None, &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(env, BTreeMap::from([("EDITOR".into(), "vim".into())]));
}

#[test]
fn selected_account_is_the_only_source_of_account_material() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work Claude".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("selected-account-secret"),
                base_url: None,
                model: None,
            },
        },
    );
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Claude, "work"));
    for key in jackin_core::account_env_names() {
        cfg.env
            .insert(key.into(), EnvValue::from(format!("ambient-{key}")));
    }

    let operator_env = crate::resolve_operator_env_with(&cfg, None, None, &NoSecrets, |_| {
        Ok("ambient-host-secret".into())
    })
    .unwrap();
    assert!(operator_env.is_empty());

    let credentials = resolve_instance_env_with(
        &cfg,
        &launch(&cfg, &["primary"]),
        None,
        "role",
        &NoSecrets,
        |_| Ok("ambient-host-secret".into()),
    )
    .unwrap();
    let instance = credentials.instance("primary").unwrap();
    assert_eq!(
        instance.env,
        BTreeMap::from([("ANTHROPIC_API_KEY".into(), "selected-account-secret".into())])
    );
    assert!(!format!("{credentials:?}").contains("ambient-"));
}

#[test]
fn missing_selected_account_secret_does_not_fall_back_to_ambient_auth() {
    let mut cfg = AppConfig::default();
    cfg.accounts
        .insert("work".into(), account("$SELECTED_WORK_TOKEN"));
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Codex, "work"));
    cfg.env
        .insert("OPENAI_API_KEY".into(), EnvValue::from("ambient-key"));
    cfg.env.insert(
        "OPENAI_BASE_URL".into(),
        EnvValue::from("https://ambient.example/v1"),
    );

    let operator_env = crate::resolve_operator_env_with(&cfg, None, None, &NoSecrets, |name| {
        Ok(format!("host-{name}"))
    })
    .unwrap();
    assert!(operator_env.is_empty());

    let error = resolve_instance_env_with(
        &cfg,
        &launch(&cfg, &["primary"]),
        None,
        "role",
        &NoSecrets,
        |name| {
            assert_eq!(name, "SELECTED_WORK_TOKEN");
            Err(std::env::VarError::NotPresent)
        },
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("SELECTED_WORK_TOKEN"),
        "{error:#}"
    );
}

#[test]
fn account_provider_cannot_authenticate_an_incompatible_agent() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("work".into(), account("$WORK_TOKEN"));
    let instance = jackin_config::ResolvedInstance {
        config_id: "claude-work".into(),
        agent: Agent::Claude,
        account_id: "work".into(),
        model: None,
        base_url: Some("https://api.openai.com/v1".into()),
        xdg_roots: None,
        label: "Claude with OpenAI account".into(),
        synthesized: false,
    };

    let error = resolve_instance_env_with(&cfg, &[instance], None, "role", &NoSecrets, |_| {
        panic!("provider/client mismatch must fail before secret resolution")
    })
    .unwrap_err();
    assert!(
        error.to_string().contains("cannot authenticate claude"),
        "{error:#}"
    );
}
