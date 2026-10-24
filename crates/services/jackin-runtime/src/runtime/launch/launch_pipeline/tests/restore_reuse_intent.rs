// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{current_role_reuse_is_compatible, validate_explicit_restore_options};
use crate::runtime::launch::LoadOptions;
use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider, AppConfig};
use jackin_core::{Agent, JackinPaths};
use jackin_instance::{AdmittedInstance, DockerResources, InstanceManifest, NewInstanceManifest};

const SAVED_ROLE: &str = "the-architect";

fn saved_restore_config() -> (AppConfig, Vec<AdmittedInstance>) {
    let mut config = AppConfig::default();
    let admitted = [("a", "a-instance"), ("b", "b-instance")]
        .into_iter()
        .map(|(account_id, config_id)| {
            config.accounts.insert(
                account_id.to_owned(),
                AccountConfig {
                    enabled: true,
                    name: account_id.to_owned(),
                    provider: AiProvider::Anthropic,
                    credential: AccountCredential::ApiKey {
                        value: format!("{account_id}-key").into(),
                        base_url: None,
                        model: None,
                    },
                },
            );
            config.agent_configurations.insert(
                config_id.to_owned(),
                AgentConfiguration {
                    agent: Agent::Claude,
                    account: account_id.to_owned(),
                    model: None,
                    base_url: None,
                    display_label: None,
                    invoked_via_wrapper: None,
                },
            );
            AdmittedInstance::new(config_id, Agent::Claude, account_id)
        })
        .collect();
    (config, admitted)
}

fn write_saved_restore_fixture(
    paths: &JackinPaths,
    container: &str,
    config: &AppConfig,
    admitted: &[AdmittedInstance],
) -> std::path::PathBuf {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, toml::to_string(config).unwrap()).unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();
    let root = paths.data_dir.join(container);
    std::fs::create_dir_all(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::symlink_metadata(&root).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&root, permissions).unwrap();
    }
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: container,
        workspace_name: None,
        workspace_label: "saved restore fixture",
        workdir: "/workspace",
        host_workdir_fingerprint: "saved-restore-fixture",
        role_key: SAVED_ROLE,
        role_display_name: "The Architect",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/architect.git",
        role_source_ref: Some("restore-ref"),
        image_tag: "saved-restore-fixture",
        docker: DockerResources::from_container_name(container),
        role_git_sha: Some("saved-role-git-sha".to_owned()),
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![Agent::Claude],
    });
    manifest.set_admitted_instances(admitted.iter().cloned());
    manifest.write(&root).unwrap();
    let fingerprint = crate::runtime::launch::account_identity::account_configuration_fingerprint(
        config, None, SAVED_ROLE, admitted,
    )
    .unwrap();
    std::fs::write(root.join("account-config.sha256"), fingerprint).unwrap();
    root
}

#[test]
fn current_role_reuse_rejects_launch_configuration_overrides() {
    assert!(current_role_reuse_is_compatible(&LoadOptions::default()));

    let options = LoadOptions {
        model: Some("gpt-6-luna".to_owned()),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        selection: Some(jackin_core::LaunchSelection::Account("work".to_owned())),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        selection: Some(jackin_core::LaunchSelection::Configuration(
            "codex-work".to_owned(),
        )),
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));

    let options = LoadOptions {
        non_interactive: true,
        ..Default::default()
    };
    assert!(!current_role_reuse_is_compatible(&options));
}

#[test]
fn exact_restore_rejects_launch_options_it_cannot_apply() {
    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        model: Some("gpt-6-luna".to_owned()),
        ..LoadOptions::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let error = validate_explicit_restore_options(&paths, &options).unwrap_err();
    assert!(error.to_string().contains("cannot apply"));

    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..LoadOptions::default()
    };
    assert!(validate_explicit_restore_options(&paths, &options).is_err());

    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        selection: Some(jackin_core::LaunchSelection::Account("work".to_owned())),
        ..LoadOptions::default()
    };
    let error = validate_explicit_restore_options(&paths, &options).unwrap_err();
    assert!(error.to_string().contains("selection"));

    let options = LoadOptions {
        restore_container_base: Some("jk-existing-role".to_owned()),
        non_interactive: true,
        ..LoadOptions::default()
    };
    assert!(validate_explicit_restore_options(&paths, &options).is_err());
}

#[test]
fn generic_v7_change_starts_fresh_but_explicit_restore_refuses_before_docker() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let (config, admitted) = saved_restore_config();
    let root = write_saved_restore_fixture(&paths, "jk-saved-account", &config, &admitted);
    let mut rotated = config.clone();
    if let AccountCredential::ApiKey { value, .. } =
        &mut rotated.accounts.get_mut("a").unwrap().credential
    {
        *value = "rotated-a-key".into();
    }

    let generic = crate::runtime::launch::restore_resolve::admit_restore(
        crate::runtime::launch::RestoreResolution::RestoreCurrentRole(
            "jk-saved-account".to_owned(),
        ),
        &paths.data_dir,
        &rotated,
        None,
        SAVED_ROLE,
    )
    .unwrap();
    assert_eq!(
        generic.resolution,
        crate::runtime::launch::RestoreResolution::StartFresh
    );
    assert!(generic.account.is_none());

    let error =
        super::admit_explicit_restore_account(&root, &rotated, None, SAVED_ROLE, None).unwrap_err();
    assert_eq!(
        error.to_string(),
        "instance account policy changed; launch a new instance"
    );
}

#[test]
fn generic_restore_rejects_invalid_tuple_when_v7_fingerprint_matches() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let (config, admitted) = saved_restore_config();
    let root = write_saved_restore_fixture(&paths, "jk-invalid-saved-account", &config, &admitted);
    let invalid = [AdmittedInstance::new("a-instance", Agent::Claude, "b")];
    let mut manifest = InstanceManifest::read(&root).unwrap();
    manifest.set_admitted_instances(invalid.iter().cloned());
    manifest.write(&root).unwrap();
    let fingerprint = crate::runtime::launch::account_identity::account_configuration_fingerprint(
        &config, None, SAVED_ROLE, &invalid,
    )
    .unwrap();
    std::fs::write(root.join("account-config.sha256"), fingerprint).unwrap();

    let error = crate::runtime::launch::restore_resolve::admit_restore(
        crate::runtime::launch::RestoreResolution::RestoreCurrentRole(
            "jk-invalid-saved-account".to_owned(),
        ),
        &paths.data_dir,
        &config,
        None,
        SAVED_ROLE,
    )
    .expect_err("matching v7 cannot authorize an invalid recorded tuple");

    assert!(
        error
            .to_string()
            .contains("saved account selection is no longer valid")
    );
}

#[test]
fn related_restore_rechecks_the_same_ordered_admission_before_recursing() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let (config, admitted) = saved_restore_config();
    let root = write_saved_restore_fixture(&paths, "jk-related-saved-account", &config, &admitted);
    let parent =
        super::admit_explicit_restore_account(&root, &config, None, SAVED_ROLE, None).unwrap();
    let expected = super::RelatedRestoreExpectation::from_admission(&parent);

    let child =
        super::admit_explicit_restore_account(&root, &config, None, SAVED_ROLE, Some(&expected))
            .unwrap();
    expected.verify(&child).unwrap();

    let unchanged =
        super::admit_explicit_restore_account(&root, &config, None, SAVED_ROLE, None).unwrap();
    assert_eq!(unchanged.fingerprint, expected.fingerprint);
    assert_eq!(
        unchanged.manifest.admitted_instances,
        parent.manifest.admitted_instances
    );

    let mut changed_pin = InstanceManifest::read(&root).unwrap();
    changed_pin.role_git_sha = Some("changed-role-git-sha".to_owned());
    changed_pin.write(&root).unwrap();
    let error =
        super::admit_explicit_restore_account(&root, &config, None, SAVED_ROLE, Some(&expected))
            .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("related restore admission changed")
    );

    let changed = [AdmittedInstance::new("b-instance", Agent::Claude, "b")];
    let mut manifest = InstanceManifest::read(&root).unwrap();
    manifest.role_git_sha = Some("saved-role-git-sha".to_owned());
    manifest.set_admitted_instances(changed.iter().cloned());
    manifest.write(&root).unwrap();
    let fingerprint = crate::runtime::launch::account_identity::account_configuration_fingerprint(
        &config, None, SAVED_ROLE, &changed,
    )
    .unwrap();
    std::fs::write(root.join("account-config.sha256"), fingerprint).unwrap();

    let error =
        super::admit_explicit_restore_account(&root, &config, None, SAVED_ROLE, Some(&expected))
            .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("related restore admission changed")
    );
}
