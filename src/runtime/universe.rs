//! Tracks how long the operator has been "in the construct".
//!
//! The span runs from the launch that brought the first container up to the
//! exit of the last one. A single marker file under the data dir holds the
//! start instant; the exit ritual reads and clears it to show elapsed time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::docker_client::DockerApi;
use crate::paths::JackinPaths;

static CLAIM_COUNTER: AtomicU64 = AtomicU64::new(0);

fn marker_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("universe-since")
}

fn pending_dir(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("universe-pending")
}

fn pending_path(paths: &JackinPaths, token: &str) -> PathBuf {
    pending_dir(paths).join(token)
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

fn claim_token() -> String {
    let counter = CLAIM_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{counter}", std::process::id(), now_millis())
}

/// Whether a launch enters an empty construct or joins one already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    /// No containers were running before this launch — (re)write the marker so
    /// the span starts now.
    FreshConstruct,
    /// A session is already ongoing — keep its original start instant.
    ResumeExisting,
}

/// A launch's claim on the construct-entry boundary.
///
/// Pending claims cover the short window before a role container exists. They
/// prevent concurrent launches from both playing the two-screen intro, and let
/// an early failed launch release only its own pending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryClaim {
    kind: StartKind,
    token: Option<String>,
}

impl EntryClaim {
    #[must_use]
    pub const fn start_kind(&self) -> StartKind {
        self.kind
    }

    #[must_use]
    const fn none(kind: StartKind) -> Self {
        Self { kind, token: None }
    }

    async fn release_if_idle(&self, paths: &JackinPaths, docker: &impl DockerApi) {
        let Some(token) = self.token.as_deref() else {
            return;
        };
        let _ = std::fs::remove_file(pending_path(paths, token));

        let Ok(running) = super::discovery::list_running_agent_names(docker).await else {
            return;
        };
        if running.is_empty() && !has_pending_claims(paths) {
            let _ = std::fs::remove_file(marker_path(paths));
            remove_empty_pending_dir(paths);
        }
    }
}

/// Claim the construct-entry boundary for an actual launch.
///
/// A fresh launch is one where Docker reports no running role containers and
/// no marker or pending claim exists for an already-starting launch.
pub async fn claim_entry(paths: &JackinPaths, docker: &impl DockerApi) -> EntryClaim {
    let Ok(names) = super::discovery::list_running_agent_names(docker).await else {
        return EntryClaim::none(StartKind::ResumeExisting);
    };
    if !names.is_empty() {
        mark_start(paths, StartKind::ResumeExisting);
        return EntryClaim::none(StartKind::ResumeExisting);
    }

    let token = claim_token();
    let wrote_claim = write_pending_claim(paths, &token);
    let pending_count = count_pending_claims(paths).unwrap_or(usize::MAX);
    let kind = if wrote_claim && pending_count <= 1 && !marker_path(paths).exists() {
        StartKind::FreshConstruct
    } else {
        StartKind::ResumeExisting
    };
    mark_start(paths, kind);
    EntryClaim {
        kind,
        token: wrote_claim.then_some(token),
    }
}

/// Record the construct's start instant. A `FreshConstruct` launch (re)writes
/// the marker to now; a `ResumeExisting` launch only writes it if absent, so an
/// ongoing session keeps its original start.
pub fn mark_start(paths: &JackinPaths, kind: StartKind) {
    let file = marker_path(paths);
    if kind == StartKind::ResumeExisting && file.exists() {
        return;
    }
    let _ = std::fs::write(&file, now_millis().to_string());
}

pub async fn release_entry_if_idle(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    claim: &EntryClaim,
) {
    claim.release_if_idle(paths, docker).await;
}

fn write_pending_claim(paths: &JackinPaths, token: &str) -> bool {
    let dir = pending_dir(paths);
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    std::fs::write(pending_path(paths, token), now_millis().to_string()).is_ok()
}

fn count_pending_claims(paths: &JackinPaths) -> Option<usize> {
    let dir = pending_dir(paths);
    if !dir.exists() {
        return Some(0);
    }
    Some(std::fs::read_dir(dir).ok()?.filter_map(Result::ok).count())
}

fn has_pending_claims(paths: &JackinPaths) -> bool {
    count_pending_claims(paths).is_none_or(|count| count > 0)
}

fn remove_empty_pending_dir(paths: &JackinPaths) {
    if !has_pending_claims(paths) {
        let _ = std::fs::remove_dir(pending_dir(paths));
    }
}

/// Read the construct's start instant, delete the marker, and return the
/// elapsed span. Returns `None` when no marker exists or it cannot be parsed
/// (the elapsed line is then simply omitted from the exit ritual).
#[must_use]
pub fn take_elapsed(paths: &JackinPaths) -> Option<Duration> {
    let file = marker_path(paths);
    let content = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir_all(pending_dir(paths));
    let started: u128 = content.trim().parse().ok()?;
    let elapsed_ms = now_millis().checked_sub(started)?;
    Some(Duration::from_millis(
        u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker_client::{ContainerRow, FakeDockerClient};
    use std::collections::{HashMap, VecDeque};

    #[test]
    fn mark_then_take_round_trips_and_clears() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        mark_start(&paths, StartKind::FreshConstruct);
        assert!(marker_path(&paths).exists(), "marker written");

        let elapsed = take_elapsed(&paths).expect("elapsed available");
        assert!(
            elapsed < Duration::from_secs(5),
            "just-started span is small"
        );
        assert!(!marker_path(&paths).exists(), "marker cleared after take");
        assert!(take_elapsed(&paths).is_none(), "second take is empty");
    }

    #[test]
    fn mark_non_fresh_preserves_existing_start() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(marker_path(&paths), "1000").unwrap();
        mark_start(&paths, StartKind::ResumeExisting); // must not overwrite
        let kept = std::fs::read_to_string(marker_path(&paths)).unwrap();
        assert_eq!(kept, "1000", "ongoing session keeps its original start");
    }

    #[tokio::test]
    async fn claim_entry_fresh_when_no_running_containers_or_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
            ..Default::default()
        };

        let claim = claim_entry(&paths, &docker).await;

        assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
        assert!(marker_path(&paths).exists(), "fresh claim writes marker");
        assert!(
            has_pending_claims(&paths),
            "fresh claim writes pending file"
        );
    }

    #[tokio::test]
    async fn claim_entry_resumes_when_container_running() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
                name: "jk-running".to_string(),
                labels: HashMap::new(),
            }]])),
            ..Default::default()
        };

        let claim = claim_entry(&paths, &docker).await;

        assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
        assert!(marker_path(&paths).exists(), "resume writes missing marker");
    }

    #[tokio::test]
    async fn claim_entry_resumes_when_marker_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(marker_path(&paths), "1000").unwrap();
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
            ..Default::default()
        };

        let claim = claim_entry(&paths, &docker).await;

        assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
        let kept = std::fs::read_to_string(marker_path(&paths)).unwrap();
        assert_eq!(kept, "1000", "existing launch marker is preserved");
    }

    #[tokio::test]
    async fn claim_entry_does_not_write_marker_when_container_list_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let docker = FakeDockerClient {
            fail_with: vec![("docker ps".to_string(), "daemon down".to_string())],
            ..Default::default()
        };

        let claim = claim_entry(&paths, &docker).await;

        assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
        assert!(
            !marker_path(&paths).exists(),
            "unknown Docker state must not claim the empty construct"
        );
    }

    #[tokio::test]
    async fn release_entry_clears_marker_when_no_instances_or_claims_remain() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![], vec![]])),
            ..Default::default()
        };

        let claim = claim_entry(&paths, &docker).await;
        release_entry_if_idle(&paths, &docker, &claim).await;

        assert!(
            !marker_path(&paths).exists(),
            "idle failed launch clears marker"
        );
        assert!(
            !has_pending_claims(&paths),
            "idle failed launch clears pending claim"
        );
    }

    #[tokio::test]
    async fn release_entry_keeps_marker_when_another_claim_remains() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([
                vec![],
                vec![],
                vec![],
                vec![],
            ])),
            ..Default::default()
        };

        let first = claim_entry(&paths, &docker).await;
        let second = claim_entry(&paths, &docker).await;
        release_entry_if_idle(&paths, &docker, &first).await;

        assert!(
            marker_path(&paths).exists(),
            "another pending launch keeps construct marker"
        );

        release_entry_if_idle(&paths, &docker, &second).await;

        assert!(
            !marker_path(&paths).exists(),
            "last pending launch clears marker"
        );
    }
}
