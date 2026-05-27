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

const FORCE_BOUNDARY_RITUALS_ENV: &str = "JACKIN_FORCE_BOUNDARY_RITUALS";
const FORCE_BOUNDARY_INTRO_ENV: &str = "JACKIN_FORCE_BOUNDARY_INTRO";
const FORCE_BOUNDARY_OUTRO_ENV: &str = "JACKIN_FORCE_BOUNDARY_OUTRO";

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

fn env_flag_enabled(value: Option<impl AsRef<std::ffi::OsStr>>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Some(value) = value.as_ref().to_str() else {
        return true;
    };
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

fn force_boundary_rituals_enabled() -> bool {
    env_flag_enabled(std::env::var_os(FORCE_BOUNDARY_RITUALS_ENV))
}

#[must_use]
pub fn force_boundary_intro_enabled() -> bool {
    force_boundary_rituals_enabled() || env_flag_enabled(std::env::var_os(FORCE_BOUNDARY_INTRO_ENV))
}

#[must_use]
pub fn force_boundary_outro_enabled() -> bool {
    force_boundary_rituals_enabled() || env_flag_enabled(std::env::var_os(FORCE_BOUNDARY_OUTRO_ENV))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitClaim {
    Missing,
    Claimed { elapsed: Option<Duration> },
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
/// no pending claim exists for an already-starting launch.
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
    let kind = if wrote_claim && pending_count <= 1 {
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

/// Claim the construct-exit boundary.
///
/// The marker is the single-consumer close claim: whichever exit path removes
/// it is the one that may render the rich outro. A malformed marker still
/// grants the claim, but omits the elapsed line from the caption.
#[must_use]
pub fn take_exit_claim(paths: &JackinPaths) -> ExitClaim {
    let file = marker_path(paths);
    // The rename is the claim, not the read: `rename` is atomic on POSIX, so
    // when parallel exits race only one can move the marker away — the losers
    // see ENOENT and bow out. A read-then-remove would let every racer observe
    // the marker first and render a duplicate outro.
    let claimed = file.with_file_name(format!("universe-since.claim.{}", std::process::id()));
    if let Err(error) = std::fs::rename(&file, &claimed) {
        // NotFound is the normal "no marker / already claimed" path. Any other
        // errno (e.g. a permissions drift on the data dir) is unexpected and
        // would silently suppress the outro, so leave a breadcrumb under
        // --debug to tell the two cases apart.
        if error.kind() != std::io::ErrorKind::NotFound {
            crate::debug_log!("universe", "exit-claim rename failed: {error}");
        }
        return ExitClaim::Missing;
    }
    let content = std::fs::read_to_string(&claimed).unwrap_or_default();
    let _ = std::fs::remove_file(&claimed);
    let _ = std::fs::remove_dir_all(pending_dir(paths));
    let elapsed = content
        .trim()
        .parse::<u128>()
        .ok()
        .and_then(|started| now_millis().checked_sub(started))
        .map(|elapsed_ms| Duration::from_millis(u64::try_from(elapsed_ms).unwrap_or(u64::MAX)));
    ExitClaim::Claimed { elapsed }
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

        let ExitClaim::Claimed {
            elapsed: Some(elapsed),
        } = take_exit_claim(&paths)
        else {
            panic!("elapsed claim available");
        };
        assert!(
            elapsed < Duration::from_secs(5),
            "just-started span is small"
        );
        assert!(!marker_path(&paths).exists(), "marker cleared after take");
        assert_eq!(
            take_exit_claim(&paths),
            ExitClaim::Missing,
            "second take is empty"
        );
    }

    #[test]
    fn env_flag_falsey_values_are_disabled() {
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("false"),
            Some("no"),
            Some("off"),
        ] {
            assert!(
                !env_flag_enabled(value),
                "value should be falsey: {value:?}"
            );
        }
    }

    #[test]
    fn env_flag_truthy_values_are_enabled() {
        for value in [Some("1"), Some("true"), Some("yes"), Some("anything")] {
            assert!(env_flag_enabled(value), "value should be truthy: {value:?}");
        }
    }

    #[test]
    fn exit_claim_is_single_consumer() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        mark_start(&paths, StartKind::FreshConstruct);

        assert!(matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. }));
        assert_eq!(
            take_exit_claim(&paths),
            ExitClaim::Missing,
            "second exit does not receive a duplicate outro claim"
        );
    }

    #[test]
    fn take_exit_claim_has_exactly_one_winner_under_contention() {
        use std::sync::{Arc, Barrier};

        let tmp = tempfile::tempdir().unwrap();
        let paths = Arc::new(JackinPaths::for_tests(tmp.path()));
        paths.ensure_base_dirs().unwrap();
        mark_start(&paths, StartKind::FreshConstruct);

        // All threads race the claim at once. The atomic rename guarantees a
        // single winner; a read-then-remove implementation would let several
        // threads read the marker and each render a duplicate outro.
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        // Collect eagerly so every thread is spawned before any is joined;
        // joining inside the spawn loop would serialize the race away.
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. })
            }));
        }

        let mut winners = 0;
        for handle in handles {
            if handle.join().unwrap() {
                winners += 1;
            }
        }
        assert_eq!(winners, 1, "exactly one exit may claim the outro");
    }

    #[test]
    fn take_exit_claim_leaves_no_claim_temp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        mark_start(&paths, StartKind::FreshConstruct);
        let _ = take_exit_claim(&paths);

        let leftover = std::fs::read_dir(&paths.data_dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("universe-since.claim.")
            });
        assert!(!leftover, "claim temp file must be removed after the take");
    }

    #[test]
    fn malformed_marker_still_grants_exit_claim_without_elapsed() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(marker_path(&paths), "not-a-timestamp").unwrap();

        let ExitClaim::Claimed { elapsed } = take_exit_claim(&paths) else {
            panic!("marker grants close claim");
        };
        assert_eq!(elapsed, None, "malformed marker omits elapsed caption");
        assert!(
            !marker_path(&paths).exists(),
            "claim clears malformed marker"
        );
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
    async fn claim_entry_treats_marker_without_running_containers_as_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(marker_path(&paths), "1000").unwrap();
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
            ..Default::default()
        };

        let claim = claim_entry(&paths, &docker).await;

        assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
        let kept = std::fs::read_to_string(marker_path(&paths)).unwrap();
        assert_ne!(kept, "1000", "stale launch marker is replaced");
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
