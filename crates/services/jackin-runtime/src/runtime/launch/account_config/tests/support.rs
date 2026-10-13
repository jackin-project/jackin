// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn slots_for(
    instances: &[jackin_config::ResolvedInstance],
) -> BTreeMap<String, crate::instance::ProvisionedInstanceAuth> {
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

pub(super) fn configure_for_test(
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
    configure_accounts(root, config, instances, &slots, &models, &BTreeMap::new()).map(|_| ())
}

pub(super) fn configure_with_models(
    root: &Path,
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    models: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let slots = slots_for(instances);
    configure_accounts(root, config, instances, &slots, models, &BTreeMap::new()).map(|_| ())
}

pub(super) fn instance(
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

pub(super) fn codex_moonshot_fixture() -> (AppConfig, [jackin_config::ResolvedInstance; 1]) {
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

pub(super) fn quarantined_payload(directory: &Path, expected: &[u8]) -> anyhow::Result<()> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| anyhow::anyhow!("read private config quarantine directory: {error}"))?;
    let matches: Vec<_> = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| anyhow::anyhow!("read quarantine directory entry: {error}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("config.toml.corrupt-"))
        })
        .collect();
    anyhow::ensure!(matches.len() == 1, "expected one quarantine file");
    let quarantine = matches
        .first()
        .ok_or_else(|| anyhow::anyhow!("expected one quarantine file"))?;
    let name = quarantine
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("quarantine file has no name"))?
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("quarantine file name is not valid UTF-8"))?
        .to_owned();
    let suffix = name
        .strip_prefix("config.toml.corrupt-")
        .ok_or_else(|| anyhow::anyhow!("quarantine file name has an unexpected prefix: {name}"))?;
    let (secs, pid) = suffix
        .rsplit_once('-')
        .ok_or_else(|| anyhow::anyhow!("quarantine file name has no PID suffix: {name}"))?;
    let _secs = secs
        .parse::<u64>()
        .map_err(|error| anyhow::anyhow!("invalid unix-secs suffix in {name}: {error}"))?;
    let pid = pid
        .parse::<u32>()
        .map_err(|error| anyhow::anyhow!("invalid PID suffix in {name}: {error}"))?;
    assert_eq!(pid, std::process::id());
    let payload = std::fs::read(quarantine)
        .map_err(|error| anyhow::anyhow!("read quarantined config payload: {error}"))?;
    assert_eq!(payload, expected);
    Ok(())
}
