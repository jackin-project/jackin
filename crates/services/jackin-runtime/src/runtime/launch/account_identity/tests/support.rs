// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn envelope() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_str(
        r#"{"schema_version":2,"instances":{"work@claude":{"agent":"claude","account_id":"work","env":{"ANTHROPIC_API_KEY":"test-key"}}}}"#,
    )
    .unwrap()
}

pub(super) fn replacement_envelope() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_str(
        r#"{"schema_version":2,"instances":{"new-a@claude":{"agent":"claude","account_id":"new-a","env":{"ANTHROPIC_API_KEY":"new-a-key"}},"new-b@claude":{"agent":"claude","account_id":"new-b","env":{"ANTHROPIC_API_KEY":"new-b-key"}}}}"#,
    )
    .unwrap()
}

pub(super) fn credential_snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(root.join("credentials"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

pub(super) fn assert_no_swap_artifacts(root: &Path) {
    let artifacts: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".credentials-"))
        .collect();
    assert!(
        artifacts.is_empty(),
        "credential swap artifacts remain: {artifacts:?}"
    );
}

pub(super) fn api_key_account(id: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: id.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: format!("{id}-key").into(),
            base_url: None,
            model: None,
        },
    }
}

pub(super) fn admitted_fingerprint_fixture() -> (AppConfig, Vec<AdmittedInstance>) {
    let mut config = AppConfig::default();
    for account_id in ["a", "b", "c"] {
        config
            .accounts
            .insert(account_id.into(), api_key_account(account_id));
        config.agent_configurations.insert(
            format!("{account_id}-instance"),
            AgentConfiguration {
                agent: Agent::Claude,
                account: account_id.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    config.default_launch = Some(
        ["a-instance", "b-instance", "c-instance"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let admitted = [
        ("a-instance", "a"),
        ("b-instance", "b"),
        ("c-instance", "c"),
    ]
    .into_iter()
    .map(|(config_id, account_id)| AdmittedInstance::new(config_id, Agent::Claude, account_id))
    .collect();
    (config, admitted)
}

pub(super) fn write_generation_fixture(paths: &jackin_core::JackinPaths, config: &AppConfig) {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, toml::to_string(config).unwrap()).unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();
}
