// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn final_hook_state(paths: &JackinPaths) -> crate::instance::RoleState {
    let root = paths.data_dir.join("instances/current");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    crate::instance::RoleState {
        gh_config_dir: root.join("gh"),
        root,
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Codex,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth::default(),
        auth_outcomes: std::collections::BTreeMap::default(),
        auth_mount_paths: std::collections::BTreeSet::default(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    }
}

#[test]
fn apple_final_complete_spec_blocks_coordination_before_submission() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let state = final_hook_state(&paths);
    let normal = state.root.join("home/.codex");
    let coordination = paths.home_dir.join(".jackin-coordination");
    std::fs::create_dir_all(&coordination).unwrap();
    let lock = coordination.join("generation.lock");
    std::fs::write(&lock, "").unwrap();
    let exposures = [
        coordination.clone(),
        lock,
        coordination.join("future/lock"),
        paths.home_dir.clone(),
    ];
    for target in ["/home/agent", "/resources", "/archive"] {
        for readonly in [false, true] {
            for exposure in exposures.iter().map(Some).chain(std::iter::once(None)) {
                let mut mounts = vec![AppleContainerMount::new(
                    normal.clone(),
                    "/home/agent/.codex",
                    false,
                )];
                if let Some(source) = exposure {
                    mounts.push(AppleContainerMount::new(source.clone(), target, readonly));
                }
                let spec = crate::apple_container_client::AppleContainerSpec {
                    image: "fixture".into(),
                    user: "0:0".into(),
                    env: Vec::new(),
                    env_file: None,
                    mounts,
                    caps_add: Vec::new(),
                };
                let submitted = std::cell::Cell::new(0usize);
                let result = with_admitted_final_apple_spec(&paths, &state, spec, |_spec| {
                    submitted.set(submitted.get() + 1);
                });
                if exposure.is_some() {
                    let error = result
                        .expect_err("complete final spec must reject before container submission");
                    assert!(error.to_string().contains("protected host root"), "{error}");
                    assert_eq!(
                        submitted.get(),
                        0,
                        "container submission must remain unreachable"
                    );
                } else {
                    result.unwrap();
                    assert_eq!(
                        submitted.get(),
                        1,
                        "ordinary agent home must reach container submission"
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn apple_final_complete_spec_rejects_coordination_aliases_before_submission() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let state = final_hook_state(&paths);
    let normal = state.root.join("home/.codex");
    let coordination = paths.home_dir.join(".jackin-coordination");
    std::fs::create_dir_all(&coordination).unwrap();
    let lock = coordination.join("generation.lock");
    std::fs::write(&lock, "").unwrap();
    let direct = temp.path().join("existing-root-alias");
    symlink(&coordination, &direct).unwrap();
    let lock_alias = temp.path().join("existing-lock-alias");
    symlink(&lock, &lock_alias).unwrap();
    let dangling = temp.path().join("future-root-alias");
    symlink(coordination.join("future"), &dangling).unwrap();
    let relative = temp.path().join("relative-future-alias");
    symlink("home/.jackin-coordination/future", &relative).unwrap();
    let sibling_normal = paths.home_dir.join(".jackin-coordination-sibling-normal");
    std::fs::create_dir_all(&sibling_normal).unwrap();
    let sources = [
        direct.clone(),
        direct.join("generation.lock"),
        lock_alias,
        direct.join("future/lock"),
        dangling.clone(),
        dangling.join("lock"),
        relative.join("lock"),
    ];
    for readonly in [false, true] {
        for (source, forbidden) in sources
            .iter()
            .map(|source| (source, true))
            .chain([(&normal, false), (&sibling_normal, false)])
        {
            let mounts = vec![
                AppleContainerMount::new(normal.clone(), "/home/agent/.codex", false),
                AppleContainerMount::new(source.clone(), "/archive", readonly),
            ];
            let spec = crate::apple_container_client::AppleContainerSpec {
                image: "fixture".into(),
                user: "0:0".into(),
                env: Vec::new(),
                env_file: None,
                mounts,
                caps_add: Vec::new(),
            };
            let submissions = std::cell::Cell::new(0usize);
            let result = with_admitted_final_apple_spec(&paths, &state, spec, |_spec| {
                submissions.set(submissions.get() + 1);
            });
            if forbidden {
                let error = result.expect_err(
                    "final complete spec must reject coordination aliases before submission",
                );
                assert!(error.to_string().contains("protected host root"), "{error}");
                assert_eq!(submissions.get(), 0);
            } else {
                result.unwrap();
                assert_eq!(
                    submissions.get(),
                    1,
                    "ordinary per-agent home control must submit"
                );
            }
        }
    }
}
