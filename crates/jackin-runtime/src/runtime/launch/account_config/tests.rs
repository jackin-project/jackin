// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;
use std::path::Path;

#[cfg(target_os = "macos")]
use std::path::PathBuf;

fn slots_for(
    instances: &[jackin_config::ResolvedInstance],
) -> std::collections::BTreeMap<String, crate::instance::ProvisionedInstanceAuth> {
    let mut seen_agents = Vec::new();
    instances
        .iter()
        .map(|instance| {
            let suffix = if seen_agents.contains(&instance.agent) {
                Some(instance.config_id.clone())
            } else {
                seen_agents.push(instance.agent);
                None
            };
            let (home_rel, store_rel) = match instance.agent {
                Agent::Codex => (".codex", "codex"),
                Agent::Opencode => (".local/share/opencode", "opencode"),
                _ => (".unused", "unused"),
            };
            let container_home_rel = crate::instance::slot_home_rel(home_rel, suffix.as_deref());
            let container_store_rel = suffix.as_deref().map_or_else(
                || store_rel.to_owned(),
                |suffix| format!("{store_rel}-{suffix}"),
            );
            let folder_target = if instance.agent == Agent::Opencode {
                "/home/agent/.local".to_owned()
            } else {
                format!("/home/agent/{container_home_rel}")
            };
            (
                instance.config_id.clone(),
                crate::instance::ProvisionedInstanceAuth {
                    agent: instance.agent,
                    account_id: instance.account_id.clone(),
                    mode: jackin_config::AuthForwardMode::ApiKey,
                    home_dir: None,
                    credential_paths: Vec::new(),
                    forward_auth: false,
                    slot_suffix: suffix,
                    container_home_rel,
                    container_store_rel,
                    folder_target,
                    cache_source_dir: None,
                    container_cache_rel: None,
                },
            )
        })
        .collect()
}

fn configure_for_test(
    root: &Path,
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
) -> anyhow::Result<()> {
    let slots = slots_for(instances);
    let models = instances
        .iter()
        .filter_map(|instance| {
            instance
                .model
                .as_ref()
                .map(|model| (instance.config_id.clone(), model.clone()))
        })
        .collect();
    configure_accounts(root, config, instances, &slots, &models, &BTreeMap::new())
}

fn configure_with_models(
    root: &Path,
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    models: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let slots = slots_for(instances);
    configure_accounts(root, config, instances, &slots, models, &BTreeMap::new())
}

fn instance(
    config_id: &str,
    agent: Agent,
    account_id: &str,
    model: Option<&str>,
    base_url: Option<&str>,
) -> jackin_config::ResolvedInstance {
    jackin_config::ResolvedInstance {
        config_id: config_id.into(),
        agent,
        account_id: account_id.into(),
        model: model.map(str::to_owned),
        base_url: base_url.map(str::to_owned),
        xdg_roots: None,
        label: config_id.into(),
        synthesized: false,
    }
}

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

/// F7 container leg: an `account add --model <exact-id>` `OpenRouter` pin
/// must land in `opencode.json` byte-exact.
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
    let contents =
        std::fs::read_to_string(temp.path().join("home/.config/opencode/opencode.json")).unwrap();
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
        let catalog_target = parsed["model_catalog_json"].as_str().unwrap().to_owned();
        let catalog_name = Path::new(&catalog_target)
            .file_name()
            .expect("catalog target has a file name")
            .to_owned();
        let catalog: serde_json::Value = serde_json::from_slice(
            &std::fs::read(temp.path().join("home/.codex").join(catalog_name)).unwrap(),
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
    let contents = std::fs::read_to_string(temp.path().join("home/.codex/config.toml")).unwrap();
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
    let contents = std::fs::read_to_string(temp.path().join("home/.codex/config.toml")).unwrap();
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
    let contents =
        std::fs::read_to_string(temp.path().join("home/.config/opencode/opencode.json")).unwrap();
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
        &std::fs::read_to_string(temp.path().join("home/.codex/config.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        codex["model_providers"]["jackin_account"]["env_key"].as_str(),
        Some("OPENAI_API_KEY")
    );
    let opencode: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(temp.path().join("home/.config/opencode/opencode.json")).unwrap(),
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
    assert!(!temp.path().join("home/.codex/config.toml").exists());
    assert!(
        !temp
            .path()
            .join("home/.config/opencode/opencode.json")
            .exists()
    );
}

#[test]
fn codex_instances_keep_slot_config_and_credential_identity() {
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
                base_url: Some("https://work.example/v1".into()),
                model: Some("k3-256k".into()),
            },
        },
    );
    config.accounts.insert(
        "personal".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: "personal-secret".into(),
                base_url: Some("https://personal.example/v1".into()),
                model: Some("glm-5.3".into()),
            },
        },
    );
    let instances = [
        instance(
            "codex-work",
            Agent::Codex,
            "work",
            Some("k3-256k"),
            Some("https://work.example/v1"),
        ),
        instance(
            "codex-personal",
            Agent::Codex,
            "personal",
            Some("glm-5.3"),
            Some("https://personal.example/v1"),
        ),
    ];
    let credentials = jackin_env::resolve_instance_env_with(
        &config,
        &instances,
        None,
        "smith",
        &jackin_env::OpCli::new(),
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();

    configure_for_test(temp.path(), &config, &instances).unwrap();

    for (config_id, account_id, secret, model, endpoint, env_key, home_rel) in [
        (
            "codex-work",
            "work",
            "work-secret",
            "k3-256k",
            "https://work.example/v1",
            "KIMI_API_KEY",
            ".codex",
        ),
        (
            "codex-personal",
            "personal",
            "personal-secret",
            "glm-5.3",
            "https://personal.example/v1",
            "OPENAI_API_KEY",
            ".codex-codex-personal",
        ),
    ] {
        let directory = temp.path().join("home").join(home_rel);
        let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        assert_eq!(parsed["model"].as_str(), Some(model));
        assert_eq!(
            parsed["model_providers"]["jackin_account"]["base_url"].as_str(),
            Some(endpoint)
        );
        assert_eq!(
            parsed["model_providers"]["jackin_account"]["env_key"].as_str(),
            Some(env_key)
        );
        let catalog = model_catalog(
            if account_id == "work" {
                AiProvider::Moonshot
            } else {
                AiProvider::Zai
            },
            model,
        )
        .expect("fixture has a published catalog");
        let catalog_contents = serde_json::to_vec_pretty(&catalog).unwrap();
        let catalog_name = codex_catalog_filename(&catalog_contents);
        let catalog_target = format!("/home/agent/{home_rel}/{catalog_name}");
        assert_eq!(
            parsed["model_catalog_json"].as_str(),
            Some(catalog_target.as_str())
        );
        assert!(directory.join(&catalog_name).is_file());
        assert!(!contents.contains(secret));

        let envelope = credentials.instance(config_id).unwrap();
        assert_eq!(envelope.agent, "codex");
        assert_eq!(envelope.account_id, account_id);
        assert_eq!(envelope.env.get(env_key).map(String::as_str), Some(secret));
    }
}

#[test]
fn codex_slots_keep_routed_models_catalogs_and_requested_effort_separate() {
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
                base_url: Some("https://work.example/v1".into()),
                model: Some("k3".into()),
            },
        },
    );
    config.accounts.insert(
        "personal".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: "personal-secret".into(),
                base_url: Some("https://personal.example/v1".into()),
                model: Some("glm-5.3".into()),
            },
        },
    );
    let instances = [
        instance(
            "codex-work",
            Agent::Codex,
            "work",
            Some("k3"),
            Some("https://work.example/v1"),
        ),
        instance(
            "codex-personal",
            Agent::Codex,
            "personal",
            Some("glm-5.3"),
            Some("https://personal.example/v1"),
        ),
    ];
    let models = BTreeMap::from([
        ("codex-work".to_owned(), "k3".to_owned()),
        ("codex-personal".to_owned(), "glm-5.3".to_owned()),
    ]);
    let efforts = BTreeMap::from([
        ("codex-work".to_owned(), "max".to_owned()),
        ("codex-personal".to_owned(), "low".to_owned()),
    ]);
    let slots = slots_for(&instances);
    configure_accounts(temp.path(), &config, &instances, &slots, &models, &efforts).unwrap();

    for (id, home_rel, endpoint, model, effort) in [
        (
            "codex-work",
            ".codex",
            "https://work.example/v1",
            "k3",
            "max",
        ),
        (
            "codex-personal",
            ".codex-codex-personal",
            "https://personal.example/v1",
            "glm-5.3",
            "low",
        ),
    ] {
        let directory = temp.path().join("home").join(home_rel);
        let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        assert_eq!(parsed["model"].as_str(), Some(model));
        assert_eq!(
            parsed["model_providers"]["jackin_account"]["base_url"].as_str(),
            Some(endpoint)
        );
        assert_eq!(parsed["model_reasoning_effort"].as_str(), Some(effort));
        let catalog = model_catalog(
            if id == "codex-work" {
                AiProvider::Moonshot
            } else {
                AiProvider::Zai
            },
            model,
        )
        .expect("fixture has a published catalog");
        let catalog_contents = serde_json::to_vec_pretty(&catalog).unwrap();
        let catalog_name = codex_catalog_filename(&catalog_contents);
        assert_eq!(
            parsed["model_catalog_json"].as_str(),
            Some(format!("/home/agent/{home_rel}/{catalog_name}").as_str())
        );
        let catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join(&catalog_name)).unwrap()).unwrap();
        let levels = catalog["models"][0]["supported_reasoning_levels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|level| level["effort"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(levels, ["low", "medium", "high", "max"]);
        assert!(contents.contains(&format!("model_reasoning_effort = \"{effort}\"")));
        assert!(!contents.contains("model_reasoning_effort = \"high\""));
        assert!(slots.contains_key(id));
    }
}

fn codex_moonshot_fixture() -> (AppConfig, [jackin_config::ResolvedInstance; 1]) {
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
                model: Some("k3-256k".into()),
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
    (config, instances)
}

fn quarantined_payload(directory: &Path, expected: &[u8]) {
    let matches: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("config.toml.corrupt-"))
        })
        .collect();
    assert_eq!(matches.len(), 1, "expected one quarantine file");
    let name = matches[0].file_name().unwrap().to_str().unwrap().to_owned();
    let suffix = name.strip_prefix("config.toml.corrupt-").unwrap();
    let (secs, pid) = suffix.rsplit_once('-').unwrap();
    assert!(secs.parse::<u64>().is_ok(), "unix-secs suffix: {name}");
    assert_eq!(pid.parse::<u32>().unwrap(), std::process::id());
    assert_eq!(std::fs::read(&matches[0]).unwrap(), expected);
}

#[test]
fn corrupt_codex_config_is_quarantined_and_regenerated() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("home/.codex");
    std::fs::create_dir_all(&directory).unwrap();
    let garbage = b"!!! not toml [[[\n";
    std::fs::write(directory.join("config.toml"), garbage).unwrap();

    configure_for_test(temp.path(), &config, &instances).unwrap();

    let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&contents).unwrap();
    assert_eq!(parsed["model"].as_str(), Some("k3-256k"));
    assert_eq!(
        parsed["model_providers"]["jackin_account"]["env_key"].as_str(),
        Some("KIMI_API_KEY")
    );
    quarantined_payload(&directory, garbage);
}

#[test]
fn non_utf8_codex_config_is_quarantined_and_regenerated() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("home/.codex");
    std::fs::create_dir_all(&directory).unwrap();
    let garbage = b"\xff\xfe\x00 not utf8 \x80";
    std::fs::write(directory.join("config.toml"), garbage).unwrap();

    configure_for_test(temp.path(), &config, &instances).unwrap();

    let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&contents).unwrap();
    assert_eq!(parsed["model"].as_str(), Some("k3-256k"));
    quarantined_payload(&directory, garbage);
}

#[test]
fn non_table_model_providers_is_quarantined_and_regenerated() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("home/.codex");
    std::fs::create_dir_all(&directory).unwrap();
    let garbage = b"model_providers = \"nope\"\n";
    std::fs::write(directory.join("config.toml"), garbage).unwrap();

    configure_for_test(temp.path(), &config, &instances).unwrap();

    let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&contents).unwrap();
    assert!(
        parsed["model_providers"].is_table(),
        "regenerated doc must carry a model_providers table"
    );
    assert_eq!(
        parsed["model_providers"]["jackin_account"]["env_key"].as_str(),
        Some("KIMI_API_KEY")
    );
    quarantined_payload(&directory, garbage);
}

#[test]
#[cfg(unix)]
fn codex_config_fifo_is_rejected_without_blocking() {
    use nix::sys::stat::Mode;
    use std::os::unix::fs::FileTypeExt as _;

    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("home/.codex");
    std::fs::create_dir_all(&directory).unwrap();
    let fifo = directory.join("config.toml");
    nix::unistd::mkfifo(&fifo, Mode::from_bits(0o644).unwrap()).unwrap();

    let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
    assert!(
        format!("{error:#}").contains("regular file"),
        "unexpected error: {error:#}"
    );
    assert!(
        std::fs::symlink_metadata(&fifo)
            .unwrap()
            .file_type()
            .is_fifo(),
        "planted FIFO must be left untouched"
    );
}

#[cfg(unix)]
#[test]
fn private_config_fs_tightens_existing_modes_without_following_child_symlinks() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    let temp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = temp.path().join("home");
    let directory_path = home.join(".codex");
    std::fs::create_dir_all(&directory_path).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&directory_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode(temp.path()), 0o700);
    assert_eq!(mode(&home), 0o700);
    assert_eq!(mode(&directory_path), 0o700);

    let catalog = br#"{"models":[{"slug":"fixture"}]}"#;
    let catalog_name = codex_catalog_filename(catalog);
    std::fs::write(directory_path.join(&catalog_name), catalog).unwrap();
    std::fs::set_permissions(
        directory_path.join(&catalog_name),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    private_config_fs::publish_catalog(&directory, &catalog_name, catalog, |_| Ok(())).unwrap();
    assert_eq!(mode(&directory_path.join(&catalog_name)), 0o600);
    let lock = private_config_fs::lock(&directory).unwrap();
    assert_eq!(
        mode(&directory_path.join(".jackin-private-provider-config.lock")),
        0o600
    );
    drop(lock);
    private_config_fs::publish_atomic(
        &directory,
        "config.toml",
        b"model = \"fixture\"\n",
        private_config_fs::Artifact::CodexConfig,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(mode(&directory_path.join("config.toml")), 0o600);

    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let symlink_root = tempfile::tempdir().unwrap();
    symlink(&outside, symlink_root.path().join("home")).unwrap();
    let error =
        private_config_fs::open_directory(symlink_root.path(), Path::new(".codex")).unwrap_err();
    assert!(!format!("{error:#}").is_empty());
    assert!(
        outside.is_dir(),
        "child symlink target must remain untouched"
    );
}

#[cfg(unix)]
#[test]
fn private_config_fs_rejects_empty_and_dot_roots_before_creating_home() {
    let current = std::env::current_dir().unwrap();
    let sentinel = format!(".jackin-rejected-root-{}", std::process::id());
    let sentinel_path = current.join("home").join(&sentinel);
    assert!(!sentinel_path.exists());

    for root in [Path::new(""), Path::new(".")] {
        let error = private_config_fs::open_directory(root, Path::new(&sentinel)).unwrap_err();
        assert!(format!("{error:#}").contains("root"));
        assert!(
            !sentinel_path.exists(),
            "invalid root must not create a private config home"
        );
    }
}

#[cfg(unix)]
#[test]
fn private_config_fs_rejects_lock_symlink_without_following_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let directory_path = temp.path().join("home/.codex");
    let outside = temp.path().join("outside-lock");
    std::fs::write(&outside, b"outside").unwrap();
    let lock_path = directory_path.join(".jackin-private-provider-config.lock");
    symlink(&outside, &lock_path).unwrap();

    let error = private_config_fs::lock(&directory).unwrap_err();
    assert!(!format!("{error:#}").is_empty());
    assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
    assert!(
        std::fs::symlink_metadata(&lock_path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "lock symlink must remain untouched"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn private_config_fs_normalizes_macos_lexical_root_aliases() {
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/var")).unwrap(),
        PathBuf::from("/private/var")
    );
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/tmp/jackin")).unwrap(),
        PathBuf::from("/private/tmp/jackin")
    );
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/etc")).unwrap(),
        PathBuf::from("/private/etc")
    );
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/various/jackin")).unwrap(),
        PathBuf::from("/various/jackin")
    );
    assert!(private_config_fs::normalize_root(Path::new("/var/../tmp")).is_err());
}

#[cfg(unix)]
#[test]
fn private_config_fs_rejects_non_leaf_names_before_filesystem_access() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let directory_path = temp.path().join("home/.codex");
    let outside = temp.path().join("home/outside");
    let catalog = br#"{"models":[{"slug":"fixture"}]}"#;
    std::fs::write(&outside, catalog).unwrap();

    for name in [
        "",
        ".",
        "..",
        "../outside",
        "nested/name",
        "/outside",
        "outside\0",
    ] {
        assert!(
            private_config_fs::read_optional(&directory, name).is_err(),
            "name {name:?} must be rejected"
        );
    }

    let error = private_config_fs::quarantine(&directory, "../outside", "test").unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert_eq!(std::fs::read(&outside).unwrap(), catalog);

    let error = private_config_fs::publish_catalog(&directory, "../outside", catalog, |_| Ok(()))
        .unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert_eq!(std::fs::read(&outside).unwrap(), catalog);

    let error = private_config_fs::publish_atomic(
        &directory,
        "../outside",
        b"replacement",
        private_config_fs::Artifact::CodexConfig,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert_eq!(std::fs::read(&outside).unwrap(), catalog);

    let (temp_name, temp_file) = private_config_fs::create_temp_file(&directory).unwrap();
    let outside_owned = temp.path().join("home/outside-owned");
    std::fs::hard_link(directory_path.join(&temp_name), &outside_owned).unwrap();
    let error =
        private_config_fs::cleanup_owned_temp(&directory, "../outside-owned", &temp_file, Ok(()))
            .unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert!(
        outside_owned.exists(),
        "invalid cleanup must not unlink outside"
    );
    private_config_fs::cleanup_owned_temp(&directory, &temp_name, &temp_file, Ok(())).unwrap();
    assert!(
        outside_owned.exists(),
        "hard link must remain outside the directory"
    );
}

#[cfg(unix)]
#[test]
fn private_config_fs_does_not_remove_replaced_orphan_staging_files() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let directory_path = temp.path().join("home/.codex");
    let (name, file) = private_config_fs::create_temp_file(&directory).unwrap();
    std::fs::remove_file(directory_path.join(&name)).unwrap();
    std::fs::write(directory_path.join(&name), b"foreign orphan").unwrap();

    let error =
        private_config_fs::cleanup_owned_temp(&directory, &name, &file, Ok(())).unwrap_err();
    assert!(format!("{error:#}").contains("changed ownership"));
    assert_eq!(
        std::fs::read(directory_path.join(&name)).unwrap(),
        b"foreign orphan"
    );
    std::fs::remove_file(directory_path.join(&name)).unwrap();
}

#[cfg(unix)]
#[test]
fn codex_catalog_publication_is_immutable_and_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = br#"{"models":[{"slug":"fixture"}]}"#;
    let name = codex_catalog_filename(contents);

    private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap();
    private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap();
    assert_eq!(
        std::fs::read(temp.path().join("home/.codex").join(&name)).unwrap(),
        contents
    );

    let collision = br#"{"models":[{"slug":"different"}]}"#;
    let error =
        private_config_fs::publish_catalog(&directory, &name, collision, |_| Ok(())).unwrap_err();
    assert!(
        format!("{error:#}").contains("different contents"),
        "unexpected collision error: {error:#}"
    );
}

#[cfg(unix)]
#[test]
fn codex_catalog_publication_rejects_symlink_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = br#"{"models":[{"slug":"fixture"}]}"#;
    let name = codex_catalog_filename(contents);
    let outside = temp.path().join("outside.json");
    std::fs::write(&outside, b"outside").unwrap();
    symlink(&outside, temp.path().join("home/.codex").join(&name)).unwrap();

    let error =
        private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap_err();
    assert!(
        !format!("{error:#}").is_empty(),
        "symlink target must be rejected"
    );
    assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
}

#[cfg(unix)]
#[test]
fn codex_catalog_publication_failure_leaves_no_staged_file() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = br#"{"models":[{"slug":"fixture"}]}"#;
    let name = codex_catalog_filename(contents);
    let error =
        private_config_fs::publish_catalog(&directory, &name, contents, |point| match point {
            private_config_fs::PublishPoint::BeforeInstall(
                private_config_fs::Artifact::CodexCatalog,
            ) => anyhow::bail!("injected publication failure"),
            _ => Ok(()),
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("injected publication failure"));
    assert!(!temp.path().join("home/.codex").join(&name).exists());
    let leftovers = std::fs::read_dir(temp.path().join("home/.codex"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".jackin-private-provider-config-")
        })
        .count();
    assert_eq!(leftovers, 0, "owned staging file must be cleaned up");
}

#[cfg(unix)]
#[test]
fn codex_catalog_install_failures_leave_a_durable_retryable_catalog() {
    for failed_at in [
        private_config_fs::PublishPoint::Installed(private_config_fs::Artifact::CodexCatalog),
        private_config_fs::PublishPoint::DirectorySynced(private_config_fs::Artifact::CodexCatalog),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let directory =
            private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
        let contents = br#"{"models":[{"slug":"retry"}]}"#;
        let name = codex_catalog_filename(contents);
        let error = private_config_fs::publish_catalog(&directory, &name, contents, |point| {
            (point == failed_at)
                .then_some(anyhow::anyhow!("injected catalog publication failure"))
                .map_or(Ok(()), Err)
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected catalog publication failure"));
        assert_eq!(
            std::fs::read(temp.path().join("home/.codex").join(&name)).unwrap(),
            contents
        );
        private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn config_rename_failures_preserve_the_new_catalog_pair_and_retry() {
    for failed_at in [
        private_config_fs::PublishPoint::Installed(private_config_fs::Artifact::CodexConfig),
        private_config_fs::PublishPoint::DirectorySynced(private_config_fs::Artifact::CodexConfig),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let directory =
            private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
        let catalog = br#"{"models":[{"slug":"pair"}]}"#;
        let catalog_name = codex_catalog_filename(catalog);
        private_config_fs::publish_catalog(&directory, &catalog_name, catalog, |_| Ok(())).unwrap();
        let config = format!("model_catalog_json = \"/home/agent/.codex/{catalog_name}\"\n");
        let error = private_config_fs::publish_atomic(
            &directory,
            "config.toml",
            config.as_bytes(),
            private_config_fs::Artifact::CodexConfig,
            |point| {
                (point == failed_at)
                    .then_some(anyhow::anyhow!("injected config publication failure"))
                    .map_or(Ok(()), Err)
            },
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected config publication failure"));
        assert_eq!(
            std::fs::read_to_string(temp.path().join("home/.codex/config.toml")).unwrap(),
            config
        );
        assert!(
            temp.path()
                .join("home/.codex")
                .join(&catalog_name)
                .is_file()
        );
        private_config_fs::publish_atomic(
            &directory,
            "config.toml",
            config.as_bytes(),
            private_config_fs::Artifact::CodexConfig,
            |_| Ok(()),
        )
        .unwrap();
    }
}

#[test]
fn codex_catalog_rotation_preserves_stale_container_reference() {
    let temp = tempfile::tempdir().unwrap();
    let (config, first_instances) = codex_moonshot_fixture();
    configure_for_test(temp.path(), &config, &first_instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let first_catalog = {
        let document: toml::Value =
            toml::from_str(&std::fs::read_to_string(directory.join("config.toml")).unwrap())
                .unwrap();
        Path::new(document["model_catalog_json"].as_str().unwrap())
            .file_name()
            .unwrap()
            .to_owned()
    };
    assert!(directory.join(&first_catalog).is_file());

    let second_instances = [instance(
        "codex-work",
        Agent::Codex,
        "work",
        Some("k3"),
        None,
    )];
    let second_config = config;
    // The first fixture uses k3-256k; the second publication intentionally
    // changes the catalog content while keeping the slot/container path.
    configure_for_test(temp.path(), &second_config, &second_instances).unwrap();
    let document: toml::Value =
        toml::from_str(&std::fs::read_to_string(directory.join("config.toml")).unwrap()).unwrap();
    let second_catalog = Path::new(document["model_catalog_json"].as_str().unwrap())
        .file_name()
        .unwrap()
        .to_owned();
    assert_ne!(first_catalog, second_catalog);
    assert!(directory.join(&first_catalog).is_file());
    assert!(directory.join(&second_catalog).is_file());
}
