// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn orphan_sweep_removes_only_abandoned_stage_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let orphan = root.join(".credentials-stage-Ab12Cd");
    let active = root.join(".credentials-stage-Xy34Zw");
    std::fs::create_dir(&orphan).unwrap();
    std::fs::create_dir(&active).unwrap();
    std::fs::write(orphan.join("secret"), b"x").unwrap();
    std::fs::write(root.join(".credentials-stage-NotADir"), b"x").unwrap();
    std::fs::create_dir(root.join(".credentials-stage-bad_suffix")).unwrap();
    std::fs::create_dir(root.join("credentials")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&orphan, root.join(".credentials-stage-LiNk99")).unwrap();

    sweep_orphan_credential_staging(root, Some(".credentials-stage-Xy34Zw")).unwrap();

    assert!(!orphan.exists(), "orphan stage dir must be removed");
    assert!(active.is_dir(), "active stage dir must be preserved");
    assert!(
        root.join(".credentials-stage-NotADir").is_file(),
        "non-directory must be preserved"
    );
    assert!(
        root.join(".credentials-stage-bad_suffix").is_dir(),
        "malformed name must be preserved"
    );
    assert!(root.join("credentials").is_dir());
    #[cfg(unix)]
    assert!(
        std::fs::symlink_metadata(root.join(".credentials-stage-LiNk99"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "symlink must be preserved"
    );
}

#[test]
fn orphan_sweep_without_transaction_removes_all_stage_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join(".credentials-stage-Aa11Bb")).unwrap();
    sweep_orphan_credential_staging(root, None).unwrap();
    assert!(!root.join(".credentials-stage-Aa11Bb").exists());
}
