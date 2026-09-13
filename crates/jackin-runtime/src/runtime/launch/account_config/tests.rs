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
        config
            .account_bindings
            .insert(Agent::Opencode, "work".into());
        configure_accounts(temp.path(), &config, None, "", &[Agent::Opencode]).unwrap();
        let contents =
            std::fs::read_to_string(temp.path().join("home/.config/opencode/opencode.json"))
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
        config.account_bindings.insert(Agent::Codex, "work".into());
        configure_accounts(temp.path(), &config, None, "", &[Agent::Codex]).unwrap();
        let contents =
            std::fs::read_to_string(temp.path().join("home/.codex/config.toml")).unwrap();
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
        let catalog: serde_json::Value = serde_json::from_slice(
            &std::fs::read(temp.path().join("home/.codex/account-models.json")).unwrap(),
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
