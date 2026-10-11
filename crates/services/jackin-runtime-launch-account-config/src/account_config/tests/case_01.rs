// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn selected_opencode_account_pairs_endpoint_key_and_model() {
    for provider in [
        AiProvider::Anthropic,
        AiProvider::OpenAi,
        AiProvider::Xai,
        AiProvider::Moonshot,
        AiProvider::Zai,
        AiProvider::Minimax,
        AiProvider::Opencode,
        AiProvider::OpenRouter,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.accounts.insert(
            "work".into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "Work".into(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: "fixture-private-key".into(),
                    base_url: Some("https://provider.example/v1".into()),
                    model: Some("custom-model".into()),
                },
            },
        );
        let instances = [instance(
            "opencode-work",
            Agent::Opencode,
            "work",
            Some("custom-model"),
            Some("https://provider.example/v1"),
        )];
        configure_for_test(temp.path(), &config, &instances).unwrap();
        let contents = std::fs::read_to_string(
            temp.path()
                .join("provider-config/home/.config/opencode/opencode.json"),
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        let (id, _, _) = opencode_provider(provider).unwrap();
        assert_eq!(parsed["enabled_providers"], serde_json::json!([id]));
        assert_eq!(parsed["model"], format!("{id}/custom-model"));
        assert_eq!(
            parsed["provider"][id]["options"]["baseURL"],
            "https://provider.example/v1"
        );
        assert!(
            parsed["provider"][id]["options"]["apiKey"]
                .as_str()
                .unwrap()
                .starts_with("{env:")
        );
        assert!(
            parsed["provider"][id]["models"]
                .get("custom-model")
                .is_some()
        );
        assert!(!contents.contains("fixture-private-key"));
        assert_eq!(
            opencode_model(provider, &format!("{id}/custom-model")).unwrap(),
            format!("{id}/custom-model")
        );
    }
}

#[test]
fn openrouter_pin_lands_byte_exact_in_opencode_json() {
    const PIN: &str = "openrouter/anthropic/claude-sonnet-4";
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "or-model".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "or-model".into(),
            provider: AiProvider::OpenRouter,
            credential: AccountCredential::ApiKey {
                value: "fixture-or-key".into(),
                base_url: None,
                model: Some(PIN.into()),
            },
        },
    );
    let instances = [instance(
        "oc-plain",
        Agent::Opencode,
        "or-model",
        Some(PIN),
        None,
    )];
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let contents = std::fs::read_to_string(
        temp.path()
            .join("provider-config/home/.config/opencode/opencode.json"),
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(parsed["model"], PIN);
    assert!(
        parsed["provider"]["openrouter"]["models"]
            .get("anthropic/claude-sonnet-4")
            .is_some()
    );
}

#[test]
fn selected_coding_provider_has_model_protocol_and_no_stored_secret() {
    for (provider, model, key, context) in [
        (AiProvider::Moonshot, "k3-256k", "KIMI_API_KEY", 262_144),
        (AiProvider::Zai, "glm-5.3", "OPENAI_API_KEY", 1_048_576),
        (
            AiProvider::Minimax,
            "MiniMax-M3",
            "MINIMAX_API_KEY",
            1_000_000,
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.accounts.insert(
            "work".into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "Work".into(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: "fixture-private-key".into(),
                    base_url: None,
                    model: Some(model.into()),
                },
            },
        );
        let instances = [instance(
            "codex-work",
            Agent::Codex,
            "work",
            Some(model),
            None,
        )];
        configure_for_test(temp.path(), &config, &instances).unwrap();
        let contents =
            std::fs::read_to_string(temp.path().join("provider-config/home/.codex/config.toml"))
                .unwrap();
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        assert_eq!(parsed["model"].as_str(), Some(model));
        assert_eq!(
            parsed["model_providers"]["jackin_account"]["env_key"].as_str(),
            Some(key)
        );
        assert_eq!(
            parsed["model_providers"]["jackin_account"]["wire_api"].as_str(),
            Some("responses")
        );
        assert!(!contents.contains("fixture-private-key"));
        if provider == AiProvider::Minimax {
            assert_eq!(
                parsed["model_providers"]["jackin_account"]["base_url"].as_str(),
                Some("https://api.minimax.io/v1")
            );
        }
        let catalog_target = parsed["model_catalog_json"].as_str().unwrap().to_owned();
        let catalog_name = Path::new(&catalog_target)
            .file_name()
            .expect("catalog target has a file name")
            .to_owned();
        let catalog: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                temp.path()
                    .join("provider-config/home/.codex")
                    .join(catalog_name),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            catalog["models"][0]["context_window"].as_i64(),
            Some(context)
        );
        if provider == AiProvider::Minimax {
            assert_eq!(
                catalog["models"][0]["supported_reasoning_levels"][0]["effort"],
                "none"
            );
            assert_eq!(
                catalog["models"][0]["input_modalities"],
                serde_json::json!(["text", "image"])
            );
        }
    }
}

#[test]
fn configuration_model_override_wins_over_account_default() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: "fixture-private-key".into(),
                base_url: None,
                model: Some("k3".into()),
            },
        },
    );
    let instances = [instance(
        "codex-work",
        Agent::Codex,
        "work",
        Some("k3-256k"),
        None,
    )];
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let contents =
        std::fs::read_to_string(temp.path().join("provider-config/home/.codex/config.toml"))
            .unwrap();
    let parsed: toml::Value = toml::from_str(&contents).unwrap();
    assert_eq!(parsed["model"].as_str(), Some("k3-256k"));
}

#[test]
fn codex_configuration_model_override_routes_without_account_model() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: "work-secret".into(),
                base_url: Some("https://account.example/v1".into()),
                model: None,
            },
        },
    );
    let instances = [instance(
        "codex-work",
        Agent::Codex,
        "work",
        Some("k3-256k"),
        Some("https://route.example/v1"),
    )];

    // Empty here is intentional: the writer must preserve the effective
    // ResolvedInstance model instead of silently falling back to the account.
    configure_with_models(temp.path(), &config, &instances, &BTreeMap::new()).unwrap();
    let contents =
        std::fs::read_to_string(temp.path().join("provider-config/home/.codex/config.toml"))
            .unwrap();
    let parsed: toml::Value = toml::from_str(&contents).unwrap();
    assert_eq!(parsed["model"].as_str(), Some("k3-256k"));
    assert_eq!(
        parsed["model_providers"]["jackin_account"]["base_url"].as_str(),
        Some("https://route.example/v1")
    );
    assert_eq!(
        parsed["model_providers"]["jackin_account"]["env_key"].as_str(),
        Some("KIMI_API_KEY")
    );
    assert!(!contents.contains("work-secret"));
}

#[test]
fn opencode_cli_model_routes_without_account_model() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: "work-secret".into(),
                base_url: Some("https://account.example/v1".into()),
                model: None,
            },
        },
    );
    let instances = [instance(
        "opencode-work",
        Agent::Opencode,
        "work",
        None,
        Some("https://route.example/v1"),
    )];
    let models = BTreeMap::from([("opencode-work".into(), "k3".into())]);

    configure_with_models(temp.path(), &config, &instances, &models).unwrap();
    let contents = std::fs::read_to_string(
        temp.path()
            .join("provider-config/home/.config/opencode/opencode.json"),
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(parsed["model"], "kimi-for-coding/k3");
    assert_eq!(
        parsed["provider"]["kimi-for-coding"]["options"]["baseURL"],
        "https://route.example/v1"
    );
    assert_eq!(
        parsed["provider"]["kimi-for-coding"]["options"]["apiKey"],
        "{env:MOONSHOT_API_KEY}"
    );
    assert!(!contents.contains("work-secret"));
}

#[test]
fn private_configs_use_each_agent_resolved_env_name() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "zai".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Z.AI".into(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: "fixture-zai-key".into(),
                base_url: None,
                model: Some("glm-5".into()),
            },
        },
    );
    let instances = [
        instance("codex-zai", Agent::Codex, "zai", Some("glm-5"), None),
        instance("opencode-zai", Agent::Opencode, "zai", Some("glm-5"), None),
    ];
    configure_for_test(temp.path(), &config, &instances).unwrap();

    let codex: toml::Value = toml::from_str(
        &std::fs::read_to_string(temp.path().join("provider-config/home/.codex/config.toml"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        codex["model_providers"]["jackin_account"]["env_key"].as_str(),
        Some("OPENAI_API_KEY")
    );
    let opencode: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            temp.path()
                .join("provider-config/home/.config/opencode/opencode.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        opencode["provider"]["zai-coding-plan"]["options"]["apiKey"],
        "{env:ZHIPU_API_KEY}"
    );
}

#[test]
fn unadmitted_agents_leave_no_staged_config() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: "fixture-private-key".into(),
                base_url: None,
                model: None,
            },
        },
    );
    let instances = [instance("claude-work", Agent::Claude, "work", None, None)];
    configure_for_test(temp.path(), &config, &instances).unwrap();
    assert!(
        !temp
            .path()
            .join("provider-config/home/.codex/config.toml")
            .exists()
    );
    assert!(
        !temp
            .path()
            .join("provider-config/home/.config/opencode/opencode.json")
            .exists()
    );
}
