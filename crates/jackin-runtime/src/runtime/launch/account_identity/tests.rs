// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_config::{AgentConfiguration, AppConfig};

fn envelope() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_str(
        r#"{"schema_version":2,"instances":{"work@claude":{"agent":"claude","account_id":"work","env":{"ANTHROPIC_API_KEY":"test-key"}}}}"#,
    )
    .unwrap()
}

#[test]
fn credentials_writer_persists_v2_envelope_privately() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let directory = temp.path().join("credentials");
    let path = directory.join("account-credentials.json");
    let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored["schema_version"], 2);
    assert_eq!(
        stored["instances"]["work@claude"]["env"]["ANTHROPIC_API_KEY"],
        "test-key"
    );
    assert_eq!(stored["instances"]["work@claude"]["account_id"], "work");
    let entries: Vec<_> = std::fs::read_dir(&directory).unwrap().collect();
    assert_eq!(entries.len(), 1, "atomic write must leave no temp files");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn fingerprint_covers_the_admitted_instance_set() {
    let base = AppConfig::default();
    let before = account_configuration_fingerprint(&base, None, "role").unwrap();
    assert_eq!(
        before,
        account_configuration_fingerprint(&base, None, "role").unwrap()
    );
    let mut with_config = base.clone();
    with_config.agent_configurations.insert(
        "primary".into(),
        AgentConfiguration {
            agent: jackin_core::Agent::Claude,
            account: "work".into(),
            model: None,
            base_url: None,
            display_label: None,
        },
    );
    assert_ne!(
        before,
        account_configuration_fingerprint(&with_config, None, "role").unwrap()
    );
    let mut with_default = base.clone();
    with_default.default_launch = Some(vec!["primary".into()]);
    assert_ne!(
        before,
        account_configuration_fingerprint(&with_default, None, "role").unwrap()
    );
}

#[test]
fn configuration_match_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let config = AppConfig::default();
    assert!(!account_configuration_matches(temp.path(), &config, None, "role").unwrap());
    assert!(!account_admission_matches(temp.path(), &config, None, "role").unwrap());
    let current = account_configuration_fingerprint(&config, None, "role").unwrap();
    std::fs::write(temp.path().join(ACCOUNT_FINGERPRINT_FILE), &current).unwrap();
    std::fs::write(temp.path().join("account-admission.sha256"), &current).unwrap();
    assert!(account_configuration_matches(temp.path(), &config, None, "role").unwrap());
    assert!(account_admission_matches(temp.path(), &config, None, "role").unwrap());
    let mut rotated = config.clone();
    rotated.default_launch = Some(vec!["other".into()]);
    assert!(!account_configuration_matches(temp.path(), &rotated, None, "role").unwrap());
    assert!(!account_admission_matches(temp.path(), &rotated, None, "role").unwrap());
}
