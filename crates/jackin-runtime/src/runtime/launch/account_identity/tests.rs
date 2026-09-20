// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider, AppConfig};
use jackin_core::{Agent, EnvValue};

fn envelope() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_str(
        r#"{"schema_version":2,"instances":{"work@claude":{"agent":"claude","account_id":"work","env":{"ANTHROPIC_API_KEY":"test-key"}}}}"#,
    )
    .unwrap()
}

fn replacement_envelope() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_str(
        r#"{"schema_version":2,"instances":{"new-a@claude":{"agent":"claude","account_id":"new-a","env":{"ANTHROPIC_API_KEY":"new-a-key"}},"new-b@claude":{"agent":"claude","account_id":"new-b","env":{"ANTHROPIC_API_KEY":"new-b-key"}}}}"#,
    )
    .unwrap()
}

fn credential_snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
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

fn assert_no_swap_artifacts(root: &Path) {
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

#[test]
fn credentials_writer_persists_one_staged_file_privately() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let directory = temp.path().join("credentials");
    let path = directory.join(jackin_protocol::account_credentials_filename("work@claude"));
    let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored["schema_version"], 1);
    assert_eq!(stored["credential"]["env"]["ANTHROPIC_API_KEY"], "test-key");
    assert_eq!(stored["credential"]["account_id"], "work");
    assert_eq!(stored["instance"], "work@claude");
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
fn credentials_writer_stages_same_account_oauth_routes_per_instance() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "shared".into(),
        AccountConfig {
            enabled: true,
            name: "Shared Claude".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::OAuthToken {
                agent: Agent::Claude,
                value: EnvValue::from("shared-oauth"),
            },
        },
    );
    config.accounts.insert(
        "unselected".into(),
        AccountConfig {
            enabled: true,
            name: "Unselected Claude".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::OAuthToken {
                agent: Agent::Claude,
                value: EnvValue::from("unselected-oauth"),
            },
        },
    );
    for (id, endpoint) in [
        ("claude-work", "https://work.example/v1"),
        ("claude-personal", "https://personal.example/v1"),
    ] {
        config.agent_configurations.insert(
            id.into(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "shared".into(),
                model: None,
                base_url: Some(endpoint.into()),
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let ids = vec!["claude-work".into(), "claude-personal".into()];
    let instances = jackin_config::resolve_launch(&config, None, "role", Some(&ids), None).unwrap();
    let credentials = jackin_env::resolve_instance_env_with(
        &config,
        &instances,
        None,
        "role",
        &jackin_env::OpCli::new(),
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();

    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &credentials).unwrap();
    let directory = temp.path().join("credentials");
    assert_eq!(directory.read_dir().unwrap().count(), 2);
    for (id, endpoint) in [
        ("claude-work", "https://work.example/v1"),
        ("claude-personal", "https://personal.example/v1"),
    ] {
        let path = directory.join(jackin_protocol::account_credentials_filename(id));
        let staged: jackin_protocol::StagedInstanceCredential =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(staged.instance, id);
        assert_eq!(staged.credential.agent, "claude");
        assert_eq!(staged.credential.account_id, "shared");
        assert_eq!(
            staged.credential.env,
            std::collections::BTreeMap::from([
                ("ANTHROPIC_BASE_URL".into(), endpoint.into()),
                ("CLAUDE_CODE_OAUTH_TOKEN".into(), "shared-oauth".into()),
            ])
        );
        assert!(
            !staged
                .credential
                .env
                .values()
                .any(|value| value == "unselected-oauth")
        );
    }
}

#[test]
fn credentials_writer_revokes_stale_instance_files() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let empty = jackin_protocol::AgentCredentialEnv::default();
    write_account_credentials(temp.path(), &empty).unwrap();
    assert_eq!(
        std::fs::read_dir(temp.path().join("credentials"))
            .unwrap()
            .count(),
        0,
        "revocation must remove every prior staged instance file"
    );
}

#[test]
fn credentials_writer_staging_failure_preserves_complete_previous_set() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let before = credential_snapshot(temp.path());

    {
        let _failure = inject_credential_write_failure(CredentialWriteFailure::StagedFile(0));
        let error = write_account_credentials(temp.path(), &replacement_envelope()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("injected credential publication failure")
        );
    }
    assert_eq!(credential_snapshot(temp.path()), before);
    assert_no_swap_artifacts(temp.path());

    write_account_credentials(temp.path(), &replacement_envelope()).unwrap();
    assert_eq!(credential_snapshot(temp.path()).len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(temp.path().join("credentials"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for entry in std::fs::read_dir(temp.path().join("credentials")).unwrap() {
            assert_eq!(
                entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    assert_no_swap_artifacts(temp.path());
}

#[test]
fn credentials_writer_rolls_back_after_moving_previous_directory() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let before = credential_snapshot(temp.path());

    let _failure = inject_credential_write_failure(CredentialWriteFailure::PreviousRename);
    let error = write_account_credentials(temp.path(), &replacement_envelope()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected credential publication failure")
    );
    assert_eq!(credential_snapshot(temp.path()), before);
    assert_no_swap_artifacts(temp.path());
}

#[test]
fn credentials_writer_rolls_back_after_install_before_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let before = credential_snapshot(temp.path());

    let _failure = inject_credential_write_failure(CredentialWriteFailure::Install);
    let error = write_account_credentials(temp.path(), &replacement_envelope()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected credential publication failure")
    );
    assert_eq!(credential_snapshot(temp.path()), before);
    assert_no_swap_artifacts(temp.path());
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
            agent: Agent::Claude,
            account: "work".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
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
