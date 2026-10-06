// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_uses_resolved_workspace_mounts_and_workdir() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
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

    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).unwrap();
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: workspace_dir.display().to_string(),
        workdir: workspace_dir.display().to_string(),
        mounts: vec![jackin_config::MountConfig {
            src: workspace_dir.display().to_string(),
            dst: workspace_dir.display().to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let run_call = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(run_call.contains(&format!("--workdir {}", workspace.workdir)));
    assert!(run_call.contains(&format!(
        "{}:{}",
        workspace_dir.display(),
        workspace_dir.display()
    )));
    assert!(!run_call.contains(&format!("{}:/workspace", repo_dir.display())));
}

#[tokio::test]
async fn load_agent_bakes_host_uid_not_gid_into_docker_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);

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

    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).unwrap();
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: workspace_dir.display().to_string(),
        workdir: workspace_dir.display().to_string(),
        mounts: vec![jackin_config::MountConfig {
            src: workspace_dir.display().to_string(),
            dst: workspace_dir.display().to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let build_call = runner
        .recorded
        .iter()
        .find(|call| {
            call.contains("docker build ")
                && call.contains("DerivedDockerfile")
                && call.contains("--output type=docker,name=jk_agent-smith")
        })
        .unwrap();
    assert!(build_call.contains("--build-arg JACKIN_RUN_UID="));
    assert!(!build_call.contains("--build-arg JACKIN_HOST_UID="));
    assert!(!build_call.contains("--build-arg JACKIN_HOST_GID="));
    assert!(!build_call.contains("--build-arg ROLE_GIT_SHA="));
    // The host-identity strategy is now folded into the master recipe hash
    // (no standalone label); its presence proves the recipe was stamped.
    assert!(build_call.contains("--label jackin.image.recipe.hash="));
    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("gh auth token"),
        "image builds must not resolve host GitHub credentials; recorded:\n{recorded}"
    );
    assert!(
        !build_call.contains("--secret") && !build_call.contains("id=github_token"),
        "the default image build must not forward a host credential to BuildKit; got:\n{build_call}"
    );
    assert!(!recorded.contains("id -u"));
    assert!(!recorded.contains("id -g"));

    let build_run_index = runner
        .run_recorded
        .iter()
        .position(|call| call.contains("docker build ") && call.contains("DerivedDockerfile"))
        .unwrap();
    let build_opts = &runner.run_options[build_run_index];
    assert!(build_opts.capture_stdout);
    assert!(build_opts.capture_stderr);
    assert!(build_opts.null_stdin);
    assert!(build_opts.tee_to_build_log);
    assert!(
        build_opts
            .extra_env
            .contains(&("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned()))
    );
    assert!(
        build_opts
            .extra_env
            .contains(&("DOCKER_BUILDKIT".to_owned(), "1".to_owned())),
        "Docker builds must use BuildKit even when no GitHub token secret is requested"
    );

    let run_call = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_call.contains("--user 0:0"),
        "role docker run must start the root capsule supervisor: {run_call}"
    );
    assert!(
        !run_call.contains("--group-add 0"),
        "role docker run must not make every session a shared group-0 process: {run_call}"
    );
    assert!(
        run_call.contains("/var/lib/extrausers/passwd:ro"),
        "role docker run must mount runtime passwd entry: {run_call}"
    );
    assert!(
        run_call.contains("/var/lib/extrausers/group:ro"),
        "role docker run must mount runtime group entry: {run_call}"
    );

    let passwd = std::fs::read_to_string(paths.jackin_home.join("extrausers/passwd")).unwrap();
    let group = std::fs::read_to_string(paths.jackin_home.join("extrausers/group")).unwrap();
    assert!(passwd.contains("jackin-slot-0:x:2000:2000:"), "{passwd}");
    assert!(passwd.contains("jackin-shell:x:2001:2001:"), "{passwd}");
    assert!(group.contains("jackin-slot-0:x:2000:"), "{group}");
    assert!(group.contains("jackin-shell:x:2001:"), "{group}");
}

#[tokio::test]
async fn load_agent_tags_fresh_published_image_as_local_base() {
    // A fresh published image is pulled, verified by Docker image labels, and
    // tagged into the local jk_<role>__base name. The overlay derives FROM that
    // local base without running a restamp Docker build.
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
    docker.inspect_image_labels_queue.borrow_mut().push_back(
        [
            (
                crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
                role_sha.to_owned(),
            ),
            (
                crate::runtime::naming::LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(),
                "0.1-trixie".to_owned(),
            ),
        ]
        .into(),
    );
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

    // The base is a local tag of the already verified published image.
    let base_tag = runner
        .recorded
        .iter()
        .find(|c| c.contains("docker tag docker.io/myorg/my-role:latest jk_agent-smith__base"))
        .expect("fresh published image must be tagged into a local base");
    assert!(
        base_tag.ends_with(&format!(":{role_sha}")),
        "base tag must use the role SHA; got: {base_tag}"
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("docker build ") && c.contains("BaseDockerfile")),
        "fresh published images must not be restamped through a Docker build"
    );
    // The overlay derives FROM that local base, not the published image.
    assert!(
        runner
            .recorded
            .iter()
            .any(|c| c.contains("docker build ") && c.contains("DerivedDockerfile")),
        "overlay must derive FROM the local base"
    );
}
