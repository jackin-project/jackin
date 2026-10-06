// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_passes_pull_flag_with_published_image() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

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
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            role_sha.to_owned(),
        )]));
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

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "pre-built image mode must pull to check for registry updates"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker tag docker.io/myorg/my-role:latest")),
        "fresh published image must be tagged as the local base"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build ") && call.contains("DerivedDockerfile")),
        "derived overlay build must still run"
    );
}

#[tokio::test]
async fn load_agent_uses_prebuilt_when_construct_version_matches() {
    // When the published image's jackin.role.git.sha label matches the role
    // checkout, the pre-built image is used.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([
            (
                crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
                role_sha.to_owned(),
            ),
            (
                crate::runtime::naming::LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(),
                "0.1-trixie".to_owned(),
            ),
        ]));
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

    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker tag docker.io/myorg/my-role:latest")),
        "pre-built mode must tag the verified image as the local base; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn load_agent_falls_back_to_workspace_when_role_sha_label_missing() {
    // When the published image cannot prove it was built for the current role
    // SHA, jackin falls back to workspace mode.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    // The published image does not carry the current role SHA, triggering
    // workspace fallback.
    let mut runner = FakeRunner::for_load_agent(["abc123".to_owned()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::new());
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
        .find(|call| call.contains("docker build "))
        .unwrap();
    // A stale published image falls back to a workspace role-base build, but it
    // is not an operator-requested rebuild: keep Docker's layer cache and do
    // not use the stale published image as base.
    assert!(
        !build_cmd.contains("--pull"),
        "published-stale fallback should preserve layer cache; got: {build_cmd}"
    );
    assert!(
        !build_cmd.contains("docker.io/myorg/my-role:latest"),
        "stale published image must not be used as base; got: {build_cmd}"
    );
}

#[tokio::test]
async fn load_agent_uses_prebuilt_when_role_sha_matches_without_construct_version() {
    // The role SHA label is authoritative for current published images. A
    // matching SHA is enough even when construct-version is absent.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            role_sha.to_owned(),
        )]));
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

    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker tag docker.io/myorg/my-role:latest")),
        "prebuilt mode must tag the verified image as the local base"
    );
    // In prebuilt mode rebuild=false, so the construct-mismatch guard calls
    // inspect_image_labels on the derived image (bollard). Workspace-rebuild mode skips it.
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker inspect image:jk_agent-smith")),
        "prebuilt mode must run docker inspect_image_label on derived image (construct-mismatch check)"
    );
}

#[tokio::test]
async fn load_agent_ignores_published_image_when_rebuild() {
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
published_image = "docker.io/myorg/my-role:latest"

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
        &LoadOptions {
            rebuild: true,
            ..LoadOptions::default()
        },
    )
    .await
    .unwrap();

    // With --rebuild the workspace Dockerfile is used even when published_image is set.
    // The DerivedDockerfile must contain the workspace FROM, not the published image.
    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("docker.io/myorg/my-role:latest"),
        "--rebuild must bypass published_image and build from the workspace Dockerfile"
    );
}
