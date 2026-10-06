// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_launch_sibling_auth_prewarm_finishes_before_mount_admission() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let opencode_source = temp.path().join("opencode-profile");
    std::fs::create_dir_all(&opencode_source).unwrap();
    std::fs::write(
        opencode_source.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"opencode-test"}}"#,
    )
    .unwrap();
    let mut config = AppConfig {
        default_launch: Some(vec!["codex-main".into(), "opencode-main".into()]),
        ..AppConfig::default()
    };
    config.accounts.insert(
        "codex".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Codex".into(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: "codex-test".into(),
                base_url: None,
                model: None,
            },
        },
    );
    config.accounts.insert(
        "opencode".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "OpenCode".into(),
            provider: jackin_config::AiProvider::Opencode,
            credential: jackin_config::AccountCredential::Profile {
                agent: jackin_core::Agent::Opencode,
                directory: opencode_source,
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    config.agent_configurations.insert(
        "codex-main".into(),
        jackin_config::AgentConfiguration {
            agent: jackin_core::Agent::Codex,
            account: "codex".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    config.agent_configurations.insert(
        "opencode-main".into(),
        jackin_config::AgentConfiguration {
            agent: jackin_core::Agent::Opencode,
            account: "opencode".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    let manifest_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        manifest_dir.path().join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\nagents = [\"codex\", \"opencode\"]\n\n[codex]\n\n[opencode]\n",
    )
    .unwrap();
    std::fs::write(
        manifest_dir.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_dir.path()).unwrap();
    let prewarm = SiblingAuthPrewarm {
        manifest: &manifest,
        config: &config,
        workspace_name: "",
        role_key: "test-role",
    };
    let worker = spawn_sibling_auth_prewarm(
        &paths,
        "jk-auth-default-launch",
        &prewarm,
        jackin_core::Agent::Codex,
    )
    .expect("default_launch must produce an OpenCode sibling prewarm");
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        await_sibling_auth_prewarm(Some(worker)),
    )
    .await
    .expect("sibling prewarm must not deadlock before admission")
    .unwrap();

    let instances = jackin_config::resolve_launch(&config, None, "test-role", None, None).unwrap();
    let bindings =
        crate::runtime::launch::capsule_setup::instance_auth_bindings(&config, &instances).unwrap();
    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-auth-default-launch",
        &manifest,
        &bindings,
        &crate::instance::GithubAuthContext::default(),
        &paths.home_dir,
        jackin_core::Agent::Codex,
    )
    .unwrap();

    assert!(
        state.auth.slots["opencode-main"]
            .credential_paths
            .iter()
            .all(|path| path.exists())
    );
    assert!(!state.auth_mount_leases.is_empty());
}

#[test]
fn docker_final_complete_spec_blocks_coordination_before_submission() {
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
                let mut binds = vec![format!("{}:/home/agent/.codex", normal.display())];
                if let Some(source) = exposure {
                    binds.push(format!(
                        "{}:{target}:{}",
                        source.display(),
                        if readonly { "ro" } else { "rw" }
                    ));
                }
                let spec = jackin_core::ContainerSpec {
                    image: "fixture".into(),
                    binds,
                    ..Default::default()
                };
                let submitted = std::cell::Cell::new(0usize);
                let result = with_admitted_final_docker_spec(&paths, &state, spec, |_spec| {
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
fn docker_final_complete_spec_rejects_coordination_aliases_before_submission() {
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
            let binds = vec![
                format!("{}:/home/agent/.codex", normal.display()),
                format!(
                    "{}:/archive:{}",
                    source.display(),
                    if readonly { "ro" } else { "rw" }
                ),
            ];
            let spec = jackin_core::ContainerSpec {
                image: "fixture".into(),
                binds,
                ..Default::default()
            };
            let submissions = std::cell::Cell::new(0usize);
            let result = with_admitted_final_docker_spec(&paths, &state, spec, |_spec| {
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
