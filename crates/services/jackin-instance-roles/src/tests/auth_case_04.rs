// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn ignore_recovers_interrupted_swap_when_destination_is_absent() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.kimi-code");
    let target_dir = temp.path().join("role/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(source_dir.join("config.toml"), "version = \"old\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "old-token").unwrap();
    provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Sync, &source_dir).unwrap();

    std::fs::write(source_dir.join("config.toml"), "version = \"new\"\n").unwrap();
    let crash = inject_failure(FailurePoint::Backup);
    provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Sync, &source_dir)
        .unwrap_err();
    drop(crash);
    assert!(!target_dir.exists());

    let (outcome, forward_auth) =
        provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Ignore, &source_dir)
            .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!forward_auth);
    assert!(
        !target_dir.exists(),
        "Ignore must revoke the recovered tree"
    );
    let parent = target_dir.parent().unwrap();
    assert!(
        !std::fs::read_dir(parent).unwrap().any(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            name.starts_with(".jackin-auth-stage-")
                || name.starts_with(".jackin-auth-previous-")
                || name.starts_with(".jackin-auth-journal-")
        }),
        "Ignore must clean recovered transaction trees even without a target"
    );
}

#[cfg(unix)]
#[test]
fn auth_lock_identity_normalizes_relative_absolute_and_dot_aliases() {
    let temp = tempdir().unwrap();
    let absolute = temp.path().join("role/.kimi-code");
    let dot_alias = temp.path().join("role/./.kimi-code");
    assert_eq!(
        target_lock_key_for_test(&absolute).unwrap(),
        target_lock_key_for_test(&dot_alias).unwrap()
    );

    let current = std::env::current_dir().unwrap();
    let relative = Path::new("target/./auth");
    let absolute_from_relative = current.join("target/auth");
    assert_eq!(
        target_lock_key_for_test(relative).unwrap(),
        target_lock_key_for_test(&absolute_from_relative).unwrap()
    );

    let mut escaping = Path::new("").to_path_buf();
    for _ in 0..=current.components().count() {
        escaping.push("..");
    }
    escaping.push("auth");
    let error = target_lock_key_for_test(&escaping).unwrap_err();
    assert!(
        error.to_string().contains("parent traversal"),
        "root escape must be rejected: {error:#}"
    );
}

#[cfg(unix)]
#[test]
fn directory_swap_serializes_concurrent_replacements_per_target() {
    use std::sync::{Arc, Barrier};

    let temp = tempdir().unwrap();
    let source_a = temp.path().join("host-a/.kimi-code");
    let source_b = temp.path().join("host-b/.kimi-code");
    let target = temp.path().join("role/.kimi-code");
    for (source, value) in [(&source_a, "a"), (&source_b, "b")] {
        std::fs::create_dir_all(source.join("credentials")).unwrap();
        std::fs::write(source.join("config.toml"), format!("value = \"{value}\"\n")).unwrap();
        std::fs::write(source.join("credentials/token"), format!("{value}-token")).unwrap();
    }

    let barrier = Arc::new(Barrier::new(3));
    let target_alias = target.parent().unwrap().join(".").join(".kimi-code");
    std::thread::scope(|scope| {
        for (source, target_path) in [
            (&source_a, target.as_path()),
            (&source_b, target_alias.as_path()),
        ] {
            let barrier = Arc::clone(&barrier);
            scope.spawn(move || {
                barrier.wait();
                provision_kimi_auth_from_source_dir(target_path, AuthForwardMode::Sync, source)
                    .unwrap();
            });
        }
        barrier.wait();
    });

    let config = std::fs::read_to_string(target.join("config.toml")).unwrap();
    let token = std::fs::read_to_string(target.join("credentials/token")).unwrap();
    assert!(config == "value = \"a\"\n" || config == "value = \"b\"\n");
    assert!(token == "a-token" || token == "b-token");
    assert_eq!(config.as_bytes().last(), Some(&b'\n'));
    assert!(
        (config.contains('a') && token == "a-token")
            || (config.contains('b') && token == "b-token")
    );
    let lock_files = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".jackin-auth-lock-")
        })
        .count();
    assert_eq!(lock_files, 1, "dot aliases must share one target lock");
}

#[cfg(unix)]
#[test]
fn kimi_and_hermes_reject_source_symlink_roots_and_fifo_files() {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let real_kimi = temp.path().join("real-kimi");
    let kimi_link = temp.path().join("kimi-link");
    std::fs::create_dir_all(real_kimi.join("credentials")).unwrap();
    std::fs::write(real_kimi.join("config.toml"), "x = 1\n").unwrap();
    symlink(&real_kimi, &kimi_link).unwrap();
    let kimi_target = temp.path().join("role/kimi");
    let error =
        provision_kimi_auth_from_source_dir(&kimi_target, AuthForwardMode::Sync, &kimi_link)
            .unwrap_err();
    assert!(error.to_string().contains("source auth directory"));
    assert!(!kimi_target.exists());

    let real_hermes = temp.path().join("real-hermes");
    let hermes_link = temp.path().join("hermes-link");
    std::fs::create_dir_all(&real_hermes).unwrap();
    symlink(&real_hermes, &hermes_link).unwrap();
    let hermes_target = temp.path().join("role/hermes");
    let error = provision_hermes_auth_from_source_dir(
        &hermes_target,
        AuthForwardMode::Sync,
        &hermes_link,
        Some(AiProvider::OpenAi),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("source auth directory"));
    assert!(!hermes_target.exists());

    let kimi_fifo_source = temp.path().join("kimi-fifo");
    std::fs::create_dir_all(kimi_fifo_source.join("credentials")).unwrap();
    let kimi_fifo = kimi_fifo_source.join("config.toml");
    mkfifo(&kimi_fifo, Mode::from_bits_truncate(0o600)).unwrap();
    let error = provision_kimi_auth_from_source_dir(
        &temp.path().join("role/kimi-fifo"),
        AuthForwardMode::Sync,
        &kimi_fifo_source,
    )
    .unwrap_err();
    assert!(error.to_string().contains("special file"));

    let hermes_fifo_source = temp.path().join("hermes-fifo");
    std::fs::create_dir_all(&hermes_fifo_source).unwrap();
    let hermes_fifo = hermes_fifo_source.join("config.yaml");
    mkfifo(&hermes_fifo, Mode::from_bits_truncate(0o600)).unwrap();
    let error = provision_hermes_auth_from_source_dir(
        &temp.path().join("role/hermes-fifo"),
        AuthForwardMode::Sync,
        &hermes_fifo_source,
        Some(AiProvider::OpenAi),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("special file"));
}

#[cfg(unix)]
#[test]
fn source_entry_replacement_between_lstat_and_open_is_rejected() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.kimi-code");
    let target_dir = temp.path().join("role/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    let config = source_dir.join("config.toml");
    let replacement = source_dir.join("config.toml.replacement");
    std::fs::write(&config, "version = \"old\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "token").unwrap();
    std::fs::write(&replacement, "version = \"replacement\"\n").unwrap();
    let config_for_hook = config.clone();
    set_source_open_hook(Box::new(move || {
        std::fs::rename(&replacement, &config_for_hook).unwrap();
    }));

    let error =
        provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Sync, &source_dir)
            .unwrap_err();
    assert!(
        error.to_string().contains("replaced during secure open"),
        "{error:#}"
    );
    assert!(
        !target_dir.exists(),
        "replaced source must not publish a tree"
    );
}

#[cfg(unix)]
#[test]
fn hermes_nested_source_symlink_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.hermes");
    let target_dir = temp.path().join("role/.hermes");
    let decoy = temp.path().join("decoy.yaml");
    std::fs::create_dir_all(source_dir.join("profiles")).unwrap();
    std::fs::write(
        source_dir.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"openai":{"type":"api","key":"sentinel"}}"#,
    )
    .unwrap();
    std::fs::write(&decoy, "provider: anthropic\n").unwrap();
    symlink(&decoy, source_dir.join("profiles/evil.yaml")).unwrap();

    let error = provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&ProfileSelector {
            entry: "openai".to_owned(),
            profile: Some("work".to_owned()),
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("symlink"), "{error:#}");
    assert!(!target_dir.exists());
    assert_eq!(
        std::fs::read_to_string(&decoy).unwrap(),
        "provider: anthropic\n"
    );
}

#[cfg(unix)]
#[test]
fn hermes_sync_copies_the_validated_snapshot_after_source_changes() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.hermes");
    let target_dir = temp.path().join("role/.hermes");
    std::fs::create_dir_all(source_dir.join("profiles")).unwrap();
    std::fs::write(
        source_dir.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"openai":{"type":"api","key":"snapshot-sentinel"}}"#,
    )
    .unwrap();
    std::fs::write(source_dir.join("profiles/work.yaml"), "provider: openai\n").unwrap();
    let source_for_hook = source_dir.clone();
    set_hermes_snapshot_hook(Box::new(move || {
        std::fs::write(
            source_for_hook.join("config.yaml"),
            "profiles:\n  other:\n    provider: anthropic\n",
        )
        .unwrap();
        std::fs::write(
            source_for_hook.join("auth.json"),
            r#"{"anthropic":{"type":"api","key":"mutated-sentinel"}}"#,
        )
        .unwrap();
    }));

    let selector = ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some("work".to_owned()),
    };
    let (outcome, forward_auth) = provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(
        std::fs::read_to_string(target_dir.join("auth.json"))
            .unwrap()
            .contains("snapshot-sentinel")
    );
    assert!(
        std::fs::read_to_string(target_dir.join("config.yaml"))
            .unwrap()
            .contains("work")
    );
    assert!(
        !std::fs::read_to_string(target_dir.join("auth.json"))
            .unwrap()
            .contains("mutated-sentinel")
    );
}

#[test]
fn validate_kimi_requires_config_and_credentials_tree() {
    let temp = tempdir().unwrap();
    let good = temp.path().join("kimi-good");
    std::fs::create_dir_all(good.join("credentials")).unwrap();
    std::fs::write(good.join("config.toml"), "x = 1\n").unwrap();
    validate_sync_source_dir(Agent::Kimi, &good, temp.path()).unwrap();

    // config.toml present but no credentials/ dir → rejected.
    let bad = temp.path().join("kimi-bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("config.toml"), "x = 1\n").unwrap();
    validate_sync_source_dir(Agent::Kimi, &bad, temp.path()).unwrap_err();
}

#[test]
fn ignore_mode_skips_state_when_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    // Ignore mode with no prior jackin-owned state provisions nothing: it does
    // not copy host auth and does not write an empty `{}` skeleton, so no
    // jackin-owned state is created for the container to mount. The agent falls
    // back to the image's credential-free default-home — the same no-auth
    // outcome as an explicit `{}`, without an empty bind source. Stale existing
    // jackin-owned state still enters the normal wipe path.
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(!state.claude_account_json().unwrap().exists());
    assert!(!state.claude_credentials_json().unwrap().exists());
}
