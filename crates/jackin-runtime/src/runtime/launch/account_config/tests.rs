// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt as _;
use std::path::Path;

struct PrivateConfigFailureGuard {
    previous: Option<PrivateConfigFailurePoint>,
}

fn inject_private_config_failure(point: PrivateConfigFailurePoint) -> PrivateConfigFailureGuard {
    let previous = PRIVATE_CONFIG_FAILURE.with(|failure| failure.replace(Some(point)));
    assert!(
        previous.is_none(),
        "nested private-config failure injection is not supported"
    );
    PrivateConfigFailureGuard { previous }
}

impl Drop for PrivateConfigFailureGuard {
    fn drop(&mut self) {
        PRIVATE_CONFIG_FAILURE.with(|failure| failure.set(self.previous));
    }
}

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
fn codex_reads_existing_config_from_target_directory() {
    let temp = tempfile::tempdir().unwrap();
    let (old_config, old_instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
    configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
    std::fs::write(temp.path().join("home/config.toml"), b"not valid toml [").unwrap();

    let (new_config, new_instances) =
        codex_fixture(AiProvider::Zai, "glm-5.3", "https://new.example/v1");
    configure_for_test(temp.path(), &new_config, &new_instances).unwrap();
    let contents = std::fs::read_to_string(temp.path().join("home/.codex/config.toml")).unwrap();
    assert!(contents.contains("https://new.example/v1"));
    assert!(contents.contains("model_provider = \"jackin_account\""));
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

fn codex_fixture(
    provider: AiProvider,
    model: &str,
    endpoint: &str,
) -> (AppConfig, [jackin_config::ResolvedInstance; 1]) {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider,
            credential: AccountCredential::ApiKey {
                value: "fixture-private-key".into(),
                base_url: Some(endpoint.into()),
                model: Some(model.into()),
            },
        },
    );
    (
        config,
        [instance(
            "codex-work",
            Agent::Codex,
            "work",
            Some(model),
            Some(endpoint),
        )],
    )
}

fn assert_no_private_config_swap_artifacts(directory: &Path) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.starts_with(".jackin-private-config-stage-")
                && !name.starts_with(".jackin-private-config-previous-")
                && !name.starts_with(".jackin-private-config-rollback-")
                && !name.starts_with(".jackin-private-config-previous-cleanup-")
                && name != PRIVATE_CONFIG_TRANSACTION_FILE,
            "private config swap artifact remained: {name}"
        );
    }
}

#[test]
fn codex_catalog_or_config_interruption_preserves_complete_previous_directory() {
    for point in [
        PrivateConfigFailurePoint::StagedFile("account-models.json"),
        PrivateConfigFailurePoint::StagedFile("config.toml"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (old_config, old_instances) =
            codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
        configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
        let directory = temp.path().join("home/.codex");
        let old_config_bytes = std::fs::read(directory.join("config.toml")).unwrap();
        let old_catalog_bytes = std::fs::read(directory.join("account-models.json")).unwrap();

        let (new_config, new_instances) =
            codex_fixture(AiProvider::Zai, "glm-5.3", "https://new.example/v1");
        let _failure = inject_private_config_failure(point);
        let error = configure_for_test(temp.path(), &new_config, &new_instances).unwrap_err();
        assert!(format!("{error:#}").contains("injected private-config publication failure"));

        assert_eq!(
            std::fs::read(directory.join("config.toml")).unwrap(),
            old_config_bytes
        );
        assert_eq!(
            std::fs::read(directory.join("account-models.json")).unwrap(),
            old_catalog_bytes
        );
        assert_no_private_config_swap_artifacts(directory.parent().unwrap());
    }
}

#[test]
fn opencode_publication_failure_preserves_previous_configuration() {
    for point in [
        PrivateConfigFailurePoint::StagedFile("opencode.json"),
        PrivateConfigFailurePoint::AfterInstall,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut old_config = AppConfig::default();
        old_config.accounts.insert(
            "work".into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "Work".into(),
                provider: AiProvider::OpenAi,
                credential: AccountCredential::ApiKey {
                    value: "old-private-key".into(),
                    base_url: Some("https://old.example/v1".into()),
                    model: Some("old-model".into()),
                },
            },
        );
        let old_instances = [instance(
            "opencode-work",
            Agent::Opencode,
            "work",
            Some("old-model"),
            Some("https://old.example/v1"),
        )];
        configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
        let directory = temp.path().join("home/.config/opencode");
        let old_bytes = std::fs::read(directory.join("opencode.json")).unwrap();
        std::fs::write(directory.join("unrelated.json"), b"keep me").unwrap();

        old_config.accounts.get_mut("work").unwrap().credential = AccountCredential::ApiKey {
            value: "new-private-key".into(),
            base_url: Some("https://new.example/v1".into()),
            model: Some("new-model".into()),
        };
        let new_instances = [instance(
            "opencode-work",
            Agent::Opencode,
            "work",
            Some("new-model"),
            Some("https://new.example/v1"),
        )];
        let _failure = inject_private_config_failure(point);
        let error = configure_for_test(temp.path(), &old_config, &new_instances).unwrap_err();
        assert!(format!("{error:#}").contains("private-config publication"));

        assert_eq!(
            std::fs::read(directory.join("opencode.json")).unwrap(),
            old_bytes
        );
        assert_eq!(
            std::fs::read(directory.join("unrelated.json")).unwrap(),
            b"keep me"
        );
        assert_no_private_config_swap_artifacts(directory.parent().unwrap());
    }
}

#[test]
fn restart_recovers_after_crash_between_previous_rename_and_install() {
    let temp = tempfile::tempdir().unwrap();
    let (old_config, old_instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
    configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let old_config_bytes = std::fs::read(directory.join("config.toml")).unwrap();
    let old_catalog_bytes = std::fs::read(directory.join("account-models.json")).unwrap();
    let (new_config, new_instances) =
        codex_fixture(AiProvider::Zai, "glm-5.3", "https://new.example/v1");

    {
        let _failure = inject_private_config_failure(
            PrivateConfigFailurePoint::SimulatedCrashAfterPreviousRename,
        );
        let error = configure_for_test(temp.path(), &new_config, &new_instances).unwrap_err();
        assert!(format!("{error:#}").contains("simulated process crash"));
    }
    assert!(
        !directory.exists(),
        "crash simulation must leave the rename gap"
    );
    assert!(
        directory
            .parent()
            .unwrap()
            .join(PRIVATE_CONFIG_TRANSACTION_FILE)
            .is_file()
    );

    // A fresh launch automatically recovers the previous complete tree before
    // attempting its requested publication.
    configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        old_config_bytes
    );
    assert_eq!(
        std::fs::read(directory.join("account-models.json")).unwrap(),
        old_catalog_bytes
    );
    assert_no_private_config_swap_artifacts(directory.parent().unwrap());

    configure_for_test(temp.path(), &new_config, &new_instances).unwrap();
    let new_contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(new_contents.contains("https://new.example/v1"));
    assert_no_private_config_swap_artifacts(directory.parent().unwrap());
}

#[test]
fn journal_persist_failures_after_renames_restore_previous_directory() {
    for point in [
        PrivateConfigFailurePoint::JournalAfterPreviousMoved,
        PrivateConfigFailurePoint::JournalAfterInstalled,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (old_config, old_instances) =
            codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
        configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
        let directory = temp.path().join("home/.codex");
        let old_config_bytes = std::fs::read(directory.join("config.toml")).unwrap();
        let old_catalog_bytes = std::fs::read(directory.join("account-models.json")).unwrap();
        let (new_config, new_instances) =
            codex_fixture(AiProvider::Zai, "glm-5.3", "https://new.example/v1");

        let _failure = inject_private_config_failure(point);
        let error = configure_for_test(temp.path(), &new_config, &new_instances).unwrap_err();
        assert!(format!("{error:#}").contains("injected private-config publication failure"));
        assert_eq!(
            std::fs::read(directory.join("config.toml")).unwrap(),
            old_config_bytes
        );
        assert_eq!(
            std::fs::read(directory.join("account-models.json")).unwrap(),
            old_catalog_bytes
        );
        assert_no_private_config_swap_artifacts(directory.parent().unwrap());
    }
}

#[cfg(unix)]
#[test]
fn forged_prepared_journal_cannot_delete_a_sibling_directory() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("home");
    let keep = parent.join("keep");
    std::fs::create_dir_all(keep.join("nested")).unwrap();
    std::fs::write(keep.join("nested/important.txt"), b"keep this").unwrap();

    let publication = begin_private_config_publication(temp.path(), &parent).unwrap();
    let transaction = PrivateConfigTransaction {
        schema_version: PRIVATE_CONFIG_TRANSACTION_VERSION,
        target: ".codex".into(),
        transaction_id: "1-0".into(),
        staged: "keep".into(),
        previous: None,
        cleanup: None,
        phase: PrivateConfigTransactionPhase::Prepared,
    };
    private_config_persist_transaction(&publication, &transaction).unwrap();

    let error = private_config_recover_transaction(&publication).unwrap_err();
    assert!(
        format!("{error:#}").contains("staged path is not bound"),
        "{error:#}"
    );
    assert_eq!(
        std::fs::read(keep.join("nested/important.txt")).unwrap(),
        b"keep this"
    );
    assert!(parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).is_file());
}

#[cfg(unix)]
#[test]
fn first_publication_cleanup_recovers_after_stage_unlink_before_sync() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");

    {
        let _failure = inject_private_config_failure(
            PrivateConfigFailurePoint::SimulatedCrashAfterFirstPublicationCleanup,
        );
        let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
        assert!(
            format!("{error:#}").contains("first-publication cleanup"),
            "{error:#}"
        );
    }

    let directory = temp.path().join("home/.codex");
    let parent = directory.parent().unwrap();
    assert!(!directory.exists());
    assert!(parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).is_file());
    let journal: PrivateConfigTransaction = serde_json::from_slice(
        &std::fs::read(parent.join(PRIVATE_CONFIG_TRANSACTION_FILE)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        journal.phase,
        PrivateConfigTransactionPhase::FirstPublicationCleanupPrepared
    );
    assert!(!std::fs::read_dir(parent).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".jackin-private-config-stage-")
    }));

    let publication = begin_private_config_publication(temp.path(), parent).unwrap();
    private_config_recover_transaction(&publication).unwrap();
    assert!(!parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).exists());
    assert!(!directory.exists());
    drop(publication);

    configure_for_test(temp.path(), &config, &instances).unwrap();
    assert!(directory.join("config.toml").is_file());
    assert_no_private_config_swap_artifacts(parent);
}

#[cfg(unix)]
#[test]
fn previous_moved_recovery_is_idempotent_after_previous_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let before = std::fs::read(directory.join("config.toml")).unwrap();
    let parent = directory.parent().unwrap();
    let publication = begin_private_config_publication(temp.path(), parent).unwrap();
    let transaction_id = "1-0";
    let transaction = PrivateConfigTransaction {
        schema_version: PRIVATE_CONFIG_TRANSACTION_VERSION,
        target: ".codex".into(),
        transaction_id: transaction_id.into(),
        staged: private_config_artifact_name("stage", ".codex", transaction_id),
        previous: Some(private_config_artifact_name(
            "previous",
            ".codex",
            transaction_id,
        )),
        cleanup: None,
        phase: PrivateConfigTransactionPhase::PreviousMoved,
    };
    private_config_persist_transaction(&publication, &transaction).unwrap();
    private_config_recover_transaction(&publication).unwrap();

    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        before
    );
    assert!(!parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).exists());
    assert_no_private_config_swap_artifacts(parent);
}

#[cfg(unix)]
#[test]
fn restart_recovers_after_previous_deletion_before_journal_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let (old_config, old_instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
    configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let (new_config, new_instances) =
        codex_fixture(AiProvider::Zai, "glm-5.3", "https://new.example/v1");

    let _failure = inject_private_config_failure(
        PrivateConfigFailurePoint::SimulatedCrashAfterPreviousDeletion,
    );
    let error = configure_for_test(temp.path(), &new_config, &new_instances).unwrap_err();
    assert!(format!("{error:#}").contains("after previous private config deletion"));
    assert!(directory.join("config.toml").is_file());
    let parent = directory.parent().unwrap();
    assert!(parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).is_file());

    let publication = begin_private_config_publication(temp.path(), parent).unwrap();
    private_config_recover_transaction(&publication).unwrap();
    assert!(directory.join("config.toml").is_file());
    assert!(
        std::fs::read_to_string(directory.join("config.toml"))
            .unwrap()
            .contains("https://new.example/v1")
    );
    assert_no_private_config_swap_artifacts(parent);
}

#[cfg(unix)]
#[test]
fn installed_recovery_quarantines_surviving_target_before_restoring_previous() {
    let temp = tempfile::tempdir().unwrap();
    let (old_config, old_instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
    configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let old_config_bytes = std::fs::read(directory.join("config.toml")).unwrap();
    let old_catalog_bytes = std::fs::read(directory.join("account-models.json")).unwrap();
    let parent = directory.parent().unwrap();
    let publication = begin_private_config_publication(temp.path(), parent).unwrap();
    let target = private_config_name(".codex").unwrap();
    let transaction_id =
        private_config_allocate_transaction_id(&publication.parent, ".codex").unwrap();
    let previous_name = private_config_artifact_name("previous", ".codex", &transaction_id);
    let previous = private_config_name(&previous_name).unwrap();
    renameat(
        &publication.parent,
        target.as_c_str(),
        &publication.parent,
        previous.as_c_str(),
    )
    .unwrap();
    let mut staged = private_config_stage(&publication.parent, ".codex", &transaction_id).unwrap();
    private_config_write_file_at(
        &staged.directory,
        "config.toml",
        b"incomplete new config",
        None,
    )
    .unwrap();
    staged.directory.sync_all().unwrap();
    let staged_name = staged.name.to_string_lossy().into_owned();
    renameat(
        &publication.parent,
        staged.name.as_c_str(),
        &publication.parent,
        target.as_c_str(),
    )
    .unwrap();
    staged.disarm();
    let transaction = PrivateConfigTransaction {
        schema_version: PRIVATE_CONFIG_TRANSACTION_VERSION,
        target: ".codex".into(),
        transaction_id,
        staged: staged_name,
        previous: Some(previous_name),
        cleanup: None,
        phase: PrivateConfigTransactionPhase::Installed,
    };
    private_config_persist_transaction(&publication, &transaction).unwrap();
    private_config_recover_transaction(&publication).unwrap();

    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        old_config_bytes
    );
    assert_eq!(
        std::fs::read(directory.join("account-models.json")).unwrap(),
        old_catalog_bytes
    );
    assert_no_private_config_swap_artifacts(parent);
}

#[cfg(unix)]
#[test]
fn installed_recovery_restores_previous_after_mid_recursive_rollback_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let (old_config, old_instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://old.example/v1");
    configure_for_test(temp.path(), &old_config, &old_instances).unwrap();
    let directory = temp.path().join("home/.codex");
    std::fs::create_dir_all(directory.join("nested")).unwrap();
    std::fs::write(directory.join("nested/old.txt"), b"old nested config").unwrap();
    let old_config_bytes = std::fs::read(directory.join("config.toml")).unwrap();
    let old_catalog_bytes = std::fs::read(directory.join("account-models.json")).unwrap();
    let (new_config, new_instances) =
        codex_fixture(AiProvider::Zai, "glm-5.3", "https://new.example/v1");

    {
        let _failure = inject_private_config_failure(
            PrivateConfigFailurePoint::SimulatedCrashDuringRollbackCleanup,
        );
        let error = configure_for_test(temp.path(), &new_config, &new_instances).unwrap_err();
        assert!(format!("{error:#}").contains("rollback failed"));
    }
    assert!(!directory.exists());
    let parent = directory.parent().unwrap();
    assert!(parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).is_file());
    assert!(std::fs::read_dir(parent).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".jackin-private-config-rollback-")
    }));

    let publication = begin_private_config_publication(temp.path(), parent).unwrap();
    private_config_recover_transaction(&publication).unwrap();
    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        old_config_bytes
    );
    assert_eq!(
        std::fs::read(directory.join("nested/old.txt")).unwrap(),
        b"old nested config"
    );
    assert_eq!(
        std::fs::read(directory.join("account-models.json")).unwrap(),
        old_catalog_bytes
    );
    assert_no_private_config_swap_artifacts(parent);
}

#[cfg(unix)]
#[test]
fn installed_recovery_clears_first_publication_after_target_removal() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");

    {
        let _failure = inject_private_config_failure(
            PrivateConfigFailurePoint::SimulatedCrashDuringRollbackCleanup,
        );
        let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
        assert!(format!("{error:#}").contains("rollback failed"));
    }
    let directory = temp.path().join("home/.codex");
    assert!(!directory.exists());
    let parent = directory.parent().unwrap();
    assert!(parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).is_file());

    let publication = begin_private_config_publication(temp.path(), parent).unwrap();
    private_config_recover_transaction(&publication).unwrap();
    assert!(!parent.join(PRIVATE_CONFIG_TRANSACTION_FILE).exists());
    assert!(!directory.exists());
    drop(publication);

    configure_for_test(temp.path(), &config, &instances).unwrap();
    assert!(directory.join("config.toml").is_file());
    assert_no_private_config_swap_artifacts(parent);
}

#[cfg(unix)]
#[test]
fn descriptor_relative_publication_survives_ancestor_swap() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let home = root.join("home");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let directory = home.join(".codex");
    let publication = begin_private_config_publication(&root, &home).unwrap();
    private_config_recover_transaction(&publication).unwrap();

    let real_home = root.join("home-real");
    std::fs::rename(&home, &real_home).unwrap();
    std::os::unix::fs::symlink(&outside, &home).unwrap();
    publish_private_config_directory_locked(
        &publication,
        &directory,
        &[("config.toml", b"descriptor-relative".to_vec())],
        &[],
    )
    .unwrap();

    assert_eq!(
        std::fs::read(real_home.join(".codex/config.toml")).unwrap(),
        b"descriptor-relative"
    );
    assert!(!outside.join(".codex").exists());
}

#[cfg(unix)]
#[test]
fn private_config_publication_rejects_fifo_replacing_config_without_blocking() {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    let temp = tempfile::tempdir().unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let config_path = temp.path().join("home/.codex/config.toml");
    std::fs::remove_file(&config_path).unwrap();
    mkfifo(&config_path, Mode::from_bits_truncate(0o600)).unwrap();

    let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
    assert!(
        format!("{error:#}").contains("not a regular file"),
        "{error:#}"
    );
    assert!(
        std::fs::metadata(config_path)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[cfg(unix)]
#[test]
fn private_config_publication_rejects_fifo_in_preserved_tree() {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    let temp = tempfile::tempdir().unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let fifo_path = directory.join("unrelated.pipe");
    mkfifo(&fifo_path, Mode::from_bits_truncate(0o600)).unwrap();
    let old_config = std::fs::read(directory.join("config.toml")).unwrap();

    let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
    assert!(format!("{error:#}").contains("special entry"), "{error:#}");
    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        old_config
    );
    assert!(std::fs::metadata(fifo_path).unwrap().file_type().is_fifo());
}

#[cfg(unix)]
#[test]
fn private_config_parent_lock_serializes_publishers() {
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let parent = root.join("home");
    std::fs::create_dir_all(&parent).unwrap();
    let held = begin_private_config_publication(&root, &parent).unwrap();
    let (attempted_tx, attempted_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let root_for_thread = root.clone();
    let parent_for_thread = parent.clone();
    let worker = std::thread::spawn(move || {
        attempted_tx.send(()).unwrap();
        let _publication = begin_private_config_publication(&root_for_thread, &parent_for_thread);
        finished_tx.send(()).unwrap();
    });
    attempted_rx.recv().unwrap();
    assert!(
        finished_rx
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    drop(held);
    finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
}

#[cfg(unix)]
#[test]
fn private_config_publication_rejects_symlinked_ancestor_below_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("home")).unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");

    let error = configure_for_test(&root, &config, &instances).unwrap_err();
    assert!(
        format!("{error:#}").contains("private config ancestor is a symlink"),
        "{error:#}"
    );
    assert!(!outside.join(".codex").exists());
    assert!(!root.join(PRIVATE_CONFIG_TRANSACTION_FILE).exists());
}

#[cfg(unix)]
#[test]
fn private_config_publication_preserves_paths_and_permissions() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempfile::tempdir().unwrap();
    let (config, instances) =
        codex_fixture(AiProvider::Moonshot, "k3", "https://provider.example/v1");
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let unrelated = directory.join("preserved.toml");
    std::fs::write(&unrelated, b"preserve this").unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o750)).unwrap();
    for name in ["config.toml", "account-models.json", "preserved.toml"] {
        std::fs::set_permissions(directory.join(name), std::fs::Permissions::from_mode(0o640))
            .unwrap();
    }

    configure_for_test(temp.path(), &config, &instances).unwrap();

    assert_eq!(
        std::fs::metadata(&directory).unwrap().permissions().mode() & 0o7777,
        0o750
    );
    for name in ["config.toml", "account-models.json", "preserved.toml"] {
        assert_eq!(
            std::fs::metadata(directory.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o640,
            "permissions changed for {name}"
        );
    }
    assert_eq!(std::fs::read(unrelated).unwrap(), b"preserve this");
}
