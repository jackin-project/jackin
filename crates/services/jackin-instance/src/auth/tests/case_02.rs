// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn selected_snapshot_pins_amp_kimi_and_opencode_material() {
    let temp = tempdir().unwrap();
    let snapshot_parent = private_snapshot_parent(&temp);

    let amp_source = temp.path().join("amp");
    std::fs::create_dir_all(&amp_source).unwrap();
    std::fs::write(amp_source.join("secrets.json"), "{\"token\":\"old-amp\"}").unwrap();
    let amp_snapshot = capture_selected_source(
        Agent::Amp,
        None,
        None,
        &amp_source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("Amp source snapshot");
    std::fs::write(amp_source.join("secrets.json"), "{\"token\":\"new-amp\"}").unwrap();
    let amp_target = temp.path().join("role/amp/secrets.json");
    std::fs::create_dir_all(amp_target.parent().unwrap()).unwrap();
    let (outcome, mounted) = RoleState::provision_amp_auth_from_source_dir(
        &amp_target,
        AuthForwardMode::Sync,
        amp_snapshot.materialized_source_dir(),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(amp_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&amp_target).unwrap(),
        "{\"token\":\"old-amp\"}"
    );

    let kimi_source = temp.path().join("kimi");
    std::fs::create_dir_all(kimi_source.join("credentials")).unwrap();
    std::fs::write(kimi_source.join("config.toml"), "version = \"old\"\n").unwrap();
    std::fs::write(kimi_source.join("credentials/token"), "old-kimi").unwrap();
    let kimi_snapshot = capture_selected_source(
        Agent::Kimi,
        None,
        None,
        &kimi_source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("Kimi source snapshot");
    std::fs::write(kimi_source.join("config.toml"), "version = \"new\"\n").unwrap();
    std::fs::write(kimi_source.join("credentials/token"), "new-kimi").unwrap();
    let kimi_target = temp.path().join("role/kimi");
    let (outcome, mounted) = RoleState::provision_kimi_auth_from_source_dir(
        &kimi_target,
        AuthForwardMode::Sync,
        kimi_snapshot.materialized_source_dir(),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(mounted);
    assert_kimi_snapshot_credentials(&kimi_target);

    let opencode_source = temp.path().join("opencode");
    std::fs::create_dir_all(&opencode_source).unwrap();
    std::fs::write(
        opencode_source.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"old-opencode"}}"#,
    )
    .unwrap();
    let opencode_snapshot = capture_selected_source(
        Agent::Opencode,
        Some(AiProvider::Opencode),
        None,
        &opencode_source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("OpenCode source snapshot");
    std::fs::write(
        opencode_source.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"new-opencode"}}"#,
    )
    .unwrap();
    let opencode_target = temp.path().join("role/opencode/auth.json");
    std::fs::create_dir_all(opencode_target.parent().unwrap()).unwrap();
    let (outcome, mounted) = RoleState::provision_opencode_auth_from_source_dir(
        &opencode_target,
        AuthForwardMode::Sync,
        opencode_snapshot.materialized_source_dir(),
        Some(AiProvider::Opencode),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(opencode_target.as_path()));
    assert!(
        std::fs::read_to_string(opencode_target)
            .unwrap()
            .contains("old-opencode")
    );

    let hermes_source = temp.path().join("hermes");
    std::fs::create_dir_all(hermes_source.join("profiles")).unwrap();
    std::fs::write(
        hermes_source.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        hermes_source.join("auth.json"),
        r#"{"openai":{"type":"api","key":"old-hermes"}}"#,
    )
    .unwrap();
    let selector = ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some("work".to_owned()),
    };
    let hermes_snapshot = capture_selected_source(
        Agent::Hermes,
        Some(AiProvider::OpenAi),
        Some(&selector),
        &hermes_source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("Hermes source snapshot");
    std::fs::write(
        hermes_source.join("config.yaml"),
        "profiles:\n  other:\n    provider: anthropic\n",
    )
    .unwrap();
    std::fs::write(
        hermes_source.join("auth.json"),
        r#"{"anthropic":{"type":"api","key":"new-hermes"}}"#,
    )
    .unwrap();
    let hermes_target = temp.path().join("role/hermes");
    let (outcome, mounted) = RoleState::provision_hermes_auth_from_source_dir(
        &hermes_target,
        AuthForwardMode::Sync,
        hermes_snapshot.materialized_source_dir(),
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(mounted);
    assert!(
        std::fs::read_to_string(hermes_target.join("auth.json"))
            .unwrap()
            .contains("old-hermes")
    );
}

#[cfg(unix)]
#[test]
fn selected_snapshot_rejects_invalid_utf8_and_oversized_credentials() {
    let temp = tempdir().unwrap();
    let snapshot_parent = private_snapshot_parent(&temp);
    let source = temp.path().join("codex");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("auth.json"), [0xff, 0xfe]).unwrap();
    let error = capture_selected_source(
        Agent::Codex,
        None,
        None,
        &source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap_err();
    assert!(error.to_string().contains("not valid UTF-8"), "{error:#}");

    std::fs::write(
        source.join("auth.json"),
        vec![b'x'; MAX_AUTH_SOURCE_FILE_BYTES + 1],
    )
    .unwrap();
    let error = capture_selected_source(
        Agent::Codex,
        None,
        None,
        &source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap_err();
    assert!(error.to_string().contains("size limit"), "{error:#}");
}

#[cfg(unix)]
#[test]
fn selected_snapshot_creates_missing_protected_parent() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let source = temp.path().join("codex");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("auth.json"), TEST_CREDENTIALS).unwrap();
    let snapshot_parent = temp
        .path()
        .join("protected/provider-config/source-snapshots");
    assert!(!snapshot_parent.exists());

    let snapshot = capture_selected_source(
        Agent::Codex,
        None,
        None,
        &source,
        temp.path(),
        &snapshot_parent,
    )
    .unwrap()
    .expect("source snapshot");

    let canonical_parent = std::fs::canonicalize(&snapshot_parent).unwrap();
    assert!(
        snapshot
            .materialized_source_dir()
            .starts_with(&canonical_parent)
    );
    assert_eq!(
        std::fs::read_to_string(snapshot.materialized_source_dir().join("auth.json")).unwrap(),
        TEST_CREDENTIALS
    );
    let mode = std::fs::symlink_metadata(&snapshot_parent)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o700, "snapshot parent must remain private");
}

#[cfg(unix)]
#[test]
fn selected_snapshot_rejects_symlink_parent_traversal() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let source = temp.path().join("real");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("auth.json"), TEST_CREDENTIALS).unwrap();
    symlink(&source, temp.path().join("link")).unwrap();
    let descriptor = temp.path().join("link/../real");

    let error = capture_selected_source(
        Agent::Codex,
        None,
        None,
        &descriptor,
        temp.path(),
        temp.path(),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("parent traversal"),
        "symlink + parent traversal must be rejected: {error:#}"
    );
}

#[cfg(unix)]
#[test]
fn source_lock_timeout_is_bounded() {
    use std::time::{Duration, Instant};

    let temp = tempdir().unwrap();
    let source = temp.path().join("codex");
    std::fs::create_dir_all(&source).unwrap();
    let _holder = lock_source_dir_for_test(&source, Duration::from_millis(25))
        .unwrap()
        .expect("source lock holder");

    let started = Instant::now();
    let error = lock_source_dir_for_test(&source, Duration::from_millis(25)).unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "contended source lock exceeded bounded test deadline"
    );
    assert!(error.to_string().contains("timed out"), "{error:#}");
}

#[test]
fn validate_single_file_agents() {
    let temp = tempdir().unwrap();
    for (agent, name) in [
        (Agent::Codex, "auth.json"),
        (Agent::Grok, "auth.json"),
        (Agent::Opencode, "auth.json"),
        (Agent::Amp, "secrets.json"),
    ] {
        let dir = temp.path().join(format!("{agent:?}-good"));
        std::fs::create_dir_all(&dir).unwrap();
        // Empty file is rejected.
        std::fs::write(dir.join(name), "").unwrap();
        validate_sync_source_dir(agent, &dir, temp.path())
            .expect_err("empty credential file must be rejected");
        // Non-empty credential file is accepted.
        let valid = if agent == Agent::Opencode {
            r#"{"opencode-go":{"type":"api","key":"x"}}"#
        } else {
            "{\"token\":\"x\"}"
        };
        std::fs::write(dir.join(name), valid).unwrap();
        validate_sync_source_dir(agent, &dir, temp.path()).unwrap_or_else(|_| {
            panic!("valid {name} must be accepted");
        });
        // Wrong folder (no credential file) is rejected.
        let bad = temp.path().join(format!("{agent:?}-bad"));
        std::fs::create_dir_all(&bad).unwrap();
        validate_sync_source_dir(agent, &bad, temp.path()).unwrap_err();
    }
}
