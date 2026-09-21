// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;
use std::path::Path;

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
        let catalog_target = format!("/home/agent/{home_rel}/account-models.json");
        assert_eq!(
            parsed["model_catalog_json"].as_str(),
            Some(catalog_target.as_str())
        );
        assert!(directory.join("account-models.json").is_file());
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
        assert_eq!(
            parsed["model_catalog_json"].as_str(),
            Some(format!("/home/agent/{home_rel}/account-models.json").as_str())
        );
        let catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("account-models.json")).unwrap())
                .unwrap();
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
