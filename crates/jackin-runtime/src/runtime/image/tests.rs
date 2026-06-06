//! Tests for `image`.
use super::*;
use crate::runtime::test_support::FakeDockerClient;
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
