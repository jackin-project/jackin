// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        let directory = temp.path().join("provider-config/home").join(home_rel);
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
        let directory = temp.path().join("provider-config/home").join(home_rel);
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

#[test]
fn corrupt_codex_config_is_quarantined_and_regenerated() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("provider-config/home/.codex");
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
    quarantined_payload(&directory, garbage)?;
    Ok(())
}

#[test]
fn non_utf8_codex_config_is_quarantined_and_regenerated() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("provider-config/home/.codex");
    std::fs::create_dir_all(&directory).unwrap();
    let garbage = b"\xff\xfe\x00 not utf8 \x80";
    std::fs::write(directory.join("config.toml"), garbage).unwrap();

    configure_for_test(temp.path(), &config, &instances).unwrap();

    let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&contents).unwrap();
    assert_eq!(parsed["model"].as_str(), Some("k3-256k"));
    quarantined_payload(&directory, garbage)?;
    Ok(())
}

#[test]
fn non_table_model_providers_is_quarantined_and_regenerated() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("provider-config/home/.codex");
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
    quarantined_payload(&directory, garbage)?;
    Ok(())
}

#[test]
#[cfg(unix)]
fn codex_config_fifo_is_rejected_without_blocking() {
    use nix::sys::stat::Mode;
    use std::os::unix::fs::FileTypeExt as _;

    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    let directory = temp.path().join("provider-config/home/.codex");
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
    let home = temp.path().join("provider-config/home");
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
    std::fs::create_dir(symlink_root.path().join("provider-config")).unwrap();
    symlink(&outside, symlink_root.path().join("provider-config/home")).unwrap();
    let error =
        private_config_fs::open_directory(symlink_root.path(), Path::new(".codex")).unwrap_err();
    assert!(!format!("{error:#}").is_empty());
    assert!(
        outside.is_dir(),
        "child symlink target must remain untouched"
    );
}
