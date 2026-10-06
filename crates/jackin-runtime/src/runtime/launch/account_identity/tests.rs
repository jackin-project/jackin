// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::instance::{
    AdmittedInstance, DockerResources, InstanceManifest, NewInstanceManifest, RegistrationState,
};
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
fn credentials_writer_rolls_back_when_cleanup_fails() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let before = credential_snapshot(temp.path());

    {
        let _failure = inject_credential_write_failure(CredentialWriteFailure::Cleanup);
        let error = write_account_credentials(temp.path(), &replacement_envelope()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("injected credential publication failure")
        );
    }
    assert_eq!(credential_snapshot(temp.path()), before);
    assert_no_swap_artifacts(temp.path());
}

#[test]
fn credentials_writer_recovers_after_rollback_cleanup_fails() {
    let temp = tempfile::tempdir().unwrap();
    write_account_credentials(temp.path(), &envelope()).unwrap();
    let before = credential_snapshot(temp.path());

    {
        let _failure =
            inject_credential_write_failure(CredentialWriteFailure::InstallAndRollbackCleanup);
        let error = write_account_credentials(temp.path(), &replacement_envelope()).unwrap_err();
        assert!(error.to_string().contains("RollbackCleanup"));
    }
    assert_eq!(credential_snapshot(temp.path()), before);
    assert!(
        std::fs::read_dir(temp.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".credentials-")),
        "failed rollback cleanup must leave a recoverable transaction"
    );

    write_account_credentials(temp.path(), &replacement_envelope()).unwrap();
    assert_eq!(credential_snapshot(temp.path()).len(), 2);
    assert_no_swap_artifacts(temp.path());
}

fn api_key_account(id: &str) -> AccountConfig {
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

fn admitted_fingerprint_fixture() -> (AppConfig, Vec<AdmittedInstance>) {
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

#[test]
fn fingerprint_ignores_unrelated_account_lifecycle_changes() {
    let (base, admitted) = admitted_fingerprint_fixture();
    let before = account_configuration_fingerprint(&base, None, "role", &admitted).unwrap();
    assert_eq!(
        before,
        account_configuration_fingerprint(&base, None, "role", &admitted).unwrap()
    );

    let mut added = base.clone();
    added.accounts.insert("d".into(), api_key_account("d"));
    assert_eq!(
        before,
        account_configuration_fingerprint(&added, None, "role", &admitted).unwrap()
    );

    let mut renamed = added.clone();
    renamed.accounts.get_mut("d").unwrap().name = "D renamed".into();
    assert_eq!(
        before,
        account_configuration_fingerprint(&renamed, None, "role", &admitted).unwrap()
    );

    let mut disabled = renamed.clone();
    disabled.accounts.get_mut("d").unwrap().enabled = false;
    assert_eq!(
        before,
        account_configuration_fingerprint(&disabled, None, "role", &admitted).unwrap()
    );

    let mut unrelated_defaults = disabled.clone();
    unrelated_defaults.default_launch = Some(vec!["unrelated-d".into()]);
    assert_eq!(
        before,
        account_configuration_fingerprint(&unrelated_defaults, None, "role", &admitted).unwrap()
    );

    disabled.accounts.remove("d");
    assert_eq!(
        before,
        account_configuration_fingerprint(&disabled, None, "role", &admitted).unwrap()
    );

    let mut selected_credential_change = base.clone();
    selected_credential_change
        .accounts
        .get_mut("a")
        .unwrap()
        .credential = AccountCredential::ApiKey {
        value: "rotated-a-key".into(),
        base_url: None,
        model: None,
    };
    assert_ne!(
        before,
        account_configuration_fingerprint(&selected_credential_change, None, "role", &admitted)
            .unwrap()
    );

    let mut selected_capability_change = base;
    selected_capability_change
        .agent_configurations
        .get_mut("b-instance")
        .unwrap()
        .model = Some("new-model".into());
    assert_ne!(
        before,
        account_configuration_fingerprint(&selected_capability_change, None, "role", &admitted)
            .unwrap()
    );
}

#[test]
fn fingerprint_excludes_manifest_state_labels_and_ambient_bindings() {
    let (base, admitted) = admitted_fingerprint_fixture();
    let before = account_configuration_fingerprint(&base, None, "role", &admitted).unwrap();

    let mut state_changed = admitted.clone();
    state_changed[0].registration_state = RegistrationState::Disabled;
    assert_eq!(
        before,
        account_configuration_fingerprint(&base, None, "role", &state_changed).unwrap()
    );

    let mut labels_changed = base.clone();
    labels_changed
        .agent_configurations
        .get_mut("b-instance")
        .unwrap()
        .display_label = Some("renamed instance".into());
    assert_eq!(
        before,
        account_configuration_fingerprint(&labels_changed, None, "role", &admitted).unwrap()
    );

    let mut ambient_changed = labels_changed;
    ambient_changed
        .accounts
        .insert("d".into(), api_key_account("d"));
    ambient_changed
        .account_bindings
        .insert(Agent::Codex, "d".into());
    assert_eq!(
        before,
        account_configuration_fingerprint(&ambient_changed, None, "role", &admitted).unwrap()
    );
}

#[test]
fn configuration_match_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".jackin")).unwrap();
    let (config, admitted) = admitted_fingerprint_fixture();
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "fixture",
        workspace_name: None,
        workspace_label: "fixture",
        workdir: "/workspace",
        host_workdir_fingerprint: "fixture",
        role_key: "role",
        role_display_name: "Role",
        agent_runtime: Agent::Claude,
        role_source_git: "",
        role_source_ref: None,
        image_tag: "fixture",
        docker: DockerResources::from_container_name("fixture"),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.set_admitted_instances(admitted.iter().cloned());
    manifest.write(temp.path()).unwrap();
    assert!(!account_configuration_matches(temp.path(), &config, None, "role").unwrap());
    assert!(!account_admission_matches(temp.path(), &config, None, "role").unwrap());
    let current = account_configuration_fingerprint(&config, None, "role", &admitted).unwrap();
    std::fs::write(temp.path().join(ACCOUNT_FINGERPRINT_FILE), &current).unwrap();
    std::fs::write(temp.path().join("account-admission.sha256"), &current).unwrap();
    assert!(account_configuration_matches(temp.path(), &config, None, "role").unwrap());
    assert!(account_admission_matches(temp.path(), &config, None, "role").unwrap());
    let mut rotated = config.clone();
    rotated.accounts.get_mut("a").unwrap().enabled = false;
    assert!(!account_configuration_matches(temp.path(), &rotated, None, "role").unwrap());
    assert!(!account_admission_matches(temp.path(), &rotated, None, "role").unwrap());
}

#[test]
fn admission_record_rejects_rotation_between_staging_and_recording() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let (config, admitted) = admitted_fingerprint_fixture();
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();

    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    let (rotation_started_tx, rotation_started_rx) = std::sync::mpsc::channel();
    let (rotate_tx, rotate_rx) = std::sync::mpsc::channel();
    let rotated_paths = paths.clone();
    let mut rotated = config.clone();
    if let AccountCredential::ApiKey { value, .. } =
        &mut rotated.accounts.get_mut("a").unwrap().credential
    {
        *value = "rotated-a-key".into();
    }
    let rotation = std::thread::spawn(move || {
        rotation_started_tx.send(()).unwrap();
        rotate_rx.recv().unwrap();
        std::fs::write(
            &rotated_paths.config_file,
            toml::to_string(&rotated).unwrap(),
        )
        .unwrap();
    });
    rotation_started_rx.recv().unwrap();

    let credentials = jackin_env::resolve_instance_env_with(
        &config,
        &jackin_config::resolve_launch(&config, None, "role", None, Some(Agent::Claude)).unwrap(),
        None,
        "role",
        &jackin_env::OpCli::new(),
        |_| Err(std::env::VarError::NotPresent),
    )
    .unwrap();
    let root = temp.path().join("instance");
    write_account_credentials(&root, &credentials).unwrap();

    rotate_tx.send(()).unwrap();
    rotation.join().unwrap();

    let error = record_account_configuration(AccountConfigurationRecord {
        root: &root,
        paths: &paths,
        revision: &revision,
        config: &config,
        admission_config: &config,
        workspace: None,
        role: "role",
        admitted: &admitted,
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected error: {error:#}"
    );
    assert!(!root.join(ACCOUNT_FINGERPRINT_FILE).exists());
    assert!(!root.join("account-admission.sha256").exists());
}

fn write_generation_fixture(paths: &jackin_core::JackinPaths, config: &AppConfig) {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, toml::to_string(config).unwrap()).unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();
}

#[test]
fn required_generation_lease_rejects_missing_lock() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let config = AppConfig::default();
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let error = AccountConfigRevision::acquire(&paths)
        .expect_err("a missing config lock must fail admission");
    assert!(
        error.to_string().contains("required config lock"),
        "unexpected missing-lock error: {error:#}"
    );
}

#[test]
fn bound_generation_lease_rejects_stale_caller_config() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let persisted = AppConfig::default();
    write_generation_fixture(&paths, &persisted);

    let mut stale = persisted.clone();
    stale
        .env
        .insert("STALE_CALLER_SNAPSHOT".into(), EnvValue::from("old"));
    let error = AccountConfigRevision::acquire_bound(&paths, &stale)
        .expect_err("stale caller config must fail admission");
    assert!(
        error
            .to_string()
            .contains("caller configuration snapshot is stale"),
        "unexpected stale-caller error: {error:#}"
    );
}

#[test]
fn direct_writer_rotation_invalidates_held_generation_lease() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let config = AppConfig::default();
    write_generation_fixture(&paths, &config);
    let revision = AccountConfigRevision::acquire(&paths).unwrap();

    let mut rotated = config;
    rotated
        .env
        .insert("DIRECT_WRITER_ROTATION".into(), EnvValue::from("new"));
    std::fs::write(&paths.config_file, toml::to_string(&rotated).unwrap()).unwrap();

    let error = revision
        .ensure_current(&paths)
        .expect_err("direct config rotation must invalidate the lease");
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
}

#[test]
fn failed_admission_drops_lease_and_allows_retry() {
    let temp = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let config = AppConfig::default();
    write_generation_fixture(&paths, &config);

    {
        let revision = AccountConfigRevision::acquire(&paths).unwrap();
        let mut rotated = config.clone();
        rotated
            .env
            .insert("FAILED_ADMISSION".into(), EnvValue::from("first"));
        std::fs::write(&paths.config_file, toml::to_string(&rotated).unwrap()).unwrap();
        assert!(revision.ensure_current(&paths).is_err());
    }

    let retry_config = AppConfig {
        env: [("RETRY_AFTER_FAILURE".into(), EnvValue::from("ok"))]
            .into_iter()
            .collect(),
        ..AppConfig::default()
    };
    std::fs::write(&paths.config_file, toml::to_string(&retry_config).unwrap()).unwrap();
    let retry = AccountConfigRevision::acquire(&paths).unwrap();
    retry.ensure_current(&paths).unwrap();
}
