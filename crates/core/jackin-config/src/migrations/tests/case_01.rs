// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn migrates_missing_config_version_to_current() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(
        &path,
        "# keep me\n\n[roles.agent-smith]\ngit = \"https://example.test/role.git\"\n",
    )
    .unwrap();

    assert!(migrate_config_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(parsed["version"].as_str().unwrap(), CURRENT_CONFIG_VERSION);
    assert!(out.contains("# keep me"), "{out}");
}

#[test]
fn config_migration_waits_for_the_shared_writer_lock() {
    use std::sync::mpsc;
    use std::time::Duration;

    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, "version = \"v1alpha10\"\n").unwrap();
    let guard = acquire_config_write_lock(&path).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let migration_path = path.clone();
    let waiter = std::thread::spawn(move || {
        let result = migrate_config_file_if_needed(&migration_path);
        done_tx.send(result).unwrap();
    });

    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(guard);
    done_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("migration thread did not finish")
        .expect("migration failed");
    waiter.join().unwrap();
}

#[test]
fn migrates_missing_workspace_version_to_current() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("prod.toml");
    std::fs::write(&path, "# keep me\nworkdir = \"/workspace/prod\"\n").unwrap();

    assert!(migrate_workspace_file_if_needed(&path).unwrap());
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(
        parsed["version"].as_str().unwrap(),
        CURRENT_WORKSPACE_VERSION
    );
    assert!(out.contains("# keep me"), "{out}");
}

#[test]
fn already_current_workspace_is_a_no_op() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("prod.toml");
    std::fs::write(
        &path,
        format!("version = \"{CURRENT_WORKSPACE_VERSION}\"\nworkdir = \"/workspace/prod\"\n"),
    )
    .unwrap();

    assert!(!migrate_workspace_file_if_needed(&path).unwrap());
}

#[test]
fn rejects_newer_config_version() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, r#"version = "v2alpha1""#).unwrap();

    let err = migrate_config_file_if_needed(&path).unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("only understands up to {CURRENT_CONFIG_VERSION}"))
    );
}

#[test]
fn rejects_invalid_version() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, r#"version = "0.1.0""#).unwrap();

    let err = migrate_config_file_if_needed(&path).unwrap_err();
    assert!(err.to_string().contains("version is invalid"));
}

#[test]
fn rejects_non_string_version_field() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, "version = 42\n").unwrap();

    let err = migrate_config_file_if_needed(&path).unwrap_err();
    assert!(
        err.to_string().contains("version must be a string"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_zero_major() {
    let err = parse_version("v0").unwrap_err();
    assert!(
        err.to_string()
            .contains("major version must be greater than zero"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_zero_alpha_sequence() {
    let err = parse_version("v1alpha0").unwrap_err();
    assert!(
        err.to_string()
            .contains("alpha sequence must be greater than zero"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_alpha_without_sequence() {
    let err = parse_version("v1alpha").unwrap_err();
    assert!(
        err.to_string()
            .contains("alpha version must include a sequence number"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_beta_without_sequence() {
    let err = parse_version("v1beta").unwrap_err();
    assert!(
        err.to_string()
            .contains("beta version must include a sequence number"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_unknown_channel() {
    let err = parse_version("v1gamma1").unwrap_err();
    assert!(
        err.to_string()
            .contains("must look like v1, v1beta1, or v1alpha1"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_missing_v_prefix() {
    let err = parse_version("1alpha1").unwrap_err();
    assert!(err.to_string().contains("must start with `v`"), "{err}");
}

#[test]
fn parse_version_rejects_no_digits() {
    let err = parse_version("vabc").unwrap_err();
    assert!(err.to_string().contains("missing major version"), "{err}");
}

#[test]
fn parse_version_rejects_leading_zero_major() {
    let err = parse_version("v01alpha1").unwrap_err();
    assert!(
        err.to_string()
            .contains("major version must not have leading zeros"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_leading_zero_alpha_sequence() {
    let err = parse_version("v1alpha01").unwrap_err();
    assert!(
        err.to_string()
            .contains("alpha sequence must not have leading zeros"),
        "{err}"
    );
}

#[test]
fn parse_version_rejects_u32_overflow() {
    let err = parse_version("v9999999999").unwrap_err();
    assert!(err.to_string().contains("invalid major version"), "{err}");
}

#[test]
fn parse_version_accepts_canonical_forms() {
    assert_eq!(
        parse_version("v1").unwrap(),
        SchemaVersion::Kubernetes(KubernetesVersion {
            major: nz(1),
            channel: Channel::Stable,
        })
    );
    assert_eq!(
        parse_version("v1alpha1").unwrap(),
        SchemaVersion::Kubernetes(KubernetesVersion {
            major: nz(1),
            channel: Channel::Alpha(nz(1)),
        })
    );
    assert_eq!(
        parse_version("v2beta3").unwrap(),
        SchemaVersion::Kubernetes(KubernetesVersion {
            major: nz(2),
            channel: Channel::Beta(nz(3)),
        })
    );
}

#[test]
fn channel_order_is_alpha_beta_stable() {
    assert!(Channel::Alpha(nz(1)) < Channel::Beta(nz(1)));
    assert!(Channel::Beta(nz(1)) < Channel::Stable);
    // Within a channel, sequence orders.
    assert!(Channel::Alpha(nz(1)) < Channel::Alpha(nz(2)));
    // Cross-channel beats sequence: a high alpha is still less than any
    // beta.
    assert!(Channel::Alpha(nz(99)) < Channel::Beta(nz(1)));
}

#[test]
fn config_migrations_chain_reaches_current() {
    assert_registry_reaches(CONFIG_MIGRATIONS, CURRENT_CONFIG_VERSION);
}

#[test]
fn parse_registry_version_handles_legacy_sentinel() {
    assert_eq!(
        parse_registry_version("legacy").unwrap(),
        SchemaVersion::Legacy
    );
    // Non-sentinel strings delegate to parse_version.
    parse_registry_version("legacyfoo").unwrap_err();
    assert_eq!(
        parse_registry_version("v1alpha1").unwrap(),
        parse_version("v1alpha1").unwrap()
    );
}

#[test]
fn workspace_migrations_chain_reaches_current() {
    assert_registry_reaches(WORKSPACE_MIGRATIONS, CURRENT_WORKSPACE_VERSION);
}

#[test]
fn kubernetes_versions_sort_by_stability_and_sequence() {
    assert!(parse_version("v1alpha1").unwrap() < parse_version("v1alpha2").unwrap());
    assert!(parse_version("v1alpha2").unwrap() < parse_version("v1beta1").unwrap());
    assert!(parse_version("v1beta1").unwrap() < parse_version("v1").unwrap());
    assert!(parse_version("v1").unwrap() < parse_version("v2alpha1").unwrap());
}

#[test]
fn legacy_orders_below_every_kubernetes_version() {
    assert!(SchemaVersion::Legacy < parse_version("v1alpha1").unwrap());
    assert!(SchemaVersion::Legacy < parse_version("v1").unwrap());
}

#[test]
fn rejects_when_migration_path_was_removed() {
    let old = SchemaVersion::Legacy;
    let current = parse_version("v1alpha1").unwrap();
    let mut doc = DocumentMut::new();

    let err = apply_migrations(&mut doc, &old, &current, &[], "config").unwrap_err();

    assert!(
        err.to_string()
            .contains("no longer includes a migration path")
    );
}

#[test]
fn rejects_when_middle_migration_path_was_removed() {
    let old = parse_version("v1alpha1").unwrap();
    let current = parse_version("v1alpha4").unwrap();
    // No content mutation: framework stamps `step.to` after each step.
    let migrations = [MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: noop_migration,
    }];
    let mut doc = DocumentMut::new();

    let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

    assert!(
        err.to_string()
            .contains("no longer includes a migration path")
    );
}

#[test]
fn rejects_backward_step_in_registry() {
    let old = SchemaVersion::Legacy;
    let current = parse_version("v1alpha1").unwrap();
    let migrations = [MigrationStep {
        from: LEGACY_VERSION,
        to: LEGACY_VERSION,
        migrate: noop_migration,
    }];
    let mut doc = DocumentMut::new();

    let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

    assert!(
        err.to_string()
            .contains("registry is invalid: step legacy -> legacy does not move forward"),
        "{err}"
    );
}

#[test]
fn rejects_when_chain_overshoots_current() {
    let old = SchemaVersion::Legacy;
    let current = parse_version("v1alpha1").unwrap();
    let migrations = [MigrationStep {
        from: LEGACY_VERSION,
        to: "v1alpha2",
        migrate: noop_migration,
    }];
    let mut doc = DocumentMut::new();

    let err = apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap_err();

    assert!(
        err.to_string()
            .contains("registry stopped at v1alpha2, expected v1alpha1"),
        "{err}"
    );
}

#[test]
fn applies_multi_step_chain_in_order() {
    // Each step appends a marker key so the final doc captures the
    // execution order — a regression that double-applies, skips, or
    // reorders steps changes the marker.

    let old = parse_version("v1alpha1").unwrap();
    let current = parse_version("v1alpha4").unwrap();
    let migrations = [
        MigrationStep {
            from: "v1alpha1",
            to: "v1alpha2",
            migrate: alpha1_to_alpha2,
        },
        MigrationStep {
            from: "v1alpha2",
            to: "v1alpha3",
            migrate: alpha2_to_alpha3,
        },
        MigrationStep {
            from: "v1alpha3",
            to: "v1alpha4",
            migrate: noop_migration,
        },
    ];
    let mut doc = DocumentMut::new();

    apply_migrations(&mut doc, &old, &current, &migrations, "config").unwrap();

    assert_eq!(doc["alpha1_to_alpha2"].as_bool(), Some(true));
    assert_eq!(doc["alpha2_to_alpha3"].as_bool(), Some(true));
    assert_eq!(doc["version"].as_str(), Some("v1alpha4"));
}
