//! Tests for `image`.
use super::*;
use crate::runtime::test_support::{FakeDockerClient, FakeRunner};
use jackin_core::agent::Agent;
use std::collections::HashMap;
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
fn docker_build_env_forces_plain_buildkit_progress() {
    assert_eq!(
        docker_build_env(false),
        vec![("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned())]
    );
    assert_eq!(
        docker_build_env(true),
        vec![
            ("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned()),
            ("DOCKER_BUILDKIT".to_owned(), "1".to_owned()),
        ]
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
async fn published_image_falls_back_to_construct_version_when_no_sha_label() {
    // No SHA label; construct_version matches → fresh.
    let docker = make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(), "0.1".to_owned())].into());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(
        !stale,
        "matching construct version should be fresh when no SHA label"
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
async fn published_image_fresh_when_no_labels_at_all() {
    // No labels at all → backward-compat: fresh.
    let docker = make_docker(HashMap::new());
    let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
    assert!(
        !stale,
        "absent construct_version label should be treated as fresh (compat)"
    );
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

fn validated_test_repo(
    paths: &JackinPaths,
    selector: &RoleSelector,
) -> (
    CachedRepo,
    jackin_manifest::repo::ValidatedRoleRepo,
    HostIdentity,
) {
    let cached_repo = CachedRepo::new(paths, selector);
    crate::runtime::test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let host = HostIdentity {
        uid: "1000".to_owned(),
        gid: "1000".to_owned(),
    };
    (cached_repo, validated_repo, host)
}

#[test]
fn image_label_classifier_reports_precise_invalidation_reasons() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo, _host) = validated_test_repo(&paths, &selector);
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
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
    labels.insert(LABEL_IMAGE_SELECTED_AGENT.to_owned(), "codex".to_owned());
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
        Some(ImageInvalidationReason::SelectedAgentChanged)
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
        LABEL_IMAGE_RECIPE_BASE_IMAGE.to_owned(),
        "projectjackin/old:latest".to_owned(),
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
        Some(ImageInvalidationReason::BaseImageChanged)
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
        classify_image_labels(&labels, &[expected], Agent::Claude),
        Some(ImageInvalidationReason::ConstructImageChanged)
    );
}

#[tokio::test]
async fn decide_agent_image_reuses_when_recipe_labels_match() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _guard = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo, host) = validated_test_repo(&paths, &selector);
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
        .push_back(vec![image_name(&selector)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_agent_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        &host,
        Agent::Claude,
        false,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::Reuse {
            image: image_name(&selector)
        }
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_hit\"")
            && diagnostics.contains("reusing derived image")
            && diagnostics.contains("recipe_hash_match"),
        "reuse decision must be visible in diagnostics: {diagnostics}"
    );
}

#[tokio::test]
async fn decide_agent_image_builds_when_local_image_missing_without_inspecting_labels() {
    let _guard = rich_surface_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let _guard = run.activate();
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo, host) = validated_test_repo(&paths, &selector);
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let decision = decide_agent_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        &host,
        Agent::Claude,
        false,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::Build {
            reason: ImageInvalidationReason::LocalImageMissing
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
    assert!(
        runner.recorded.is_empty(),
        "missing local image should not even run git SHA capture"
    );
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"image_cache_miss\"")
            && diagnostics.contains("local_image_missing"),
        "build decision must include invalidation reason in diagnostics: {diagnostics}"
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
    let host = HostIdentity {
        uid: "1000".to_owned(),
        gid: "1000".to_owned(),
    };
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
        .push_back(vec![image_name(&selector)]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::with_capture_queue(["abc123".to_owned()]);

    let decision = decide_agent_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        &host,
        Agent::Claude,
        false,
        None,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(
        decision,
        ImageDecision::Build {
            reason: ImageInvalidationReason::HooksHashChanged
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
    let image = super::super::naming::image_name_for_branch(&selector, branch);
    let (cached_repo, validated_repo, host) = validated_test_repo(&paths, &selector);
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

    let decision = decide_agent_image(
        &paths,
        &selector,
        &cached_repo,
        &validated_repo,
        &host,
        Agent::Claude,
        false,
        Some(branch),
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(decision, ImageDecision::Reuse { image });
}

#[test]
fn custom_construct_identity_changes_recipe_hash() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let (cached_repo, validated_repo, host) = validated_test_repo(&paths, &selector);
    let canonical = build_image_recipe_with_construct_image(
        &cached_repo,
        &validated_repo,
        &host,
        Agent::Claude,
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
        &host,
        Agent::Claude,
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
