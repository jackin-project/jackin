// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn fixture() -> (
    AppConfig,
    jackin_config::ResolvedInstance,
    crate::instance::ProvisionedInstanceAuth,
) {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "selected".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Selected".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: "fixture-only-key".into(),
                base_url: None,
                model: Some("k3-256k".into()),
            },
        },
    );
    let instance = jackin_config::ResolvedInstance {
        config_id: "selected-codex".into(),
        agent: Agent::Codex,
        account_id: "selected".into(),
        model: Some("k3-256k".into()),
        base_url: None,
        xdg_roots: None,
        label: "Selected".into(),
        synthesized: false,
    };
    let slot = crate::instance::ProvisionedInstanceAuth {
        profile_material: None,
        agent: Agent::Codex,
        account_id: "selected".into(),
        mode: jackin_config::AuthForwardMode::ApiKey,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: false,
        slot_suffix: None,
        container_home_rel: ".codex".into(),
        container_store_rel: "codex".into(),
        folder_target: "/home/agent/.codex".into(),
        cache_source_dir: None,
        container_cache_rel: None,
    };
    (config, instance, slot)
}

#[test]
fn writable_agent_home_cannot_exchange_publication_staging_names() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home/.codex");
    std::fs::create_dir_all(&home).unwrap();
    let (config, instance, slot) = fixture();
    let mut exchanged = 0;
    let mut substituted_catalog_installed = false;
    let result = configure_codex_with_publish_hook(
        temp.path(),
        &config,
        &instance,
        &slot,
        Some("k3-256k"),
        None,
        |point| {
            if matches!(point, private_config_fs::PublishPoint::BeforeInstall(_)) {
                // This attacker can write only the directory mounted into its
                // own home. Replace every visible staged leaf immediately before
                // install, including the catalog's hard-link publication path.
                for entry in std::fs::read_dir(&home)? {
                    let entry = entry?;
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".jackin-private-provider-config-")
                    {
                        std::fs::remove_file(entry.path())?;
                        std::fs::write(entry.path(), b"model_provider = \"attacker\"\n")?;
                        exchanged += 1;
                    }
                }
                std::fs::write(home.join("config.toml"), b"model_provider = \"attacker\"\n")?;
            }
            if matches!(
                point,
                private_config_fs::PublishPoint::Installed(
                    private_config_fs::Artifact::CodexCatalog
                )
            ) {
                for entry in std::fs::read_dir(&home)? {
                    let entry = entry?;
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("account-models-")
                    {
                        substituted_catalog_installed |=
                            std::fs::read(entry.path())? == b"model_provider = \"attacker\"\n";
                    }
                }
            }
            Ok(())
        },
    );
    assert!(
        !substituted_catalog_installed,
        "attacker substituted bytes reached the published catalog before cleanup"
    );
    assert_eq!(
        exchanged, 0,
        "publication staging must be outside the writable agent home"
    );
    result.unwrap();
    let authority = temp.path().join("provider-config/home/.codex");
    let contents = std::fs::read_to_string(authority.join("config.toml")).unwrap();
    let config: toml::Value = toml::from_str(&contents).unwrap();
    assert_eq!(config["model_provider"].as_str(), Some("jackin_account"));
    let name = Path::new(config["model_catalog_json"].as_str().unwrap())
        .file_name()
        .unwrap();
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(authority.join(name)).unwrap()).unwrap();
    assert!(catalog["models"].is_array());
}

#[test]
fn generated_authority_does_not_import_writable_home_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home/.codex");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("config.toml"),
        b"attacker_option = true\n[mcp_servers.attacker]\ncommand = \"attacker\"\n",
    )
    .unwrap();
    let (config, instance, slot) = fixture();
    let mounts = configure_accounts(
        temp.path(),
        &config,
        &[instance.clone()],
        &BTreeMap::from([(instance.config_id, slot)]),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        mounts.len(),
        2,
        "config and selected catalog must both have exact overlays"
    );
    let config_mount = mounts
        .iter()
        .find(|(_, target)| target.ends_with("/config.toml"))
        .unwrap();
    assert!(
        config_mount
            .0
            .starts_with(temp.path().join("provider-config"))
    );
    assert_eq!(config_mount.1, "/home/agent/.codex/config.toml");
    let generated: toml::Value =
        toml::from_str(&std::fs::read_to_string(&config_mount.0).unwrap()).unwrap();
    assert!(generated.get("attacker_option").is_none());
    assert!(generated.get("mcp_servers").is_none());
    assert_eq!(generated["model_provider"].as_str(), Some("jackin_account"));
}

#[test]
fn provider_authority_rejects_symlink_into_writable_agent_home() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("home")).unwrap();
    symlink(
        temp.path().join("home"),
        temp.path().join("provider-config"),
    )
    .unwrap();
    let (config, instance, slot) = fixture();
    let error = configure_codex(
        temp.path(),
        &config,
        &instance,
        &slot,
        Some("k3-256k"),
        None,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("private account config directory"));
    assert!(!temp.path().join("home/home/.codex/config.toml").exists());
}

#[test]
fn later_host_publication_cannot_replace_an_admitted_config_catalog_generation() {
    let temp = tempfile::tempdir().unwrap();
    let (config, mut instance, slot) = fixture();
    let slots = BTreeMap::from([(instance.config_id.clone(), slot)]);
    let first = configure_accounts(
        temp.path(),
        &config,
        &[instance.clone()],
        &slots,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let first_config = &first
        .iter()
        .find(|(_, target)| target.ends_with("/config.toml"))
        .unwrap()
        .0;
    let first_bytes = std::fs::read(first_config).unwrap();
    instance.model = Some("k3".into());
    let second = configure_accounts(
        temp.path(),
        &config,
        &[instance],
        &slots,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let second_config = &second
        .iter()
        .find(|(_, target)| target.ends_with("/config.toml"))
        .unwrap()
        .0;
    assert_ne!(
        first_config, second_config,
        "different configurations need immutable source identities before Docker resolves a bind"
    );
    assert_eq!(std::fs::read(first_config).unwrap(), first_bytes);
    let first_document: toml::Value =
        toml::from_str(std::str::from_utf8(&first_bytes).unwrap()).unwrap();
    let second_document: toml::Value =
        toml::from_str(&std::fs::read_to_string(second_config).unwrap()).unwrap();
    assert_eq!(first_document["model"].as_str(), Some("k3-256k"));
    assert_eq!(second_document["model"].as_str(), Some("k3"));
    for mounts in [first, second] {
        let config_source = &mounts
            .iter()
            .find(|(_, target)| target.ends_with("/config.toml"))
            .unwrap()
            .0;
        let document: toml::Value =
            toml::from_str(&std::fs::read_to_string(config_source).unwrap()).unwrap();
        let catalog_target = document["model_catalog_json"].as_str().unwrap();
        let catalog_source = &mounts
            .iter()
            .find(|(_, target)| target == catalog_target)
            .unwrap()
            .0;
        let catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(catalog_source).unwrap()).unwrap();
        assert!(catalog["models"].is_array());
    }
}
