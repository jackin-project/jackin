// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_instance::{AdmittedInstance, RegistrationState};

use crate::account_identity::account_configuration_fingerprint;

fn admitted(config_id: &str, agent: Agent, account_id: &str) -> AdmittedInstance {
    AdmittedInstance {
        config_id: config_id.to_owned(),
        agent,
        account_id: account_id.to_owned(),
        registration_state: RegistrationState::default(),
    }
}

fn is_canonical_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[test]
fn empty_admission_produces_stable_digest() -> anyhow::Result<()> {
    let config = AppConfig::default();
    let first = account_configuration_fingerprint(&config, None, "test-role", &[])?;
    let second = account_configuration_fingerprint(&config, None, "test-role", &[])?;
    assert!(is_canonical_hex_digest(&first));
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn admission_order_does_not_change_digest() -> anyhow::Result<()> {
    let config = AppConfig::default();
    let forward = [
        admitted("cfg-a", Agent::Claude, "acct-1"),
        admitted("cfg-b", Agent::Codex, "acct-2"),
    ];
    let backward = [forward[1].clone(), forward[0].clone()];
    let first = account_configuration_fingerprint(&config, None, "test-role", &forward)?;
    let second = account_configuration_fingerprint(&config, None, "test-role", &backward)?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn different_admission_changes_digest() -> anyhow::Result<()> {
    let config = AppConfig::default();
    let baseline = account_configuration_fingerprint(
        &config,
        None,
        "test-role",
        &[admitted("cfg-a", Agent::Claude, "acct-1")],
    )?;
    let changed = account_configuration_fingerprint(
        &config,
        None,
        "test-role",
        &[admitted("cfg-a", Agent::Claude, "acct-9")],
    )?;
    assert!(is_canonical_hex_digest(&baseline));
    assert!(is_canonical_hex_digest(&changed));
    assert_ne!(baseline, changed);
    Ok(())
}
