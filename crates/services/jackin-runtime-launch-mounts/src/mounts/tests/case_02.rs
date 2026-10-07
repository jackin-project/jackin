// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn authority_audits_reject_symlink_parent_traversal_before_canonicalization() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let source = root.join("provider-config/home/.codex/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"work\"\n").unwrap();
    let mut docker_state = role_state(&root, vec![]);
    docker_state.provider_config_mounts =
        vec![(source.clone(), "/home/agent/.codex/config.toml".to_owned())];
    let overlay = format!("{}:/home/agent/.codex/config.toml:ro", source.display());

    let alias = temp.path().join("authority-alias");
    symlink(root.join("provider-config"), &alias).unwrap();
    let escaped = alias.join("..").join("provider-config");
    let escaped_bind = format!("{}:/tmp/escaped:ro", escaped.display());
    assert!(
        ensure_provider_authority_not_writable(&docker_state, &[overlay, escaped_bind], &[])
            .is_err()
    );

    let apple_state = role_state(&root, vec![]);
    let mounts = vec![AppleContainerMount::new(escaped, "/tmp/escaped", true)];
    assert!(ensure_apple_provider_authority_not_exposed(&apple_state, &mounts, &[]).is_err());
}

#[cfg(unix)]
#[test]
fn authority_audits_reject_readonly_source_under_writable_alias_parent() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let writable_root = root.join("home");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&writable_root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("config.toml"), "model = \"outside\"\n").unwrap();
    let alias = writable_root.join("cache-alias");
    symlink(&outside, &alias).unwrap();
    let readonly_source = alias.join("config.toml");
    let state = role_state(&root, vec![]);

    let docker_mounts = vec![
        format!("{}:/tmp/writable", writable_root.display()),
        format!("{}:/tmp/readonly:ro", readonly_source.display()),
    ];
    assert!(ensure_provider_authority_not_writable(&state, &docker_mounts, &[]).is_err());

    let apple_mounts = vec![
        AppleContainerMount::new(writable_root.clone(), "/tmp/writable", false),
        AppleContainerMount::new(readonly_source.clone(), "/tmp/readonly", true),
    ];
    assert!(ensure_apple_provider_authority_not_exposed(&state, &apple_mounts, &[]).is_err());
}

#[cfg(unix)]
#[test]
fn authority_audits_reject_nested_rw_cache_alias() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let writable_root = root.join("home");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&writable_root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("config.toml"), "model = \"outside\"\n").unwrap();
    let alias = writable_root.join("cache-alias");
    symlink(&outside, &alias).unwrap();
    let cache_source = alias.join("config.toml");
    let state = role_state(&root, vec![]);

    let docker_mounts = vec![
        format!("{}:/tmp/home", writable_root.display()),
        format!("{}:/tmp/cache", cache_source.display()),
    ];
    assert!(ensure_provider_authority_not_writable(&state, &docker_mounts, &[]).is_err());

    let apple_mounts = vec![
        AppleContainerMount::new(writable_root, "/tmp/home", false),
        AppleContainerMount::new(cache_source, "/tmp/cache", false),
    ];
    assert!(ensure_apple_provider_authority_not_exposed(&state, &apple_mounts, &[]).is_err());
}

#[test]
fn authority_audits_allow_readonly_source_under_readonly_parent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let readonly_root = root.join("readonly-parent");
    let readonly_source = readonly_root.join("config.toml");
    std::fs::create_dir_all(&readonly_root).unwrap();
    std::fs::write(&readonly_source, "model = \"outside\"\n").unwrap();
    let state = role_state(&root, vec![]);

    let docker_mounts = vec![
        format!("{}:/tmp/readonly-parent:ro", readonly_root.display()),
        format!("{}:/tmp/readonly:ro", readonly_source.display()),
    ];
    ensure_provider_authority_not_writable(&state, &docker_mounts, &[]).unwrap();

    let apple_mounts = vec![
        AppleContainerMount::new(readonly_root.clone(), "/tmp/readonly-parent", true),
        AppleContainerMount::new(readonly_source.clone(), "/tmp/readonly", true),
    ];
    ensure_apple_provider_authority_not_exposed(&state, &apple_mounts, &[]).unwrap();
}

#[test]
fn github_config_mount_is_absent_only_when_skipped_and_missing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let state = role_state(&root, vec![]);
    assert_eq!(github_config_mount(&state).unwrap(), None);

    std::fs::create_dir_all(root.join("gh")).unwrap();
    let mounted = github_config_mount(&state)
        .unwrap()
        .expect("existing dir must mount");
    assert!(mounted.ends_with(":/home/agent/.config/gh"), "{mounted}");
}

#[test]
fn coordination_audits_reject_all_overlap_and_future_sources() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("host-home");
    let coordination = home.join(".jackin-coordination");
    let state = role_state(&temp.path().join("data/instances/current"), vec![]);
    std::fs::create_dir_all(&home).unwrap();
    // Root and descendants do not exist yet; nearest existing ancestor resolution
    // must still reject their future source paths.
    for source in [
        &home,
        &coordination,
        &coordination.join("future/generation.lock"),
    ] {
        assert_coordinator_source_rejected(&state, &coordination, source);
    }
    std::fs::create_dir_all(&coordination).unwrap();
    let lock = coordination.join("generation.lock");
    std::fs::write(&lock, "").unwrap();
    for source in [&home, &coordination, &lock] {
        assert_coordinator_source_rejected(&state, &coordination, source);
    }
    let agent_home = state.root.join("home/.codex");
    std::fs::create_dir_all(&agent_home).unwrap();
    for readonly in [false, true] {
        let docker = format!(
            "{}:/home/agent/.codex:{}",
            agent_home.display(),
            if readonly { "ro" } else { "rw" }
        );
        ensure_provider_authority_not_writable(
            &state,
            &[docker],
            std::slice::from_ref(&coordination),
        )
        .unwrap();
        let apple = AppleContainerMount::new(agent_home.clone(), "/home/agent/.codex", readonly);
        ensure_apple_provider_authority_not_exposed(
            &state,
            &[apple],
            std::slice::from_ref(&coordination),
        )
        .unwrap();
    }
}

#[cfg(unix)]
#[test]
fn coordination_audits_reject_symlink_alias_future_and_lexical_escape() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("host-home");
    let coordination = home.join(".jackin-coordination");
    std::fs::create_dir_all(&coordination).unwrap();
    let state = role_state(&temp.path().join("data/instances/current"), vec![]);
    let alias = temp.path().join("coordination-alias");
    symlink(&coordination, &alias).unwrap();
    for source in [&alias, &alias.join("future/generation.lock")] {
        assert_coordinator_source_rejected(&state, &coordination, source);
    }
    let dangling = temp.path().join("dangling-coordination-alias");
    symlink(coordination.join("future"), &dangling).unwrap();
    assert_coordinator_source_rejected(&state, &coordination, &dangling);
    assert_coordinator_source_rejected(&state, &coordination, &dangling.join("generation.lock"));
    let relative_alias = temp.path().join("relative-coordination-alias");
    symlink("host-home/.jackin-coordination/future", &relative_alias).unwrap();
    assert_coordinator_source_rejected(
        &state,
        &coordination,
        &relative_alias.join("generation.lock"),
    );
    let home_alias = temp.path().join("home-alias");
    symlink(&home, &home_alias).unwrap();
    assert_coordinator_source_rejected(&state, &coordination, &home_alias);
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("resource"), "fixture").unwrap();
    symlink(&outside, coordination.join("escape")).unwrap();
    assert_coordinator_source_rejected(
        &state,
        &coordination,
        &coordination.join("escape/resource"),
    );
    assert_coordinator_source_rejected(&state, &coordination, &alias.join("escape/resource"));
    // Protected root itself may be a symlink: protect both its lexical namespace
    // and its resolved namespace, including future descendants.
    let protected_alias = temp.path().join("protected-root-alias");
    symlink(&coordination, &protected_alias).unwrap();
    assert_coordinator_source_rejected(&state, &protected_alias, &coordination.join("future/lock"));
    assert_coordinator_source_rejected(
        &state,
        &protected_alias,
        &protected_alias.join("escape/resource"),
    );
}

#[test]
fn coordination_audit_preserves_exact_provider_overlay_without_coordination_exception() {
    let temp = tempfile::tempdir().unwrap();
    let coordination = temp.path().join("host-home/.jackin-coordination");
    let mut state = role_state(&temp.path().join("data/instances/current"), vec![]);
    let source = state.root.join("provider-config/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"fixture\"\n").unwrap();
    state.provider_config_mounts = vec![(source.clone(), "/home/agent/.codex/config.toml".into())];
    let overlay = format!("{}:/home/agent/.codex/config.toml:ro", source.display());
    ensure_provider_authority_not_writable(&state, std::slice::from_ref(&overlay), &[coordination])
        .unwrap();
    // A provider overlay exemption must never exempt an explicit protected root.
    let error = ensure_provider_authority_not_writable(&state, &[overlay], &[source])
        .expect_err("protected host roots have no overlay exemption")
        .to_string();
    assert!(error.contains("protected host root"), "{error}");
}

#[cfg(unix)]
#[test]
fn provider_audits_reject_dangling_aliases_to_future_authority() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let state = role_state(&temp.path().join("instances/current"), vec![]);
    let future_authority = state.root.join("provider-config/future");
    let alias = temp.path().join("future-provider-alias");
    symlink(&future_authority, &alias).unwrap();
    let relative = temp.path().join("relative-provider-alias");
    symlink("instances/current/provider-config/future", &relative).unwrap();
    for source in [
        &alias,
        &alias.join("config.toml"),
        &relative.join("config.toml"),
    ] {
        for readonly in [false, true] {
            let docker = format!(
                "{}:/resources:{}",
                source.display(),
                if readonly { "ro" } else { "rw" }
            );
            let error = ensure_provider_authority_not_writable(&state, &[docker], &[])
                .expect_err("future provider authority cannot escape through dangling aliases")
                .to_string();
            assert!(
                error.contains("provider configuration authority"),
                "{error}"
            );
            let apple = AppleContainerMount::new(source.to_owned(), "/resources", readonly);
            let error = ensure_apple_provider_authority_not_exposed(&state, &[apple], &[])
                .expect_err("future provider authority cannot escape through dangling aliases")
                .to_string();
            assert!(
                error.contains("provider configuration authority"),
                "{error}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn authority_path_resolution_fails_closed_on_symlink_loop() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let state = role_state(&temp.path().join("instances/current"), vec![]);
    let alias = temp.path().join("loop");
    symlink("loop", &alias).unwrap();
    assert!(
        canonical_mount_path(&alias)
            .unwrap_err()
            .to_string()
            .contains("symlink hop limit")
    );
    for readonly in [false, true] {
        let docker = format!(
            "{}:/resources:{}",
            alias.display(),
            if readonly { "ro" } else { "rw" }
        );
        assert!(ensure_provider_authority_not_writable(&state, &[docker], &[]).is_err());
        let apple = AppleContainerMount::new(alias.clone(), "/resources", readonly);
        assert!(ensure_apple_provider_authority_not_exposed(&state, &[apple], &[]).is_err());
    }
}

#[cfg(unix)]
#[test]
fn authority_path_resolution_resolves_relative_parent_after_symlink() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let namespace = temp.path().join("namespace");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&namespace).unwrap();
    std::fs::create_dir_all(outside.join("child")).unwrap();
    symlink(outside.join("child"), namespace.join("jump")).unwrap();
    symlink("jump/../future", namespace.join("alias")).unwrap();
    assert_eq!(
        canonical_mount_path(&namespace.join("alias/lock")).unwrap(),
        std::fs::canonicalize(&outside).unwrap().join("future/lock")
    );
}
