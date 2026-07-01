//! Tests for `image`.
use super::*;
use crate::runtime::test_support::{FakeDockerClient, FakeRunner, TEST_DOCKERFILE_FROM};
use jackin_core::agent::Agent;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard};

static RICH_SURFACE_TEST_LOCK: Mutex<()> = Mutex::new(());

struct RichSurfaceTestGuard {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for RichSurfaceTestGuard {
    fn drop(&mut self) {
        jackin_diagnostics::set_rich_surface_active(false);
        jackin_diagnostics::set_host_screen_owned(false);
    }
}

fn rich_surface_test_guard() -> RichSurfaceTestGuard {
    let guard = RICH_SURFACE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    jackin_diagnostics::set_rich_surface_active(false);
    jackin_diagnostics::set_host_screen_owned(false);
    RichSurfaceTestGuard { _guard: guard }
}

fn make_docker(labels: HashMap<String, String>) -> FakeDockerClient {
    let docker = FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker
}

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
            ("BUILDX_NO_DEFAULT_ATTESTATIONS".to_owned(), "1".to_owned(),),
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

#[tokio::test]
async fn non_containerd_image_store_note_emits_diagnostic() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let mut runner = FakeRunner::with_capture_queue(["overlay2\n[]".to_owned()]);

    emit_non_containerd_image_store_note(&mut runner).await;

    assert_eq!(
        runner.recorded,
        vec!["docker info --format {{.Driver}}\n{{json .DriverStatus}}".to_owned()]
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"docker_image_store\"")
            && diagnostics.contains("Docker daemon is not using the containerd image store")
            && diagnostics.contains("\\\"containerd_image_store\\\":false")
            && diagnostics.contains("overlay2"),
        "non-containerd image store note missing: {diagnostics}"
    );
}

#[tokio::test]
async fn containerd_image_store_note_is_suppressed() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let mut runner = FakeRunner::with_capture_queue([concat!(
        "overlayfs\n",
        "[[\"driver-type\",\"io.containerd.snapshotter.v1\"]]"
    )
    .to_owned()]);

    emit_non_containerd_image_store_note(&mut runner).await;

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        !diagnostics.contains("docker_image_store"),
        "containerd-backed daemon should not emit slow-store note: {diagnostics}"
    );
}

#[tokio::test]
async fn role_git_sha_for_recipe_uses_known_sha_without_git_capture() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, _) = validated_test_repo(&paths, &selector);
    let mut runner = FakeRunner {
        fail_on: vec!["git -C".to_owned()],
        ..Default::default()
    };

    let sha = role_git_sha_for_recipe(&cached_repo, Some("abc123"), &mut runner).await;

    assert_eq!(sha.as_deref(), Some("abc123"));
    assert!(
        runner.recorded.is_empty(),
        "known role SHA should avoid git rev-parse capture: {:?}",
        runner.recorded
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("role_git_sha") && diagnostics.contains("known"),
        "known role SHA skip should be visible in diagnostics: {diagnostics}"
    );
}

#[test]
fn build_context_snapshot_records_file_count_and_bytes() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let context = temp.path().join("context");
    std::fs::create_dir_all(context.join("nested")).unwrap();
    std::fs::write(context.join("Dockerfile"), "FROM scratch\n").unwrap();
    std::fs::write(context.join("nested/file.txt"), "abc").unwrap();

    assert_eq!(
        build_context_stats(&context).unwrap(),
        BuildContextStats {
            files: 2,
            bytes: 16
        }
    );
    emit_build_context_snapshot(&context, "published");

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"build_context_snapshot\"")
            && diagnostics.contains("derived published build context snapshot")
            && diagnostics.contains("\\\"source\\\":\\\"published\\\"")
            && diagnostics.contains("\\\"files\\\":2")
            && diagnostics.contains("\\\"bytes\\\":16"),
        "build context telemetry missing: {diagnostics}"
    );
}

#[test]
fn image_build_source_diagnostic_reports_published_base() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    emit_image_build_source(
        Some("registry.example/role:latest"),
        "published_image_fresh",
        true,
    );

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_build_source\"")
            && diagnostics.contains("\\\"source\\\":\\\"published_image\\\"")
            && diagnostics.contains("\\\"reason\\\":\\\"published_image_fresh\\\"")
            && diagnostics.contains("\\\"pull_base_image\\\":true")
            && diagnostics.contains("\\\"base_image\\\":\\\"registry.example/role:latest\\\""),
        "published-image build source diagnostic missing: {diagnostics}"
    );
}

#[test]
fn image_build_source_diagnostic_reports_workspace_reason() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    emit_image_build_source(None, "custom_construct", false);

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_build_source\"")
            && diagnostics.contains("\\\"source\\\":\\\"workspace_dockerfile\\\"")
            && diagnostics.contains("\\\"reason\\\":\\\"custom_construct\\\"")
            && diagnostics.contains("\\\"pull_base_image\\\":false")
            && diagnostics.contains("\\\"base_image\\\":null"),
        "workspace build source diagnostic missing: {diagnostics}"
    );
}

#[test]
fn dockerfile_secret_detection_only_requests_github_token_when_used() {
    assert!(!dockerfile_body_requests_github_token_secret(
        "FROM projectjackin/construct:0.1-trixie\nRUN echo no secrets\n"
    ));
    assert!(!dockerfile_body_requests_github_token_secret(
        "FROM projectjackin/construct:0.1-trixie\n# RUN --mount=type=secret,id=github_token git ls-remote https://github.com/example/private\n"
    ));
    assert!(dockerfile_body_requests_github_token_secret(
        "FROM projectjackin/construct:0.1-trixie\nRUN --mount=type=secret,id=github_token git ls-remote https://github.com/example/private\n"
    ));
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
async fn prepare_runtime_binaries_for_agents_skips_sibling_runtime_prep() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    jackin_image::agent_binary::install_test_stub(&paths, Agent::Claude).unwrap();
    capsule_binary::install_test_stub(&paths).unwrap();
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(cached_repo.repo_dir.join(".git")).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        TEST_DOCKERFILE_FROM,
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

    let prepared =
        prepare_runtime_binaries_for_agents(&paths, &validated_repo, &[Agent::Claude], None)
            .await
            .unwrap();

    assert!(prepared.agent_installs.contains_key(&Agent::Claude));
    assert!(!prepared.agent_installs.contains_key(&Agent::Kimi));
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("ensure_claude_binary"),
        "selected agent binary prep should be timed: {diagnostics}"
    );
    assert!(
        !diagnostics.contains("ensure_kimi_binary"),
        "sibling runtime prep must not run on selected-agent foreground path: {diagnostics}"
    );
    assert!(
        diagnostics.contains("ensure_capsule_binary"),
        "capsule prep remains required for the role entrypoint: {diagnostics}"
    );
}

#[tokio::test]
async fn sibling_runtime_prewarm_runs_in_background() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    jackin_image::agent_binary::install_test_stub(&paths, Agent::Kimi).unwrap();
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(cached_repo.repo_dir.join(".git")).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        TEST_DOCKERFILE_FROM,
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

    spawn_sibling_runtime_prewarm(&paths, &validated_repo, Agent::Claude, true);

    for _ in 0..20 {
        let diagnostics = std::fs::read_to_string(run.path()).unwrap();
        if diagnostics.contains("\"kind\":\"runtime_prewarm_done\"") {
            assert!(diagnostics.contains("prewarming sibling runtime binaries"));
            assert!(diagnostics.contains("\"kind\":\"launch_plan\""));
            assert!(diagnostics.contains("PrewarmOnly"));
            assert!(diagnostics.contains("sibling_runtime_prewarm:kimi"));
            assert!(diagnostics.contains("ensure_kimi_binary"));
            assert!(diagnostics.contains("prefetched=1"));
            assert!(diagnostics.contains("fallback=0"));
            assert!(diagnostics.contains("versions=0"));
            assert!(!diagnostics.contains("ensure_claude_binary"));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!(
        "sibling runtime prewarm did not finish: {}",
        std::fs::read_to_string(run.path()).unwrap()
    );
}

#[tokio::test]
async fn sibling_runtime_prewarm_skips_after_selected_image_rebuild() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    jackin_image::agent_binary::install_test_stub(&paths, Agent::Kimi).unwrap();
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(cached_repo.repo_dir.join(".git")).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        TEST_DOCKERFILE_FROM,
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

    spawn_sibling_runtime_prewarm(&paths, &validated_repo, Agent::Claude, false);

    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"runtime_prewarm_skipped\"")
            && diagnostics.contains("selected image was rebuilt")
            && !diagnostics.contains("ensure_kimi_binary"),
        "cold selected-image builds should not start sibling binary work before attach: {diagnostics}"
    );
}

#[tokio::test]
async fn sibling_image_prewarm_skips_when_no_sibling_agents() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

    spawn_sibling_image_prewarm(
        &paths,
        &selector,
        "https://github.com/example/agent-smith.git",
        None,
        &validated_repo,
        Agent::Claude,
        true,
    );

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"sibling_image_prewarm_skipped\"")
            && diagnostics.contains("no sibling runtime images to prewarm"),
        "single-agent roles should not spawn image prewarm work: {diagnostics}"
    );
}

#[tokio::test]
async fn sibling_image_prewarm_skips_after_selected_image_rebuild() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(cached_repo.repo_dir.join(".git")).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        TEST_DOCKERFILE_FROM,
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

    spawn_sibling_image_prewarm(
        &paths,
        &selector,
        "https://github.com/example/agent-smith.git",
        None,
        &validated_repo,
        Agent::Claude,
        false,
    );

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"sibling_image_prewarm_skipped\"")
            && diagnostics.contains("selected image was rebuilt"),
        "cold selected-image builds should not start sibling image work before attach: {diagnostics}"
    );
}

#[test]
fn selected_image_refresh_records_test_skip_with_reason() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");

    spawn_selected_image_refresh(
        &paths,
        &selector,
        "https://example.invalid/agent-smith.git",
        None,
        Agent::Claude,
        ImageInvalidationReason::PublishedImageStale,
        false,
    );

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"selected_image_refresh_skipped\"")
            && diagnostics.contains("selected image refresh disabled in unit tests")
            && diagnostics.contains("claude:published_image_stale"),
        "selected refresh decision should be visible in test diagnostics: {diagnostics}"
    );
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
command: docker buildx build .

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

#[test]
fn parse_buildkit_duration_ms_handles_fraction_shapes() {
    assert_eq!(parse_buildkit_duration_ms("9s"), Some(9000));
    assert_eq!(parse_buildkit_duration_ms("9.2s"), Some(9200));
    assert_eq!(parse_buildkit_duration_ms("9.23s"), Some(9230));
    assert_eq!(parse_buildkit_duration_ms("9.234s"), Some(9234));
    assert_eq!(parse_buildkit_duration_ms("9.2349s"), Some(9234));
    assert_eq!(parse_buildkit_duration_ms("9ms"), None);
    assert_eq!(parse_buildkit_duration_ms("abc"), None);
}

#[test]
fn compact_image_warning_line_is_not_debug_prefixed() {
    let line = compact_image_warning_line("docker pull image failed");
    assert_eq!(line, "jackin: warning: docker pull image failed");
    assert!(!line.contains("[jackin debug"));
}

#[tokio::test]
async fn published_image_fresh_when_sha_matches() {
    let docker = make_docker([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(!stale, "matching SHA should report image as fresh");
}

#[tokio::test]
async fn published_image_stale_when_sha_differs() {
    let docker = make_docker([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "oldsha".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("newsha"), &docker).await;
    assert!(stale, "mismatched SHA should report image as stale");
}

#[tokio::test]
async fn published_image_stale_when_sha_label_missing_and_sha_known() {
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.1".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(stale, "known role SHA requires a matching SHA label");
}

#[tokio::test]
async fn published_image_falls_back_to_construct_version_when_sha_unknown() {
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.1".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", None, &docker).await;
    assert!(
        !stale,
        "matching construct version should be fresh before role SHA is known"
    );
}

#[tokio::test]
async fn published_image_stale_when_construct_version_differs() {
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.0".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(
        stale,
        "outdated construct version should report image as stale"
    );
}

#[tokio::test]
async fn published_image_stale_when_no_labels_and_sha_known() {
    let docker = make_docker(HashMap::new());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(stale, "known role SHA requires a matching SHA label");
}

#[tokio::test]
async fn published_image_stale_when_pull_fails() {
    let docker = FakeDockerClient {
        fail_with: vec![("docker pull".to_owned(), "network error".to_owned())],
        ..FakeDockerClient::default()
    };
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(stale, "pull failure should report image as stale");
}

#[tokio::test]
async fn published_image_stale_when_inspect_image_labels_fails() {
    let docker = FakeDockerClient {
        fail_with: vec![(
            "docker inspect image:".to_owned(),
            "daemon error".to_owned(),
        )],
        ..FakeDockerClient::default()
    };
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc"), &docker).await;
    assert!(
        stale,
        "inspect_image_labels failure should treat image as stale"
    );
}

#[test]
fn local_role_base_reuse_accepts_sha_only_published_labels() {
    let labels = HashMap::from([(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned())]);
    assert!(local_role_base_labels_match(
        &labels,
        "projectjackin/construct:trixie",
        "0.1-trixie",
        Some("abc123"),
    ));
}

#[test]
fn local_role_base_reuse_rejects_stale_construct_label() {
    let labels = HashMap::from([
        (LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "abc123".to_owned()),
        (
            LABEL_IMAGE_CONSTRUCT.to_owned(),
            "projectjackin/construct:old".to_owned(),
        ),
    ]);
    assert!(!local_role_base_labels_match(
        &labels,
        "projectjackin/construct:trixie",
        "0.1-trixie",
        Some("abc123"),
    ));
}

fn validated_test_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> (CachedRepo, jackin_manifest::repo::ValidatedRoleRepo) {
    let cached_repo = CachedRepo::new(paths, selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    (cached_repo, validated_repo)
}

#[test]
fn image_recipe_canonicalizes_supported_agent_order() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "kimi"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let claude_first = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let claude_first_labels = image_recipe_label_map_for_test(
        &cached_repo,
        &claude_first,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );

    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["kimi", "claude"]

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();
    let kimi_first = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let kimi_first_labels = image_recipe_label_map_for_test(
        &cached_repo,
        &kimi_first,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );

    assert_eq!(
        claude_first_labels.get(LABEL_IMAGE_RECIPE_HASH),
        kimi_first_labels.get(LABEL_IMAGE_RECIPE_HASH),
        "recipe hash should be stable for same supported-agent set"
    );
}

#[test]
fn image_recipe_accepts_script_fallback_install_recipe() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_install_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
        AgentInstall::ScriptFallback,
    );
    let expected = expected_image_recipes(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        &paths,
        &image_name(&selector, None),
    )
    .unwrap();

    assert_eq!(classify_image_labels(&labels, &expected), None);
}

#[test]
fn image_recipe_is_agent_independent() {
    // The recipe (and thus the image identity) keys on the supported-agent set,
    // never the selected agent — so the same role yields one recipe hash
    // regardless of which agent is launched. Selecting a different initial agent
    // must reuse the warm image instead of forking a redundant one.
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);

    let labels_claude = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let expected_codex = expected_image_recipes(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        &paths,
        &image_name(&selector, None),
    )
    .unwrap();

    // Labels written while launching Claude satisfy the recipe expected when
    // launching Codex — one image, reused across agents.
    assert_eq!(classify_image_labels(&labels_claude, &expected_codex), None);
}

#[test]
fn image_label_classifier_reports_precise_invalidation_reasons() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let expected_hash = expected.hash.clone();

    let labels = HashMap::new();
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::MissingRecipeLabel)
    );

    let labels = [(LABEL_IMAGE_RECIPE_VERSION.to_owned(), "future".to_owned())].into();
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::RecipeVersionChanged)
    );

    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(LABEL_IMAGE_RECIPE_HASH.to_owned(), "old".to_owned());
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::RecipeHashChanged)
    );

    let labels = [
        (LABEL_IMAGE_RECIPE_VERSION.to_owned(), "v1".to_owned()),
        (LABEL_IMAGE_RECIPE_HASH.to_owned(), expected_hash.clone()),
    ]
    .into();
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::RecipeVersionChanged)
    );

    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(
        LABEL_IMAGE_CONSTRUCT.to_owned(),
        "projectjackin/old-construct:latest".to_owned(),
    );
    let expected = expected_image_recipe_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    assert_eq!(
        classify_image_labels(&labels, &[expected]),
        Some(ImageInvalidationReason::ConstructImageChanged)
    );

    // Only the minimal kept labels report a precise, component-specific reason.
    // Every other recipe input now invalidates via the master recipe hash
    // (RecipeHashChanged) — see `recipe_diagnostic_labels`.
    for (label, reason) in [
        (
            LABEL_IMAGE_ROLE_GIT_SHA,
            ImageInvalidationReason::RoleGitShaChanged,
        ),
        (
            LABEL_IMAGE_MANIFEST_VERSION,
            ImageInvalidationReason::ManifestVersionChanged,
        ),
        (
            LABEL_IMAGE_CAPSULE_VERSION,
            ImageInvalidationReason::CapsuleVersionChanged,
        ),
    ] {
        let mut labels = image_recipe_label_map_for_test(
            &cached_repo,
            &validated_repo,
            Agent::Claude,
            Some("abc123"),
            None,
            None,
            "0",
        );
        labels.insert(label.to_owned(), "stale".to_owned());
        let expected = expected_image_recipe_for_test(
            &cached_repo,
            &validated_repo,
            Agent::Claude,
            Some("abc123"),
            None,
            None,
            "0",
        );
        assert_eq!(
            classify_image_labels(&labels, &[expected]),
            Some(reason),
            "{label} mismatch should report the precise invalidation reason"
        );
    }
}

#[tokio::test]
async fn decide_agent_image_reuses_when_recipe_labels_match() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _guard = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, Some("abc123"))]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::Reuse {
            image: image_name(&selector, Some("abc123")),
        }
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_hit\"")
            && diagnostics.contains("reusing derived image")
            && diagnostics.contains("recipe_hash_match")
            && diagnostics.contains("prepare_runtime_binaries")
            && diagnostics.contains("selected_agent_version_probe"),
        "reuse decision must be visible in diagnostics: {diagnostics}"
    );
}

#[tokio::test]
async fn decide_agent_image_rebuilds_on_legacy_or_mismatched_recipe_labels() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);

    let base_labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );

    let cases = [
        (
            "missing recipe version",
            {
                let mut labels = base_labels.clone();
                labels.remove(LABEL_IMAGE_RECIPE_VERSION);
                labels
            },
            ImageInvalidationReason::MissingRecipeLabel,
        ),
        (
            "legacy recipe version",
            {
                let mut labels = base_labels.clone();
                labels.insert(LABEL_IMAGE_RECIPE_VERSION.to_owned(), "v1".to_owned());
                labels
            },
            ImageInvalidationReason::RecipeVersionChanged,
        ),
        (
            "stale construct",
            {
                let mut labels = base_labels.clone();
                labels.insert(
                    LABEL_IMAGE_CONSTRUCT.to_owned(),
                    "projectjackin/old-construct:latest".to_owned(),
                );
                labels
            },
            ImageInvalidationReason::ConstructImageChanged,
        ),
    ];

    for (name, labels, expected_reason) in cases {
        let image = image_name(&selector, None);
        let docker = FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![image.clone()]);
        docker
            .inspect_image_labels_queue
            .borrow_mut()
            .push_back(labels.clone());
        let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

        let decision = decide_role_image(
            &paths,
            &selector,
            &cached_repo,
            &validated_repo,
            false,
            None,
            None,
            &docker,
            &mut runner,
        )
        .await
        .unwrap();
        match decision {
            ImageDecision::BuildFromWorkspace {
                reason,
                role_git_sha,
            } => {
                assert_eq!(
                    reason, expected_reason,
                    "case '{name}' should emit targeted invalidation reason"
                );
                assert_eq!(role_git_sha, Some("abc123".to_owned()));
            }
            ImageDecision::Reuse { .. } => {
                panic!("case '{name}' should rebuild but decided reuse");
            }
            ImageDecision::BuildFromPublished { .. } => {
                panic!("case '{name}' should rebuild from workspace but chose published image");
            }
            ImageDecision::RefreshInBackground { .. } => {
                panic!("case '{name}' should rebuild from workspace but chose background refresh");
            }
        }
    }
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    for reason in [
        ImageInvalidationReason::MissingRecipeLabel,
        ImageInvalidationReason::RecipeVersionChanged,
        ImageInvalidationReason::ConstructImageChanged,
    ] {
        assert!(
            diagnostics.contains("\"kind\":\"image_cache_miss\"")
                && diagnostics.contains(reason.as_str()),
            "diagnostics must explain rebuild reason {}: {diagnostics}",
            reason.as_str()
        );
    }
}

#[tokio::test]
async fn decide_agent_image_builds_when_local_image_missing_without_inspecting_labels() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _guard = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::LocalImageMissing,
            role_git_sha: None,
        }
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains("docker inspect image:")),
        "missing local tag must not consume inspect-label state"
    );
    // The role-repo HEAD SHA is now resolved up front because it is the image
    // tag (`jk_<role>:<sha>`) the lookup keys on. On the missing-image path that
    // single git capture is the only command that runs — no label inspection.
    assert!(
        runner.recorded.iter().all(|c| c.contains("rev-parse HEAD")),
        "missing-image path should run only the role-SHA git capture; got: {:?}",
        runner.recorded
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_miss\"")
            && diagnostics.contains("local_image_missing"),
        "build decision must include invalidation reason in diagnostics: {diagnostics}"
    );
}

#[tokio::test]
async fn decide_agent_image_builds_from_published_when_declared_image_is_missing() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromPublished {
            reason: ImageInvalidationReason::LocalImageMissing,
            role_git_sha: None,
            base_image: "docker.io/myorg/my-role:latest".to_owned(),
        }
    );
    // HEAD SHA is resolved up front (it is the image tag); the only command on
    // the missing-image path is that git capture.
    assert!(
        runner.recorded.iter().all(|c| c.contains("rev-parse HEAD")),
        "missing-image path should run only the role-SHA git capture; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn decide_agent_image_builds_from_workspace_when_published_image_is_stale() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let docker = FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            "old-sha".to_owned(),
        )]));
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::PublishedImageStale,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "published image freshness must be checked before binary prep: {recorded:?}"
    );
}

#[tokio::test]
async fn decide_agent_image_refreshes_background_when_workspace_image_is_valid_but_published_stale()
{
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let image = image_name(&selector, Some("abc123"));
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            "old-sha".to_owned(),
        )]));
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::RefreshInBackground {
            image,
            reason: ImageInvalidationReason::PublishedImageStale,
        }
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_hit\"")
            && diagnostics.contains("\"kind\":\"image_refresh_background\"")
            && diagnostics.contains("published_image_stale"),
        "background refresh decision should explain foreground reuse: {diagnostics}"
    );
}

#[tokio::test]
async fn prewarm_reuse_emits_prewarm_launch_plan_and_skips_build() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
    let _active = run.activate();
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = image_name(&selector, Some("abc123"));
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    #[expect(
        clippy::disallowed_methods,
        reason = "test opens the role lock file directly to pass a real File handle to the prewarm helper"
    )]
    let repo_lock = std::fs::File::open(cached_repo.repo_dir.join("jackin.role.toml")).unwrap();
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let row = prewarm_agent_image_from_validated_repo(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        None,
        Agent::Claude,
        &docker,
        &mut runner,
        repo_lock,
        false,
    )
    .await
    .unwrap();

    assert_eq!(row.status, ImagePrewarmStatus::Reused);
    assert_eq!(row.image, image);
    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("docker buildx build "),
        "valid prewarm image should skip expensive build path; recorded:\n{recorded}"
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"launch_plan\"")
            && diagnostics.contains("PrewarmOnly")
            && diagnostics.contains("image_reuse:recipe_hash_match"),
        "explicit image prewarm reuse should emit a typed PrewarmOnly launch plan: {diagnostics}"
    );
}

#[tokio::test]
async fn prewarm_refreshes_stale_published_base_when_local_workspace_image_is_valid() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
    let _active = run.activate();
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = image_name(&selector, Some("abc123"));
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            "old-sha".to_owned(),
        )]));
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            LABEL_IMAGE_CONSTRUCT.to_owned(),
            jackin_manifest::repo_contract::construct_image(),
        )]));
    #[expect(
        clippy::disallowed_methods,
        reason = "test opens the role lock file directly to pass a real File handle to the prewarm helper"
    )]
    let repo_lock = std::fs::File::open(cached_repo.repo_dir.join("jackin.role.toml")).unwrap();
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned(), "abc123".to_owned()]);

    let row = prewarm_agent_image_from_validated_repo(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        None,
        Agent::Claude,
        &docker,
        &mut runner,
        repo_lock,
        false,
    )
    .await
    .unwrap();

    assert_eq!(row.status, ImagePrewarmStatus::Built);
    assert_eq!(row.image, image);
    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker buildx build "),
        "explicit/background prewarm should rebuild refresh decisions; recorded:\n{recorded}"
    );
    assert!(
        recorded.contains("--label jackin.image.recipe.hash="),
        "refreshed image must keep stable recipe labels; recorded:\n{recorded}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "prewarm must check published image freshness before refresh: {docker_recorded:?}"
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"launch_plan\"")
            && diagnostics.contains("PrewarmOnly")
            && diagnostics.contains("image_refresh:published_image_stale"),
        "explicit image prewarm refresh should emit a typed PrewarmOnly launch plan: {diagnostics}"
    );
}

#[tokio::test]
async fn hook_content_change_invalidates_image_recipe() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = CachedRepo::new(&paths, &selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::create_dir_all(cached_repo.repo_dir.join("hooks")).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("hooks/preflight.sh"),
        "echo old\n",
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
preflight = "hooks/preflight.sh"
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    std::fs::write(
        cached_repo.repo_dir.join("hooks/preflight.sh"),
        "echo new\n",
    )
    .unwrap();

    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, None)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[tokio::test]
async fn branch_override_uses_branch_tag_and_recipe_ref() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let branch = "feat/instant-launch";
    // The runner returns HEAD SHA `abc123`, so the reuse lookup keys on the
    // commit-tagged branch image (`jk_agent-smith_feat-instant-launch:abc123`).
    let image = image_name_for_branch(&selector, branch, Some("abc123"));
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        Some(branch),
        None,
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        Some(branch),
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(decision, ImageDecision::Reuse { image });
}

#[tokio::test]
async fn decide_agent_image_rebuilds_when_construct_image_label_has_changed() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _guard = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(
        LABEL_IMAGE_CONSTRUCT.to_owned(),
        "projectjackin/custom-construct:latest".to_owned(),
    );
    let image = image_name(&selector, None);
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::ConstructImageChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );

    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_miss\"")
            && diagnostics.contains("construct_image_changed"),
        "construct label mismatch should be explained in diagnostics: {diagnostics}"
    );
}

#[tokio::test]
async fn decide_agent_image_rebuilds_when_role_git_sha_has_changed() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    labels.insert(LABEL_IMAGE_ROLE_GIT_SHA.to_owned(), "old-sha".to_owned());
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, None)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::RoleGitShaChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[tokio::test]
async fn decide_agent_image_rebuilds_when_role_source_ref_has_changed() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        Some("main"),
        None,
        "0",
    );
    let image = image_name_for_branch(&selector, "feature/instant-launch", None);
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        Some("feature/instant-launch"),
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some("abc123".to_owned()),
        }
    );
}

#[tokio::test]
async fn decide_agent_image_reuses_when_host_uid_matches_recipe() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, Some("abc123"))]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::Reuse {
            image: image_name(&selector, Some("abc123")),
        }
    );
}

#[test]
fn host_uid_changes_recipe_hash() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let mut first = build_image_recipe(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        "0",
    )
    .unwrap();
    let mut second = first.clone();
    first.host_uid = Some(501);
    second.host_uid = Some(1000);

    assert_ne!(
        first.hash().unwrap(),
        second.hash().unwrap(),
        "host UID must participate in the derived image recipe hash"
    );
}

#[test]
fn custom_construct_identity_changes_recipe_hash() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let canonical = build_image_recipe_with_construct_image(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        "0",
        jackin_manifest::repo_contract::CONSTRUCT_IMAGE.to_owned(),
    )
    .unwrap();
    let custom = build_image_recipe_with_construct_image(
        &cached_repo,
        &validated_repo,
        Some("abc123"),
        None,
        None,
        "0",
        "localhost/projectjackin-construct:test".to_owned(),
    )
    .unwrap();

    assert_ne!(
        canonical.hash().unwrap(),
        custom.hash().unwrap(),
        "construct image identity must participate in the recipe hash"
    );
}

#[tokio::test]
async fn decide_agent_image_rebuild_reason_is_emitted_in_diagnostics() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _guard = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo) = validated_test_repo(&paths, &selector);
    let mut labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        Agent::Claude,
        Some("abc123"),
        None,
        None,
        "0",
    );
    // Tamper the master recipe hash to force a mismatch. Component inputs
    // (hooks, plugins, etc.) no longer carry their own labels — they invalidate
    // through this hash, surfacing as the generic `recipe_hash_changed` reason.
    labels.insert(
        LABEL_IMAGE_RECIPE_HASH.to_owned(),
        "this-is-a-stale-recipe-hash".to_owned(),
    );
    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image_name(&selector, None)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_role_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        false,
        None,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    match decision {
        ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::RecipeHashChanged,
            role_git_sha: Some(sha),
        } => {
            assert_eq!(sha, "abc123");
        }
        _ => panic!("expected recipe-hash mismatch to trigger RecipeHashChanged"),
    }
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_miss\"")
            && diagnostics.contains("recipe_hash_changed"),
        "rebuild decision should be explained in diagnostics: {diagnostics}"
    );
}
