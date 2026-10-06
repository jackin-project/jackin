// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn build_output_streams_for_compact_non_debug_runs() {
    let _guard = rich_surface_test_guard();
    assert!(should_stream_build_output(false));
}

#[test]
fn build_output_is_suppressed_for_debug_or_rich_surface() {
    let _guard = rich_surface_test_guard();
    assert!(!should_stream_build_output(true));

    jackin_diagnostics::set_rich_surface_active(true);
    assert!(!should_stream_build_output(false));
    jackin_diagnostics::set_rich_surface_active(false);

    jackin_diagnostics::set_host_screen_owned(true);
    assert!(!should_stream_build_output(false));
}

#[test]
fn docker_build_env_always_enables_buildkit_with_plain_progress() {
    // BuildKit must be forced on for every build: the generated Dockerfiles
    // use `COPY --link --chmod=`, which the legacy builder rejects.
    assert_eq!(
        docker_build_env(),
        vec![
            ("DOCKER_BUILDKIT".to_owned(), "1".to_owned()),
            ("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned()),
        ]
    );
}

#[test]
fn docker_info_store_parser_detects_containerd_snapshotter() {
    assert_eq!(
        docker_info_uses_containerd_store(
            "overlayfs\n[[\"driver-type\",\"io.containerd.snapshotter.v1\"]]"
        ),
        Some(true)
    );
    assert_eq!(
        docker_info_uses_containerd_store("overlay2\n[]"),
        Some(false)
    );
    assert_eq!(docker_info_uses_containerd_store(""), None);
}

#[test]
fn image_build_sources_have_no_ambient_github_secret_contract() {
    for forbidden in [
        "resolve_github_token",
        "NamedTempFile",
        "--secret",
        "id=github_token",
        "gh auth token",
    ] {
        assert!(
            !IMAGE_BUILD_SOURCE.contains(forbidden),
            "runtime image builder retained forbidden ambient-secret plumbing: {forbidden}"
        );
    }
    assert!(!IMAGE_VERSION_SOURCE.contains("resolve_github_token"));
    assert!(!IMAGE_VERSION_SOURCE.contains("capture_secret"));
    assert!(!IMAGE_VERSION_SOURCE.contains("GITHUB_TOKEN"));
    assert!(!IMAGE_VERSION_SOURCE.contains("GH_TOKEN"));
    for source in [IMAGE_MODULE_SOURCE, SHARED_IMAGE_BUILD_SOURCE] {
        assert!(!source.contains("dockerfile_requests_github_token_secret"));
        assert!(!source.contains("dockerfile_body_requests_github_token_secret"));
        assert!(!source.contains("id=github_token"));
    }
}

#[cfg(unix)]
#[tokio::test]
#[expect(
    clippy::disallowed_methods,
    reason = "test re-execs itself in an isolated child for env hermeticity"
)]
async fn ambient_github_credentials_are_not_forwarded_to_buildkit() -> anyhow::Result<()> {
    const CHILD_TEST: &str =
        "runtime::image::tests::case_01::ambient_github_credentials_are_not_forwarded_to_buildkit";

    if let Some(case) = std::env::var_os(BUILD_TOKEN_TEST_CHILD) {
        let marker = std::env::var_os(BUILD_TOKEN_TEST_MARKER)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("isolated test omitted the fake gh marker path"))?;
        match case.to_str() {
            Some("github-token") => anyhow::ensure!(
                std::env::var("GITHUB_TOKEN").ok().as_deref()
                    == Some(BUILD_TOKEN_TEST_GITHUB_TOKEN)
                    && std::env::var_os("GH_TOKEN").is_none(),
                "GITHUB_TOKEN child did not receive its fake-only credential"
            ),
            Some("gh-token") => anyhow::ensure!(
                std::env::var("GH_TOKEN").ok().as_deref() == Some(BUILD_TOKEN_TEST_GH_TOKEN)
                    && std::env::var_os("GITHUB_TOKEN").is_none(),
                "GH_TOKEN child did not receive its fake-only credential"
            ),
            Some("gh-cli") => anyhow::ensure!(
                std::env::var_os("GITHUB_TOKEN").is_none()
                    && std::env::var_os("GH_TOKEN").is_none(),
                "gh CLI child inherited a process token"
            ),
            _ => anyhow::bail!("unknown isolated test case"),
        }

        let mut runner = BuildSecurityRunner {
            execute_fake_gh: true,
            ..BuildSecurityRunner::default()
        };
        build_test_agent_image(&mut runner).await?;

        let docker_builds: Vec<&str> = runner
            .commands
            .iter()
            .filter(|command| command.starts_with("docker build "))
            .map(String::as_str)
            .collect();
        anyhow::ensure!(
            docker_builds.len() == 2,
            "expected role-base and derived BuildKit invocations, got {docker_builds:?}"
        );
        for command in docker_builds {
            anyhow::ensure!(
                !command.contains("--secret")
                    && !command.contains("id=github_token")
                    && !command.contains("src="),
                "ambient credentials created a BuildKit secret argument: {command}"
            );
            for canary in [
                BUILD_TOKEN_TEST_GITHUB_TOKEN,
                BUILD_TOKEN_TEST_GH_TOKEN,
                BUILD_TOKEN_TEST_GH_CLI,
            ] {
                anyhow::ensure!(
                    !command.contains(canary),
                    "credential canary reached Docker argv: {command}"
                );
            }
        }
        anyhow::ensure!(
            runner.secret_commands.is_empty(),
            "image builds must not ask the credential resolver: {:?}",
            runner.secret_commands
        );
        anyhow::ensure!(
            !marker.exists(),
            "fake gh auth token executable was invoked"
        );
        anyhow::ensure!(
            runner
                .docker_build_options
                .iter()
                .all(|options| options.extra_env == docker_build_env()),
            "BuildKit environment defaults changed"
        );
        return Ok(());
    }

    let temp = tempfile::tempdir()?;
    let fake_bin = temp.path().join("bin");
    std::fs::create_dir_all(&fake_bin)?;
    let fake_gh = fake_bin.join("gh");
    std::fs::write(
        &fake_gh,
        format!(
            "#!/bin/sh\nprintf '%s\\n' invoked > \"${BUILD_TOKEN_TEST_MARKER}\"\nprintf '%s\\n' '{BUILD_TOKEN_TEST_GH_CLI}'\n"
        ),
    )?;
    let mut permissions = std::fs::metadata(&fake_gh)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_gh, permissions)?;
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let child_path = std::env::join_paths(
        std::iter::once(fake_bin).chain(std::env::split_paths(&original_path)),
    )?;

    for (case, token) in [
        (
            "github-token",
            Some(("GITHUB_TOKEN", BUILD_TOKEN_TEST_GITHUB_TOKEN)),
        ),
        ("gh-token", Some(("GH_TOKEN", BUILD_TOKEN_TEST_GH_TOKEN))),
        ("gh-cli", None),
    ] {
        let marker = temp.path().join(format!("{case}.marker"));
        let mut command = ProcessCommand::new(std::env::current_exe()?);
        command
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env(BUILD_TOKEN_TEST_CHILD, case)
            .env(BUILD_TOKEN_TEST_MARKER, &marker)
            .env("PATH", &child_path)
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_TOKEN");
        if let Some((name, value)) = token {
            command.env(name, value);
        }
        let output = command.output()?;
        anyhow::ensure!(
            output.status.success(),
            "isolated {case} build-token test failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        anyhow::ensure!(
            !marker.exists(),
            "fake gh auth token executable ran in the {case} case"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn buildkit_errors_propagate_from_role_base_and_derived_builds() -> anyhow::Result<()> {
    for failed_build in [1, 2] {
        let mut runner = BuildSecurityRunner {
            fail_docker_build_at: Some(failed_build),
            ..BuildSecurityRunner::default()
        };
        let error = build_test_agent_image(&mut runner)
            .await
            .expect_err("Docker BuildKit failure must propagate to the image caller");
        anyhow::ensure!(
            error
                .to_string()
                .contains("simulated Docker BuildKit failure"),
            "unexpected image build error: {error:#}"
        );
        anyhow::ensure!(
            runner.docker_build_count == failed_build,
            "expected build failure at invocation {failed_build}, got {} builds",
            runner.docker_build_count
        );
    }
    Ok(())
}

#[test]
fn dockerfile_role_sha_detection_only_requests_declared_arg() {
    assert!(!dockerfile_body_requests_role_git_sha_arg(
        "FROM projectjackin/construct:0.1-trixie\nRUN echo $ROLE_GIT_SHA\n"
    ));
    assert!(!dockerfile_body_requests_role_git_sha_arg(
        "FROM projectjackin/construct:0.1-trixie\n# ARG ROLE_GIT_SHA\n"
    ));
    assert!(dockerfile_body_requests_role_git_sha_arg(
        "FROM projectjackin/construct:0.1-trixie\nARG ROLE_GIT_SHA=unknown\nRUN echo $ROLE_GIT_SHA\n"
    ));
    assert!(dockerfile_body_requests_role_git_sha_arg(
        "FROM projectjackin/construct:0.1-trixie\nARG\tROLE_GIT_SHA\n"
    ));
}

#[tokio::test]
async fn record_built_agent_version_skips_docker_probe_for_prefetched_version() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let runtime_binaries = PreparedRuntimeBinaries {
        agent_installs: BTreeMap::from([(
            Agent::Claude,
            AgentInstall::Prefetched(paths.cache_dir.join("claude")),
        )]),
        prefetched_agent_versions: BTreeMap::from([(Agent::Claude, "2.1.91".to_owned())]),
        jackin_capsule_src: "/tmp/jackin-capsule".to_owned(),
    };
    let mut runner = FakeRunner {
        fail_on: vec!["docker run --rm --entrypoint".to_owned()],
        ..Default::default()
    };

    record_built_agent_version(
        &paths,
        "jk_agent-smith",
        Agent::Claude,
        &runtime_binaries,
        false,
        &mut runner,
    )
    .await;

    assert!(
        !runner
            .recorded
            .join("\n")
            .contains("docker run --rm --entrypoint"),
        "prefetched metadata must skip foreground version probe"
    );
    assert_eq!(
        version_check::stored_version(&paths, Agent::Claude, "jk_agent-smith"),
        Some("2.1.91".to_owned())
    );
}

#[tokio::test]
async fn record_built_agent_version_probes_when_prefetched_version_missing() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let runtime_binaries = PreparedRuntimeBinaries {
        agent_installs: BTreeMap::from([(
            Agent::Claude,
            AgentInstall::Prefetched(paths.cache_dir.join("claude")),
        )]),
        prefetched_agent_versions: BTreeMap::new(),
        jackin_capsule_src: "/tmp/jackin-capsule".to_owned(),
    };
    let mut runner = FakeRunner::with_capture_queue(["2.1.91 (Claude Code)".to_owned()]);

    record_built_agent_version(
        &paths,
        "jk_agent-smith",
        Agent::Claude,
        &runtime_binaries,
        false,
        &mut runner,
    )
    .await;

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker run --rm --entrypoint claude jk_agent-smith --version"),
        "missing metadata must keep the Docker version probe; recorded:\n{recorded}"
    );
    assert_eq!(
        version_check::stored_version(&paths, Agent::Claude, "jk_agent-smith"),
        Some("2.1.91".to_owned())
    );
}

#[test]
fn parse_docker_build_steps_extracts_completed_buildkit_lines() {
    let steps = parse_docker_build_steps(
        r#"
run: jk-run-test
command: docker build .

----- stdout -----
#0 building with "orbstack" instance using docker driver
#1 [internal] load build definition from DerivedDockerfile
#1 transferring dockerfile: 6.15kB done
#1 DONE 0.3s
#2 [internal] load metadata for docker.io/projectjackin/jackin-the-architect:latest
#2 CACHED
#7 [ 2/46] RUN current_gid="$(id -g agent)"
#7 0.433 usermod: no changes
#7 DONE 8.5s
#12 exporting to image
#12 exporting layers 76.456s done
#12 DONE 76.5s
----- stderr -----
"#,
    );

    assert_eq!(
        steps,
        vec![
            DockerBuildStep {
                step: "1".to_owned(),
                label: "[internal] load build definition from DerivedDockerfile".to_owned(),
                duration_ms: Some(300),
                cached: false,
            },
            DockerBuildStep {
                step: "2".to_owned(),
                label: "[internal] load metadata for docker.io/projectjackin/jackin-the-architect:latest".to_owned(),
                duration_ms: None,
                cached: true,
            },
            DockerBuildStep {
                step: "7".to_owned(),
                label: "[ 2/46] RUN current_gid=\"$(id -g agent)\"".to_owned(),
                duration_ms: Some(8500),
                cached: false,
            },
            DockerBuildStep {
                step: "12".to_owned(),
                label: "exporting to image".to_owned(),
                duration_ms: Some(76500),
                cached: false,
            },
        ]
    );
}
