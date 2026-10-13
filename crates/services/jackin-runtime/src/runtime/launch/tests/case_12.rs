// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_builds_local_role_base_then_derives_overlay_from_it() {
    // The workspace build is two-stage: first a role *base* image
    // (jk_<role>__base, the role Dockerfile, no overlay), then the derived image
    // (FROM that base + jackin overlay). The base carries the role-sha + construct
    // labels so it can be reused across overlay rebuilds.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    // Stage 1: the role base build.
    let base_build = runner
        .recorded
        .iter()
        .find(|c| c.contains("docker build ") && c.contains("BaseDockerfile"))
        .expect("workspace build must first build the role base image");
    assert!(
        base_build.contains("--output type=docker,name=jk_agent-smith__base")
            && base_build.contains("compression=uncompressed"),
        "base build must load uncompressed jk_<role>__base; got: {base_build}"
    );
    assert!(
        base_build.contains("docker build ")
            && !base_build.contains("--builder")
            && !base_build.contains("--context"),
        "base build consumes local images and must use plain docker build (ambient endpoint, Docker driver); got: {base_build}"
    );
    assert!(
        base_build.contains("--label jackin.construct.image=")
            && base_build.contains("--label jackin.role.git.sha="),
        "base build must stamp construct + role-sha labels for reuse; got: {base_build}"
    );
    assert!(
        !base_build.contains("DerivedDockerfile"),
        "base build must not include the jackin overlay; got: {base_build}"
    );

    // Stage 2: the derived overlay build, FROM the local base (not the construct).
    let derived_build = runner
        .recorded
        .iter()
        .find(|c| c.contains("docker build ") && c.contains("DerivedDockerfile"))
        .expect("workspace build must derive the overlay after the base");
    assert!(
        !derived_build.contains("--pull"),
        "derived build is FROM a local base and must never --pull; got: {derived_build}"
    );
    assert!(
        derived_build.contains("docker build ")
            && !derived_build.contains("--builder")
            && !derived_build.contains("--context"),
        "derived build consumes the local role base and must use plain docker build (ambient endpoint, Docker driver); got: {derived_build}"
    );
    assert!(
        derived_build.contains("--label jackin.image.recipe.hash="),
        "derived build stamps the recipe labels; got: {derived_build}"
    );
}

#[tokio::test]
async fn load_agent_omits_pull_flag_in_normal_workspace_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build ") && call.contains("DerivedDockerfile"))
        .unwrap();
    assert!(
        !build_cmd.contains("--pull"),
        "workspace mode without --rebuild must not pass --pull"
    );
    assert!(
        build_cmd.contains("--label jackin.image.recipe.version=v9"),
        "workspace build must stamp recipe version label; got: {build_cmd}"
    );
    assert!(
        build_cmd.contains("--label jackin.image.recipe.hash="),
        "workspace build must stamp recipe hash label; got: {build_cmd}"
    );
    // Agent-independence is now captured inside the recipe hash (the
    // supported-agent set is a recipe input) rather than a standalone label.
    assert!(
        build_cmd.contains("--label jackin.manifest.version="),
        "workspace build must stamp the manifest version label; got: {build_cmd}"
    );
}

#[tokio::test]
async fn load_agent_cleans_up_sidecar_when_derived_build_fails() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    runner.fail_with.push((
        "docker build ".to_owned(),
        "derived build failed".to_owned(),
    ));

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("derived build failed"),
        "unexpected error: {error:#}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && call.ends_with("-dind")),
        "DinD cleanup missing after build failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker volume rm jk-")),
        "cert volume cleanup missing after build failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker network rm jk-")),
        "network cleanup missing after build failure: {docker_recorded:?}"
    );
}

#[tokio::test]
async fn load_agent_reuses_valid_local_image_and_skips_build_work() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    let local_base = local_role_base_for_test(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        Some(local_base.as_str()),
        "0",
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        cached_repo.repo_dir.join("Dockerfile"),
        cached_repo.repo_dir.join("context-copy-poison"),
    )
    .unwrap();
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    runner.fail_on = vec![
        "docker build ".to_owned(),
        "gh auth token".to_owned(),
        "docker run --rm --entrypoint".to_owned(),
        "agent_binary".to_owned(),
    ];

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("docker build "),
        "valid local recipe must skip docker build; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("gh auth token"),
        "valid local recipe must skip GitHub token lookup; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("docker run --rm --entrypoint"),
        "valid local recipe must skip foreground agent version probe; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("agent_binary_resolve_started"),
        "valid local recipe must skip runtime binary preparation; recorded:\n{recorded}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker inspect image:{image}")),
        "valid local image must still be inspected for recipe labels"
    );
}
